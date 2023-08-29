mod frame;

use super::record::{EncodeRecordError, Header, HeaderDecoded, Padding, Record};
use crate::HEADER_SIZE;
use bytes::{Buf, BufMut, BytesMut};
pub(crate) use frame::*;
use tokio_util::codec::{Decoder, Encoder};

#[derive(Debug, Clone, Copy)]
enum DecodeState {
    Header,
    Payload((Header, u16)),
    Padding(u8),
}

#[derive(Debug)]
pub(crate) struct FastCgiCodec {
    state: DecodeState,
}

impl FastCgiCodec {
    pub(crate) fn new() -> Self {
        Self {
            state: DecodeState::Header,
        }
    }

    /// Decodes a header and reserves space to fit the entire record body, including padding bytes.
    fn decode_header(src: &mut BytesMut) -> Result<Option<(Header, u16)>, DecodeCodecError> {
        if let Some(HeaderDecoded {
            header,
            content_length,
            padding_length,
        }) = Header::decode(src)?
        {
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

impl Encoder<Record> for FastCgiCodec {
    type Error = std::io::Error;

    fn encode(&mut self, record: Record, dst: &mut BytesMut) -> Result<(), Self::Error> {
        let (header, body) = record.into_parts();

        let content_length = body.remaining() as u16;
        let padding_length = header
            .padding
            .map_or(0, |padding| padding.into_u8(content_length));

        dst.reserve(HEADER_SIZE as usize + content_length as usize + padding_length as usize);

        header.encode(content_length, padding_length, dst);
        dst.put(body);
        dst.put_bytes(0, padding_length as usize);

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

                src.reserve(HEADER_SIZE as usize);

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
    EncodeRecordError(EncodeRecordError),
    StdIoError(std::io::Error),
}

impl From<EncodeRecordError> for EncodeCodecError {
    fn from(value: EncodeRecordError) -> Self {
        EncodeCodecError::EncodeRecordError(value)
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
