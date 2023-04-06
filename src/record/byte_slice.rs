use std::marker::PhantomData;

use bytes::{BufMut, Bytes, BytesMut};

use crate::meta::{Meta, Stream};

use super::{DecodeFrame, DecodeFrameError, EncodeFragment, EncodeFrameError};

/// Contiguous byte slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteSlice<T> {
    bytes: Bytes,
    _marker: PhantomData<fn() -> T>,
}

impl<T> ByteSlice<T> {
    pub fn new(bytes: Bytes) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }

        Some(Self {
            bytes,
            _marker: PhantomData,
        })
    }

    /// Assumes `!bytes.is_empty()`.
    pub fn new_unchecked(bytes: Bytes) -> Self {
        Self {
            bytes,
            _marker: PhantomData,
        }
    }

    pub const fn from_static(bytes: &'static [u8]) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }

        Some(Self {
            bytes: Bytes::from_static(bytes),
            _marker: PhantomData,
        })
    }
}

impl<T> AsRef<Bytes> for ByteSlice<T> {
    fn as_ref(&self) -> &Bytes {
        &self.bytes
    }
}

impl<T> EncodeFragment for ByteSlice<T>
where
    ByteSlice<T>: Meta<DataKind = Stream>,
{
    fn encode_fragment(
        &mut self,
        buf: &mut bytes::buf::Limit<&mut BytesMut>,
    ) -> Option<Result<(), EncodeFrameError>> {
        if self.bytes.is_empty() {
            return None;
        }

        let n = buf.remaining_mut().min(self.bytes.len());

        buf.get_mut().reserve(n);
        buf.put(self.bytes.split_to(n));

        Some(Ok(()))
    }
}

impl<T> DecodeFrame for ByteSlice<T>
where
    ByteSlice<T>: Meta,
{
    fn decode(src: BytesMut) -> Result<ByteSlice<T>, DecodeFrameError> {
        Ok(Self::new(src.freeze()).unwrap())
    }
}
