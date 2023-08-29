use super::{Decode, DecodeError, EncodeBuffer, EncodeRecord, EncodeRecordError};
use bytes::{BufMut, BytesMut};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownType {
    record_type: u8,
}

impl UnknownType {
    pub fn new(record_type: u8) -> Self {
        Self { record_type }
    }

    fn encode<B: BufMut>(self, dst: &mut B) -> Result<(), EncodeRecordError> {
        if dst.remaining_mut() < 8 {
            return Err(EncodeRecordError::InsufficientSizeInBuffer);
        }

        dst.put_u8(self.record_type);
        dst.put_bytes(0, 7);

        Ok(())
    }

    fn decode(src: BytesMut) -> Result<Self, DecodeError> {
        if src.len() != 8 {
            return Err(DecodeError::CorruptedFrame);
        }

        // Check that the last 7 bytes are all 0.
        if (u64::from_be_bytes(src[..].try_into().unwrap()) << 8) > 0 {
            return Err(DecodeError::CorruptedFrame);
        };

        Ok(Self::new(src[0]))
    }

    pub fn get_record_type(&self) -> u8 {
        self.record_type
    }
}

impl EncodeRecord for UnknownType {
    fn encode_record(self, mut dst: &mut EncodeBuffer) -> Result<(), EncodeRecordError> {
        self.encode(dst)
    }
}

impl Decode for UnknownType {
    type Error = DecodeError;

    fn decode(src: BytesMut) -> Result<Self, Self::Error> {
        Self::decode(src)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode() {
        let unknown_request = UnknownType::new(5);

        let mut buf = BytesMut::with_capacity(8);

        unknown_request.encode(&mut buf).unwrap();

        let result = UnknownType::decode(buf).unwrap();

        assert_eq!(unknown_request, result);
    }
}
