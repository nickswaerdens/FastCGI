use bytes::{Buf, BufMut, BytesMut};

use tokio_util::codec::{Decoder, Encoder};

use crate::meta::{self, Meta};
use crate::record::{
    Empty, EncodeChunk, EncodeFrame, EncodeFrameError, Header, Id, Record, RecordType,
    StreamChunker, DEFAULT_MAX_PAYLOAD_SIZE, HEADER_SIZE,
};
use crate::FCGI_VERSION_1;

use super::ring_buffer::RingBuffer;

/// Partially parsed header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PartialHeader {
    pub(crate) id: Id,
    pub(crate) record_type: RecordType,
    pub(crate) padding_length: u8,
}

/// Unparsed frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Frame {
    pub(crate) header: PartialHeader,
    pub(crate) payload: BytesMut,
}

impl Frame {
    pub(crate) fn new(header: PartialHeader, payload: BytesMut) -> Self {
        Self { header, payload }
    }

    pub fn get_header(&self) -> &PartialHeader {
        &self.header
    }

    pub fn get_id(&self) -> Id {
        self.header.id
    }

    pub fn get_record_type(&self) -> RecordType {
        self.header.record_type
    }

    pub fn get_padding(&self) -> u8 {
        self.header.padding_length
    }

    pub fn as_parts(&self) -> (&PartialHeader, &BytesMut) {
        (&self.header, &self.payload)
    }

    pub fn into_parts(self) -> (PartialHeader, BytesMut) {
        (self.header, self.payload)
    }
}

#[derive(Debug, Clone, Copy)]
enum DecodeState {
    Header,
    Payload((PartialHeader, u16)),
    Padding(u8),
}

#[derive(Debug)]
pub(crate) struct FastCgiCodec {
    // Encode
    buffer: RingBuffer,

    // Decode
    state: DecodeState,
}

impl FastCgiCodec {
    pub fn new() -> Self {
        Self {
            buffer: RingBuffer::with_capacity(DEFAULT_MAX_PAYLOAD_SIZE + 1),
            state: DecodeState::Header,
        }
    }

    /// Encodes the header, the currently encoded record body, and the padding of a record.
    pub fn encode_record<T: Meta>(&mut self, header: Header, dst: &mut BytesMut) {
        let content_length = self.buffer.remaining() as u16;
        let padding_length = header
            .padding
            .map_or(0, |padding| padding.into_u8(content_length));

        dst.reserve(HEADER_SIZE + content_length as usize + padding_length as usize);

        header.encode::<T>(content_length, padding_length, dst);
        dst.put(&mut self.buffer);
        dst.put_bytes(0, padding_length as usize);
    }

    /// Decodes a header and reserves space to fit the entire record body, including padding bytes.
    pub fn decode_header(
        src: &mut BytesMut,
    ) -> Result<Option<(PartialHeader, u16)>, DecodeCodecError> {
        if src.len() < HEADER_SIZE {
            return Ok(None);
        }

        if src[0] != FCGI_VERSION_1 {
            return Err(DecodeCodecError::IncompatibleVersion);
        }

        if src[7] != 0 {
            return Err(DecodeCodecError::CorruptedHeader);
        }

        let content_length = u16::from_be_bytes(src[4..6].try_into().unwrap());
        let padding_length = src[6];

        let header = PartialHeader {
            id: u16::from_be_bytes(src[2..4].try_into().unwrap()),
            record_type: RecordType::from(src[1]),
            padding_length,
        };

        // Discard header from src.
        src.advance(HEADER_SIZE);

        // Grow the buffer for the expected data, plus padding.
        src.reserve(content_length as usize + padding_length as usize);

        Ok(Some((header, content_length)))
    }

    fn decode_body(content_length: u16, src: &mut BytesMut) -> Option<BytesMut> {
        if src.len() < content_length as usize {
            return None;
        }

        Some(src.split_to(content_length as usize))
    }

    fn skip_padding(skip: u8, src: &mut BytesMut) -> Option<()> {
        if src.len() < skip as usize {
            return None;
        }

        src.advance(skip as usize);

        Some(())
    }
}

