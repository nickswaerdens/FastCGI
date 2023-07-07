use crate::protocol::record::{DecodeError, EncodeRecordError};
use bytes::{BufMut, Bytes, BytesMut};

/// Contiguous byte slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteSlice {
    bytes: Bytes,
}

impl ByteSlice {
    pub fn new(bytes: Bytes) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }

        Some(Self { bytes })
    }

    pub fn bytes(&self) -> &Bytes {
        &self.bytes
    }

    /// Assumes `!bytes.is_empty()`.
    pub fn new_unchecked(bytes: Bytes) -> Self {
        Self { bytes }
    }

    pub const fn from_static(bytes: &'static [u8]) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }

        Some(Self {
            bytes: Bytes::from_static(bytes),
        })
    }

    pub fn encode_chunk<B: BufMut>(
        &mut self,
        buf: &mut B,
    ) -> Option<Result<(), EncodeRecordError>> {
        if self.bytes.is_empty() {
            return None;
        }

        let n = buf.remaining_mut().min(self.bytes.len());

        buf.put(self.bytes.split_to(n));

        Some(Ok(()))
    }

    pub fn decode(src: BytesMut, validate: fn(&[u8]) -> bool) -> Result<ByteSlice, DecodeError> {
        let bytes = src.freeze();

        if !validate(&bytes) {
            return Err(DecodeError::CorruptedFrame);
        }

        Ok(Self::new(bytes).unwrap())
    }

    //
    // Expose basic validation methods which can be used by users
    // when adding custom record types.
    //

    pub fn validate_non_empty(bytes: &[u8]) -> bool {
        !bytes.is_empty()
    }
}

impl AsRef<Bytes> for ByteSlice {
    fn as_ref(&self) -> &Bytes {
        &self.bytes
    }
}
