use crate::protocol::record::{DecodeError, EncodeRecordError};
use bytes::{Buf, BufMut, Bytes, BytesMut};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct NameValuePairs {
    inner: Vec<NameValuePair>,
}

impl NameValuePairs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert_nvp(mut self, nvp: NameValuePair) -> Self {
        self.inner.push(nvp);
        self
    }

    pub fn size_hint(&self) -> usize {
        self.inner
            .iter()
            .fold(0, |acc, pair| acc + pair.size_hint())
    }

    pub fn encode_chunk<B: BufMut>(
        &mut self,
        buf: &mut B,
    ) -> Option<Result<(), EncodeRecordError>> {
        // Make sure at least the first element fits into the buffer.
        if let Some(size) = self.inner.first().map(|x| x.size_hint()) {
            if size > buf.remaining_mut() {
                return Some(Err(EncodeRecordError::InsufficientSizeInBuffer));
            }
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

        for nvp in drain {
            if let Err(e) = nvp.encode(buf) {
                return Some(Err(e));
            }
        }

        Some(Ok(()))
    }

    pub fn decode<F: Fn(&NameValuePair) -> bool>(
        mut src: BytesMut,
        validate: F,
    ) -> Result<NameValuePairs, DecodeError> {
        let mut nvps = NameValuePairs::new();

        while src.has_remaining() {
            let nvp = NameValuePair::decode(&mut src)?;

            if !validate(&nvp) {
                // TODO: Let users define errors.
                return Err(DecodeError::CorruptedFrame);
            }

            nvps.inner.push(nvp);
        }

        Ok(nvps)
    }
}

impl IntoIterator for NameValuePairs {
    type Item = NameValuePair;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
    }
}

impl AsRef<Vec<NameValuePair>> for NameValuePairs {
    fn as_ref(&self) -> &Vec<NameValuePair> {
        &self.inner
    }
}

impl AsMut<Vec<NameValuePair>> for NameValuePairs {
    fn as_mut(&mut self) -> &mut Vec<NameValuePair> {
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

    pub fn size_hint(&self) -> usize {
        match self {
            Self::Short(bytes) => 1 + bytes.len(),
            Self::Long(bytes) => 4 + bytes.len(),
        }
    }

    pub fn validate(param: &[u8]) -> bool {
        !(param.is_empty() || param.len() > i32::MAX as usize)
    }

    /// Encodes the length of a param, returning the inner bytes.
    pub fn encode_length<B: BufMut>(&self, dst: &mut B) -> &Bytes {
        match self {
            Param::Short(b) => {
                dst.put_u8(b.len() as u8);
                b
            }
            Param::Long(b) => {
                dst.put_u32(b.len() as u32 | 0x80000000);
                b
            }
        }
    }

    /// Decodes the length of a param.
    /// Returns None if there's not enough data in the buffer.
    pub fn decode_length<B: Buf>(buf: &mut B) -> Option<usize> {
        buf.chunk().first().copied().and_then(|byte| {
            if byte >> 7u8 == 1 {
                if buf.remaining() < 4 {
                    return None;
                }

                let [b0, b1, b2, b3] = <[u8; 4]>::try_from(&buf.chunk()[..4]).unwrap();

                buf.advance(4);

                Some(
                    ((u32::from(b0 & 0x7f) << 24)
                        + (u32::from(b1) << 16)
                        + (u32::from(b2) << 8)
                        + u32::from(b3)) as usize,
                )
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameValuePair {
    pub name: Param,
    pub value: Option<Param>,
}

impl NameValuePair {
    pub fn new(name: impl Into<Bytes>, value: impl Into<Bytes>) -> Option<Self> {
        let name = name.into();
        let value = value.into();

        if !Param::validate(&name) || !Param::validate(&value) {
            return None;
        }

        Some(Self::new_unchecked(name, Some(value)))
    }

    pub fn new_empty(name: impl Into<Bytes>) -> Option<Self> {
        let name: Bytes = name.into();

        if !Param::validate(&name) {
            return None;
        }

        Some(Self::new_unchecked(name, None::<&[u8]>))
    }

    pub fn new_unchecked(name: impl Into<Bytes>, value: Option<impl Into<Bytes>>) -> Self {
        Self {
            name: Param::new(name),
            value: value.map(Param::new),
        }
    }

    pub fn size_hint(&self) -> usize {
        self.name.size_hint() + self.value.as_ref().map_or(0, |x| x.size_hint())
    }

    fn encode<B: BufMut>(self, dst: &mut B) -> Result<(), EncodeRecordError> {
        if dst.remaining_mut() < self.size_hint() {
            return Err(EncodeRecordError::InsufficientSizeInBuffer);
        }

        let name = self.name.encode_length(dst);
        let value = self
            .value
            .as_ref()
            .map(|param| param.encode_length(dst))
            .or_else(|| {
                dst.put_u8(0);
                None
            });

        dst.put(&name[..]);
        if let Some(bytes) = value {
            dst.put(&bytes[..])
        }

        Ok(())
    }

    fn decode(src: &mut BytesMut) -> Result<Self, DecodeError> {
        let Some(name_len) = Param::decode_length(src) else {
            return Err(DecodeError::CorruptedFrame);
        };

        if name_len == 0 {
            return Err(DecodeError::CorruptedFrame);
        }

        let Some(value_len) = Param::decode_length(src) else {
            return Err(DecodeError::CorruptedFrame);
        };

        if src.remaining() < name_len + value_len {
            return Err(DecodeError::CorruptedFrame);
        }

        let name = src.split_to(name_len).freeze();
        let value = (value_len > 0).then(|| src.split_to(value_len).freeze());

        Ok(Self {
            name: Param::from(name),
            value: value.map(Param::new),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_name_empty_pair() {
        let nvp = NameValuePair::new_empty("a".repeat(255)).unwrap();

        let mut buffer = BytesMut::new();
        nvp.clone().encode(&mut buffer).unwrap();

        let res = NameValuePair::decode(&mut buffer).unwrap();

        assert!(buffer.is_empty());
        assert_eq!(nvp, res);
    }

    #[test]
    fn test_name_value_pair() {
        let nvp = NameValuePair::new_unchecked("a".repeat(255), Some("b"));

        let mut buffer = BytesMut::new();
        nvp.clone().encode(&mut buffer).unwrap();

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
