mod buffer;
mod ring_buffer;

pub use buffer::*;
pub(crate) use ring_buffer::*;

use bytes::{Buf, BufMut, BytesMut};
use tokio_util::codec::{Decoder, Encoder};

use crate::meta::{self, Meta};
use crate::record::{
    EncodeChunk, EncodeFrame, EncodeFrameError, EndOfStream, Header, Id, Padding, Record,
    RecordType, StreamChunker, DEFAULT_MAX_PAYLOAD_SIZE, HEADER_SIZE,
};

/// Unparsed frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Frame {
    pub(crate) id: Id,
    pub(crate) record_type: RecordType,
    pub(crate) payload: BytesMut,
}

impl Frame {
    pub(crate) fn new(id: Id, record_type: RecordType, payload: BytesMut) -> Self {
        Self {
            id,
            record_type,
            payload,
        }
    }

    pub fn as_parts(&self) -> (Id, RecordType, &BytesMut) {
        (self.id, self.record_type, &self.payload)
    }

    pub fn into_parts(self) -> (Id, RecordType, BytesMut) {
        (self.id, self.record_type, self.payload)
    }
}

#[derive(Debug, Clone, Copy)]
enum DecodeState {
    Header,
    Payload((Header, u16)),
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
    pub(crate) fn new() -> Self {
        Self {
            buffer: RingBuffer::with_capacity(DEFAULT_MAX_PAYLOAD_SIZE + 1),
            state: DecodeState::Header,
        }
    }

    /// Encodes the header, the currently encoded record body, and the padding of a record.
    fn encode_record(&mut self, header: Header, dst: &mut BytesMut) {
        let content_length = self.buffer.remaining() as u16;
        let padding_length = header
            .padding
            .map_or(0, |padding| padding.into_u8(content_length));

        dst.reserve(HEADER_SIZE + content_length as usize + padding_length as usize);

        Header::encode(
            header.record_type,
            header.id,
            content_length,
            padding_length,
            dst,
        );

        dst.put(&mut self.buffer);
        dst.put_bytes(0, padding_length as usize);
    }

    /// Decodes a header and reserves space to fit the entire record body, including padding bytes.
    fn decode_header(src: &mut BytesMut) -> Result<Option<(Header, u16)>, DecodeCodecError> {
        if let Some((header, content_length, padding_length)) = Header::decode(src)? {
            // Grow the buffer for the expected data, plus padding.
            src.reserve(content_length as usize + padding_length as usize);

            Ok(Some((header, content_length)))
        } else {
            Ok(None)
        }
    }

    /// Extracts the body from the source.
    fn extract_body(content_length: u16, src: &mut BytesMut) -> Option<BytesMut> {
        if src.len() < content_length as usize {
            return None;
        }

        Some(src.split_to(content_length as usize))
    }

    /// Consumes n padding bytes from the source.
    fn consume_padding(n: u8, src: &mut BytesMut) -> Option<()> {
        if src.len() < n as usize {
            return None;
        }

        src.advance(n as usize);

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
        body.encode_frame(&mut self.buffer.write_only())
            .map_err(|err| {
                // Advance the read cursor past the invalid data.
                self.buffer.advance(self.buffer.remaining_read());

                EncodeCodecError::from(err)
            })?;

        self.encode_record(header, dst);

        Ok(())
    }
}

// Record<StreamChunker> is not moved, as it may contain data for additional chunks.
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
        let option = record
            .body
            .encode(&mut self.buffer.write_only())
            .transpose()
            .map_err(|err| {
                // Advance the read cursor past the invalid data.
                self.buffer.advance(self.buffer.remaining_read());

                EncodeCodecError::from(err)
            })?;

        // Encode either a full chunk, or the last chunk.
        if option.is_some() || self.buffer.remaining_read() > 0 {
            self.encode_record(record.header, dst);
        }

        Ok(())
    }
}

impl<T> Encoder<Record<EndOfStream<T>>> for FastCgiCodec
where
    T: Meta<DataKind = meta::Stream>,
{
    type Error = EncodeCodecError;

    fn encode(
        &mut self,
        record: Record<EndOfStream<T>>,
        dst: &mut BytesMut,
    ) -> Result<(), Self::Error> {
        self.encode_record(record.header, dst);

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
            match Self::consume_padding(skip, src) {
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
        match Self::extract_body(content_length, src) {
            Some(data) => {
                if let Some(Padding::Static(n)) = header.padding {
                    self.state = DecodeState::Padding(n);
                } else {
                    self.state = DecodeState::Header;
                }

                src.reserve(HEADER_SIZE);

                // Padding is stripped during the decoding of frames.
                Ok(Some(Frame::new(header.id, header.record_type, data)))
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
