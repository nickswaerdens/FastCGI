use bytes::{BufMut, BytesMut};

use crate::codec::Buffer;

use super::{DecodeFrame, DecodeFrameError, EncodeFrame, EncodeFrameError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownType {
    record_type: u8,
}

impl UnknownType {
    pub fn new(record_type: u8) -> Self {
        Self { record_type }
    }

    fn encode<B: BufMut>(self, dst: &mut B) -> Result<(), EncodeFrameError> {
        if dst.remaining_mut() < 8 {
            return Err(EncodeFrameError::InsufficientSizeInBuffer);
        }

        dst.put_u8(self.record_type);
        dst.put_bytes(0, 7);

        Ok(())
    }

    fn decode(src: BytesMut) -> Result<Self, DecodeFrameError> {
        if src.len() != 8 {
            return Err(DecodeFrameError::CorruptedFrame);
        }

        // Check that the last 7 bytes are all 0.
        if (u64::from_be_bytes(src[..].try_into().unwrap()) << 8) > 0 {
            return Err(DecodeFrameError::CorruptedFrame);
        };

        Ok(Self::new(src[0]))
    }

    pub fn get_record_type(&self) -> u8 {
        self.record_type
    }
}

impl EncodeFrame for UnknownType {
    fn encode_frame(self, dst: &mut Buffer) -> Result<(), EncodeFrameError> {
        self.encode(dst)
    }
}

impl DecodeFrame for UnknownType {
    fn decode_frame(src: BytesMut) -> Result<Self, DecodeFrameError> {
        Self::decode(src)
    }
}

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
