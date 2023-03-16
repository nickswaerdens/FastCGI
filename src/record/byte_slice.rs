use std::marker::PhantomData;

use bytes::{BufMut, Bytes, BytesMut};

use crate::meta::{Meta, Stream};

use super::{DecodeFrame, DecodeFrameError, StreamFragment, StreamFragmenter};

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

impl<T> Iterator for StreamFragmenter<ByteSlice<T>>
where
    ByteSlice<T>: Meta<DataKind = Stream>,
{
    type Item = StreamFragment<ByteSlice<T>>;

    fn next(&mut self) -> Option<Self::Item> {
        let (data, mut buffer) = self.parts();

        if data.bytes.is_empty() {
            return None;
        }

        let n = buffer.remaining_mut().min(data.bytes.len());

        buffer.get_mut().reserve(n);
        buffer.put(data.bytes.split_to(n));

        Some(self.split_fragment())
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
