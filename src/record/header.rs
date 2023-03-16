use bytes::BufMut;

use crate::{meta::Meta, types::RecordType, FCGI_VERSION_1};

pub const HEADER_SIZE: usize = 8;

pub type Id = u16;

/// Fastcgi header
///
/// Header is automatically set to pad frames to a multiple of 8 bytes as recommended by the spec.
/// This behavior can be changed by calling the relevant with/without methods on this struct.
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
    pub fn from_meta<T: Meta>(id: Id) -> Self {
        Self {
            id,
            record_type: T::TYPE,
            padding: Some(Padding::Automatic),
        }
    }

    /// Apply padding to this frame's payload based on the length of the payload.
    pub fn with_adaptive_padding(mut self, f: fn(u16) -> u8) -> Self {
        self.padding = Some(Padding::Adaptive(f));
        self
    }

    /// Apply a static amount padding to this frame's payload.
    pub fn with_static_padding(mut self, n: u8) -> Self {
        self.padding = Some(Padding::Static(n));
        self
    }

    /// Avoid adding padding to the frame's payload.
    pub fn without_padding(mut self) -> Self {
        self.padding = None;
        self
    }

    pub fn encode<B: BufMut>(self, content_length: u16, padding_length: u8, dst: &mut B) {
        dst.put_u8(FCGI_VERSION_1);
        dst.put_u8(self.record_type.into());
        dst.put_u16(self.id);
        dst.put_u16(content_length);
        dst.put_u8(padding_length);
        dst.put_u8(0);
    }

    pub fn encode_zeroed<B: BufMut>(self, dst: &mut B) {
        self.encode(0, 0, dst)
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
        if n == 0 {
            None
        } else {
            Some(Padding::Static(n))
        }
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
