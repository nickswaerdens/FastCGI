use bytes::{Buf, BufMut, BytesMut};

use crate::{codec::DecodeCodecError, FCGI_VERSION_1};

use super::RecordType;

pub const HEADER_SIZE: usize = 8;

pub type Id = u16;

/// Fastcgi header
///
/// Header is automatically set to pad frames to a multiple of 8 bytes as recommended by the spec.
/// This behavior can be changed by calling the relevant with/without methods on this struct.
///
/// The remaining header information is stored in the type of the body of `Record`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub(crate) id: Id,
    pub(crate) record_type: RecordType,
    pub(crate) padding: Option<Padding>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Padding {
    Automatic,
    Adaptive(fn(u16) -> u8),
    Static(u8),
}

impl Header {
    pub(crate) fn new(id: Id, record_type: RecordType) -> Self {
        Self {
            id,
            record_type,
            padding: Some(Padding::Automatic),
        }
    }

    pub fn with_padding(mut self, padding: Padding) -> Self {
        self.padding = Some(padding);
        self
    }

    /// Apply padding to this records's payload based on the length of the payload.
    pub fn with_adaptive_padding(mut self, f: fn(u16) -> u8) -> Self {
        self.padding = Some(Padding::Adaptive(f));
        self
    }

    /// Apply a static amount padding to this records's payload.
    pub fn with_static_padding(mut self, n: u8) -> Self {
        self.padding = Some(Padding::Static(n));
        self
    }

    /// Avoid adding padding to the records's payload.
    pub fn without_padding(mut self) -> Self {
        self.padding = None;
        self
    }

    pub fn encode<B: BufMut>(
        record_type: RecordType,
        id: u16,
        content_length: u16,
        padding_length: u8,
        dst: &mut B,
    ) {
        dst.put_u8(FCGI_VERSION_1);
        dst.put_u8(record_type.into());
        dst.put_u16(id);
        dst.put_u16(content_length);
        dst.put_u8(padding_length);
        dst.put_u8(0);
    }

    /// Returns a triple containing the header, content_length, and padding length.
    pub fn decode(src: &mut BytesMut) -> Result<Option<(Header, u16, u8)>, DecodeCodecError> {
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

        Ok(Some((header, content_length, padding_length)))
    }
}

impl Padding {
    fn pad_to_multiple_of_8(n: u16) -> u8 {
        // Avoid overflows when n approaches u16::MAX.
        let len = u32::from(n);

        // Pad to multiple of 8.
        (((len + 7) & !7) - len) as u8
    }

    pub fn from_u8(n: u8) -> Option<Padding> {
        (n > 0).then_some(Padding::Static(n))
    }

    pub fn into_u8(self, content_length: u16) -> u8 {
        match (self, content_length) {
            (Padding::Automatic, 0) => 0,
            (Padding::Automatic, n) => Self::pad_to_multiple_of_8(n),
            (Padding::Adaptive(f), n) => f(n),
            (Padding::Static(n), _) => n,
        }
    }
}
