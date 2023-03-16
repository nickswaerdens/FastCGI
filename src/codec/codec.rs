use bytes::{Buf, BufMut, BytesMut};

use tokio_util::codec::{Decoder, Encoder};

use crate::meta::{Discrete, Meta, Stream};
use crate::record::{
    Empty, EncodeFrame, Header, Id, Padding, Record, StreamFragment, DEFAULT_MAX_PAYLOAD_SIZE,
    HEADER_SIZE,
};
use crate::types::RecordType;
use crate::FCGI_VERSION_1;

use super::ring_buffer::RingBuffer;

/// Unparsed frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub(crate) header: Header,
    pub(crate) payload: BytesMut,
}

impl Frame {
    pub(crate) fn new(header: Header, payload: BytesMut) -> Self {
        Self { header, payload }
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn id(&self) -> Id {
        self.header.id
    }

    pub fn record_type(&self) -> RecordType {
        self.header.record_type
    }

    pub fn padding(&self) -> Option<Padding> {
        self.header.padding
    }

    pub fn into_parts(self) -> (Header, BytesMut) {
        (self.header, self.payload)
    }
}

#[derive(Debug)]
pub enum EncodeCodecError {
    MaxLengthExceeded,
    StdIoError(std::io::Error),
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
    pub fn new() -> Self {
        Self {
            buffer: RingBuffer::with_capacity(DEFAULT_MAX_PAYLOAD_SIZE + 1),
            state: DecodeState::Header,
        }
    }

    fn decode_header(src: &mut BytesMut) -> Result<Option<(Header, u16)>, DecodeCodecError> {
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

        let header = Header {
            id: u16::from_be_bytes(src[2..4].try_into().unwrap()),
            record_type: RecordType::from(src[1]),
            padding: Padding::from_u8(padding_length),
        };

        // Discard header from src.
        src.advance(HEADER_SIZE);

        // Grow the buffer for the expected data, plus padding.
        src.reserve(content_length as usize + padding_length as usize);

        Ok(Some((header, content_length)))
    }

    fn decode_data(&self, content_length: u16, src: &mut BytesMut) -> Option<BytesMut> {
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

/// A wrapper trait to allow traits with mutually exclusive associated types to implement the same trait.
///
/// In this case, it allows the compiler to differentiate between the two data kinds of a `Meta` type T.
///
/// This trait can be removed when https://github.com/rust-lang/rust/issues/20400 is stabilized.
pub(crate) trait EncoderSpecialization<T, R = <T as Meta>::DataKind> {
    fn specialized_encode(&mut self, item: T, dst: &mut BytesMut) -> Result<(), EncodeCodecError>;
}

// Write to an internal ring buffer before sending it down stream, as the content_length
// and padding_length need to be set after encoding. The data must not be sent before
// these values are set in the header.
impl<T> EncoderSpecialization<Record<T>, Discrete> for FastCgiCodec
where
    T: Meta<DataKind = Discrete> + EncodeFrame,
{
    fn specialized_encode(
        &mut self,
        item: Record<T>,
        dst: &mut BytesMut,
    ) -> Result<(), EncodeCodecError> {
        let (header, payload) = item.into_parts();

        // RingBuffer always has a capacity of ^2, which it can't have at
        // max content_length size + header_size, therefore, the header is
        // stored separately.
        let mut header_buf = Vec::with_capacity(8);
        header.encode_zeroed(&mut header_buf);

        //let payload_start = self.buffer.len();
        if payload.encode(&mut self.buffer).is_err() {
            self.buffer
                .set_position(self.buffer.remaining_read() as u64);
        }

        let content_length = self.buffer.remaining_read() as u16;
        let padding_length = header
            .padding
            .map_or_else(|| 0, |padding| padding.into_u8(content_length));

        header_buf[4..6].copy_from_slice(&content_length.to_be_bytes()[..2]);
        header_buf[6] = padding_length;

        dst.reserve(HEADER_SIZE + self.buffer.len() + padding_length as usize);

        dst.put_slice(&header_buf);
        dst.put(&mut self.buffer);
        dst.put_bytes(0, padding_length as usize);

        Ok(())
    }
}

// Stream record fragments are already encoded, so we can write to the internal `Framed` buffer directly.
impl<T> EncoderSpecialization<Record<StreamFragment<T>>, Stream> for FastCgiCodec
where
    T: Meta<DataKind = Stream>,
{
    fn specialized_encode(
        &mut self,
        item: Record<StreamFragment<T>>,
        dst: &mut BytesMut,
    ) -> Result<(), EncodeCodecError> {
        let (header, fragment) = item.into_parts();

        if fragment.inner.len() > DEFAULT_MAX_PAYLOAD_SIZE {
            return Err(EncodeCodecError::MaxLengthExceeded);
        }

        let padding_length = header
            .padding
            .map_or_else(|| 0, |padding| padding.into_u8(fragment.inner.len() as u16));

        dst.reserve(HEADER_SIZE + fragment.inner.len() + padding_length as usize);

        header.encode(fragment.inner.len() as u16, padding_length, dst);

        dst.put(fragment.inner);
        dst.put_bytes(0, padding_length as usize);

        Ok(())
    }
}

impl<T> EncoderSpecialization<Record<Empty<T>>, Stream> for FastCgiCodec
where
    T: Meta<DataKind = Stream>,
{
    fn specialized_encode(
        &mut self,
        item: Record<Empty<T>>,
        dst: &mut BytesMut,
    ) -> Result<(), EncodeCodecError> {
        let header = item.header();

        let padding_length = header
            .padding
            .map_or_else(|| 0, |padding| padding.into_u8(0));

        dst.reserve(HEADER_SIZE + padding_length as usize);

        header.encode(0, padding_length, dst);
        dst.put_bytes(0, padding_length as usize);

        Ok(())
    }
}

impl<T> Encoder<Record<T>> for FastCgiCodec
where
    T: Meta,
    Self: EncoderSpecialization<Record<T>, T::DataKind>,
{
    type Error = EncodeCodecError;

    fn encode(&mut self, item: Record<T>, dst: &mut BytesMut) -> Result<(), Self::Error> {
        self.specialized_encode(item, dst)
    }
}

impl Decoder for FastCgiCodec {
    type Item = Frame;
    type Error = DecodeCodecError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // Eat the padding at the end of the previous request.
        // This is done at the start instead of end to return the Frame ASAP.
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
        match self.decode_data(content_length, src) {
            Some(data) => {
                self.state = match header.padding {
                    Some(Padding::Static(n)) => DecodeState::Padding(n),
                    _ => DecodeState::Header,
                };

                src.reserve(HEADER_SIZE);

                // Padding is stripped during the decoding of frames.
                Ok(Some(Frame::new(header.without_padding(), data)))
            }
            None => Ok(None),
        }
    }
}
