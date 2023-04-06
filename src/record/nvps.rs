use std::marker::PhantomData;

use bytes::{buf::Limit, Buf, BufMut, Bytes, BytesMut};

use crate::meta::{Meta, Stream};

use super::{DecodeFrame, DecodeFrameError, EncodeFragment, EncodeFrameError};

#[derive(Debug, Clone)]
pub struct NameValuePairs<T: NameValuePairType, M> {
    inner: Vec<T>,
    _marker: PhantomData<M>,
}

impl<T: NameValuePairType, M> NameValuePairs<T, M> {
    fn new() -> Self {
        Self {
            inner: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// An empty body can be constructed with the `AsEmpty` trait method.
    pub fn builder() -> NameValuePairsBuilder<T, M> {
        NameValuePairsBuilder::new()
    }

    pub fn size_hint(&self) -> usize {
        self.inner
            .iter()
            .fold(0, |acc, pair| acc + pair.size_hint())
    }
}

impl<T: NameValuePairType, M> IntoIterator for NameValuePairs<T, M> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl<T: NameValuePairType, M> AsRef<Vec<T>> for NameValuePairs<T, M> {
    fn as_ref(&self) -> &Vec<T> {
        &self.inner
    }
}

impl<T: NameValuePairType, M> AsMut<Vec<T>> for NameValuePairs<T, M> {
    fn as_mut(&mut self) -> &mut Vec<T> {
        &mut self.inner
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Param {
    Short(Bytes),
    Long(Bytes),
}

impl Param {
    pub fn new(bytes: impl Into<Bytes>) -> Self {
        let bytes: Bytes = bytes.into();

        if bytes.len() > i8::MAX as usize {
            Self::Long(bytes)
        } else {
            Self::Short(bytes)
        }
    }

    pub fn inner(&self) -> &[u8] {
        match self {
            Self::Short(b) => b,
            Self::Long(b) => b,
        }
    }

    pub fn byte_count(&self) -> u8 {
        match self {
            Self::Short(_) => 1,
            Self::Long(_) => 4,
        }
    }

    pub fn validate(param: &[u8]) -> bool {
        // Can't imagine the max size happening by accident, should this be checked?

        param.is_empty() || param.len() > i32::MAX as usize
    }

    pub fn encode_length(&self, dst: &mut BytesMut) {
        match self {
            Param::Short(b) => dst.put_u8(b.len() as u8),
            Param::Long(b) => dst.put_u32(b.len() as u32 | 0x80000000),
        }
    }

    /// Decodes the length of a param, and returns the length.
    pub fn decode_length(buf: &mut impl Buf) -> Option<usize> {
        buf.chunk().first().copied().and_then(|byte| {
            if byte >> 7u8 == 1 {
                if buf.remaining() >= 4 {
                    let [b0, b1, b2, b3] = <[u8; 4]>::try_from(&buf.chunk()[..4]).unwrap();

                    buf.advance(4);

                    Some(
                        ((u32::from(b0 & 0x7f) << 24)
                            + (u32::from(b1) << 16)
                            + (u32::from(b2) << 8)
                            + u32::from(b3)) as usize,
                    )
                } else {
                    None
                }
            } else {
                Some(buf.get_u8() as usize)
            }
        })
    }
}

impl From<Bytes> for Param {
    fn from(value: Bytes) -> Self {
        if value.len() > i8::MAX as usize {
            Self::Long(value)
        } else {
            Self::Short(value)
        }
    }
}

pub trait NameValuePairType: Sized {
    fn size_hint(&self) -> usize;

    fn encode(self, dst: &mut Limit<&mut BytesMut>);

    fn decode(src: &mut BytesMut) -> Result<Self, DecodeFrameError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameEmptyPair {
    name: Param,
}

impl NameEmptyPair {
    pub fn new(name: impl Into<Bytes>) -> Option<Self> {
        let name: Bytes = name.into();

        if Param::validate(&name) {
            return None;
        }

        Some(Self::new_unchecked(name))
    }

    pub fn new_unchecked(name: impl Into<Bytes>) -> Self {
        Self {
            name: Param::new(name),
        }
    }

    pub fn size_hint(&self) -> usize {
        // +1 for value length, which is always 0.
        self.name.byte_count() as usize + 1 + self.name.inner().len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameValuePair {
    name: Param,
    value: Param,
}

impl NameValuePair {
    pub fn new(name: impl Into<Bytes>, value: impl Into<Bytes>) -> Option<Self> {
        let name: Bytes = name.into();
        let value: Bytes = value.into();

        // Can't imagine the max size happening by accident, should this be checked?
        if Param::validate(&name) || Param::validate(&value) {
            return None;
        }

        Some(Self::new_unchecked(name, value))
    }

    pub fn new_unchecked(name: impl Into<Bytes>, value: impl Into<Bytes>) -> Self {
        Self {
            name: Param::new(name),
            value: Param::new(value),
        }
    }

    pub fn size_hint(&self) -> usize {
        self.name.byte_count() as usize
            + self.value.byte_count() as usize
            + self.name.inner().len()
            + self.value.inner().len()
    }
}

#[derive(Debug)]
pub struct NameValuePairsBuilder<T: NameValuePairType, M> {
    inner: NameValuePairs<T, M>,
    _marker: PhantomData<fn() -> M>,
}

impl<T: NameValuePairType, M> NameValuePairsBuilder<T, M> {
    /// This method is called from an NameValuePairs.
    fn new() -> Self {
        Self {
            inner: NameValuePairs::new(),
            _marker: PhantomData,
        }
    }

    pub fn push(mut self, nvp: T) -> Self {
        self.inner.as_mut().push(nvp);
        self
    }

    /// Build a NameValuePairs, returning an error if the body is empty.
    /// If the empty body is intended, use the `empty()` method on an NameValuePairs instead.
    pub fn build(self) -> Option<NameValuePairs<T, M>> {
        if self.inner.as_ref().is_empty() {
            return None;
        }

        Some(self.inner)
    }
}

impl NameValuePairType for NameEmptyPair {
    fn size_hint(&self) -> usize {
        self.size_hint()
    }

    fn encode(self, dst: &mut Limit<&mut BytesMut>) {
        let n = self.size_hint();

        assert!(dst.remaining_mut() >= n);

        let inner = match self.name {
            Param::Short(b) => {
                dst.put_u8(b.len() as u8);
                b
            }
            Param::Long(b) => {
                dst.put_u32(b.len() as u32 | 0x80000000);
                b
            }
        };

        dst.put_u8(0);
        dst.put(inner);
    }

    fn decode(src: &mut BytesMut) -> Result<Self, DecodeFrameError> {
        let Some(name_len) = Param::decode_length(src) else {
            return Err(DecodeFrameError::CorruptedFrame);
        };

        if name_len == 0 {
            return Err(DecodeFrameError::CorruptedFrame);
        }

        if !src.has_remaining() {
            return Err(DecodeFrameError::CorruptedFrame);
        }

        if src.get_u8() != 0 {
            return Err(DecodeFrameError::CorruptedFrame);
        }

        if src.remaining() < name_len {
            return Err(DecodeFrameError::CorruptedFrame);
        }

        let name = src.split_to(name_len).freeze();

        Ok(Self {
            name: Param::from(name),
        })
    }
}

impl NameValuePairType for NameValuePair {
    fn size_hint(&self) -> usize {
        self.size_hint()
    }

    fn encode(self, dst: &mut Limit<&mut BytesMut>) {
        let n = self.size_hint();

        assert!(dst.remaining_mut() >= n);

        let name = match self.name {
            Param::Short(b) => {
                dst.put_u8(b.len() as u8);
                b
            }
            Param::Long(b) => {
                dst.put_u32(b.len() as u32 | 0x80000000);
                b
            }
        };

        let value = match self.value {
            Param::Short(b) => {
                dst.put_u8(b.len() as u8);
                b
            }
            Param::Long(b) => {
                dst.put_u32(b.len() as u32 | 0x80000000);
                b
            }
        };

        dst.put(name);
        dst.put(value);
    }

    fn decode(src: &mut BytesMut) -> Result<Self, DecodeFrameError> {
        let Some(name_len) = Param::decode_length(src) else {
            return Err(DecodeFrameError::CorruptedFrame);
        };

        if name_len == 0 {
            return Err(DecodeFrameError::CorruptedFrame);
        }

        let Some(value_len) = Param::decode_length(src) else {
            return Err(DecodeFrameError::CorruptedFrame);
        };

        if src.remaining() < name_len + value_len {
            return Err(DecodeFrameError::CorruptedFrame);
        }

        let name = src.split_to(name_len).freeze();
        let value = src.split_to(value_len).freeze();

        Ok(Self {
            name: Param::from(name),
            value: Param::from(value),
        })
    }
}

impl<T, M> EncodeFragment for NameValuePairs<T, M>
where
    T: NameValuePairType,
    NameValuePairs<T, M>: Meta<DataKind = Stream>,
{
    fn encode_fragment(
        &mut self,
        buf: &mut bytes::buf::Limit<&mut BytesMut>,
    ) -> Option<Result<(), EncodeFrameError>> {
        // Make sure at least the first element fits into the buffer.
        if let Some(size) = self.inner.first().map(|x| x.size_hint()) {
            assert!(size <= buf.remaining_mut());
        } else {
            return None;
        }

        // Find the position at which the buffer can no longer fit another nvp.
        let mut size = 0;
        let drain = match self.inner.iter().position(|nvp| {
            let hint = nvp.size_hint();

            if size + hint <= buf.remaining_mut() {
                size += nvp.size_hint();
                false
            } else {
                true
            }
        }) {
            Some(index) => self.inner.drain(..index),
            None => self.inner.drain(..),
        };
        buf.get_mut().reserve(size);

        for nvp in drain {
            nvp.encode(buf);
        }

        Some(Ok(()))
    }
}

impl<T, M> DecodeFrame for NameValuePairs<T, M>
where
    T: NameValuePairType,
    Self: Meta,
{
    fn decode(mut src: BytesMut) -> Result<NameValuePairs<T, M>, DecodeFrameError> {
        let mut builder = NameValuePairsBuilder::new();

        while src.has_remaining() {
            let nvp = T::decode(&mut src)?;

            builder = builder.push(nvp);
        }

        Ok(builder.build().unwrap())
    }
}

mod tests {
    use super::*;

    #[test]
    fn test_name_empty_pair() {
        let nvp = NameEmptyPair::new_unchecked("a".repeat(255));

        let limit = 0xFFFF;

        let mut buffer = BytesMut::new();
        nvp.clone().encode(&mut (&mut buffer).limit(limit));

        let res = NameEmptyPair::decode(&mut buffer).unwrap();

        assert!(buffer.is_empty());
        assert_eq!(nvp, res);
    }

    #[test]
    fn test_name_value_pair() {
        let nvp = NameValuePair::new_unchecked("a".repeat(255), "b");

        let limit = 0xFFFF;

        let mut buffer = BytesMut::new();
        nvp.clone().encode(&mut (&mut buffer).limit(limit));

        let res = NameValuePair::decode(&mut buffer).unwrap();

        assert!(buffer.is_empty());
        assert_eq!(nvp, res);
    }

    #[test]
    fn length_encoding_decoding() {
        let length = 255;
        let param = Param::new("b".repeat(length));

        let mut buffer = BytesMut::new();
        param.encode_length(&mut buffer);

        let length_2 = Param::decode_length(&mut buffer).unwrap();

        assert!(buffer.is_empty());
        assert_eq!(length, length_2);
    }
}