impl<T> Encoder<Record<T>> for FastCgiCodec
where
    T: EncodeFrame,
{
    type Error = EncodeCodecError;

    fn encode(&mut self, record: Record<T>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let (header, body) = record.into_parts();

        // Write to an internal ring buffer before sending it down stream, as the content_length
        // and padding_length are unknown before encoding.
        body.encode(&mut self.buffer.write_only()).map_err(|err| {
            // Read past the invalid data.
            self.buffer.advance(self.buffer.remaining_read());

            EncodeCodecError::from(err)
        })?;

        self.encode_record::<T>(header, dst);

        Ok(())
    }
}

impl<'a, T> Encoder<&'a mut Record<StreamChunker<T>>> for FastCgiCodec
where
    T: EncodeChunk,
{
    type Error = EncodeCodecError;

    fn encode(
        &mut self,
        record: &'a mut Record<StreamChunker<T>>,
        dst: &mut BytesMut,
    ) -> Result<(), Self::Error> {
        // Write to an internal ring buffer before sending it down stream, as the content_length
        // and padding_length are unknown before encoding.
        if let Some(result) = record.body.encode_chunk(&mut self.buffer.write_only()) {
            result.map_err(|err| {
                // Set the read cursor past the invalid data.
                self.buffer.advance(self.buffer.remaining_read());

                EncodeCodecError::from(err)
            })?
        } else {
            if self.buffer.remaining_read() > 0 {
                // TODO: turn this print into a log.
                println!("Warning: data was ignored...");

                self.buffer.advance(self.buffer.remaining_read());
            }

            return Ok(());
        }

        self.encode_record::<T>(record.header, dst);

        Ok(())
    }
}

impl<T> Encoder<Record<Empty<T>>> for FastCgiCodec
where
    T: Meta<DataKind = meta::Stream>,
{
    type Error = EncodeCodecError;

    fn encode(&mut self, record: Record<Empty<T>>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        self.encode_record::<T>(record.header, dst);

        Ok(())
    }
}

// Flush
impl Encoder<()> for FastCgiCodec {
    type Error = EncodeCodecError;

    fn encode(&mut self, _: (), _: &mut BytesMut) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl Decoder for FastCgiCodec {
    type Item = Frame;
    type Error = DecodeCodecError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Eat the padding at the end of the previous request.
        // This is done at the start instead of end to return the previous Frame ASAP.
        if let DecodeState::Padding(skip) = self.state {
            match Self::skip_padding(skip, src) {
                Some(_) => self.state = DecodeState::Header,
                None => return Ok(None),
            }
        }

        // Decode the header, if the header was already decoded, return the
        // decoded value.
        let (header, content_length) = match self.state {
            DecodeState::Header => match Self::decode_header(src)? {
                Some(x) => {
                    self.state = DecodeState::Payload(x);
                    x
                }
                None => return Ok(None),
            },
            DecodeState::Payload(x) => x,
            _ => unreachable!(),
        };

        // Decode body and reserve space for the next header.
        match Self::decode_body(content_length, src) {
            Some(data) => {
                self.state = if header.padding_length > 0 {
                    DecodeState::Padding(header.padding_length)
                } else {
                    DecodeState::Header
                };

                src.reserve(HEADER_SIZE);

                // Padding is stripped during the decoding of frames.
                Ok(Some(Frame::new(header, data)))
            }
            None => Ok(None),
        }
    }
}

#[derive(Debug)]
pub enum EncodeCodecError {
    MaxLengthExceeded,
    EncodeFrameError(EncodeFrameError),
    StdIoError(std::io::Error),
}

impl From<EncodeFrameError> for EncodeCodecError {
    fn from(value: EncodeFrameError) -> Self {
        EncodeCodecError::EncodeFrameError(value)
    }
}

impl From<std::io::Error> for EncodeCodecError {
    fn from(value: std::io::Error) -> Self {
        EncodeCodecError::StdIoError(value)
    }
}

#[derive(Debug)]
pub enum DecodeCodecError {
    IncompatibleVersion,
    CorruptedHeader,
    StdIoError(std::io::Error),
}

impl From<std::io::Error> for DecodeCodecError {
    fn from(value: std::io::Error) -> Self {
        DecodeCodecError::StdIoError(value)
    }
}
