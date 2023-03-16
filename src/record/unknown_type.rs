use bytes::{BufMut, BytesMut};

use crate::codec::RingBuffer;

use super::{DecodeFrame, DecodeFrameError, EncodeFrame, EncodeFrameError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnknownType {
    record_type: u8,
}

impl UnknownType {
    pub fn new(record_type: u8) -> Self {
        Self { record_type }
    }

    pub fn record_type(&self) -> u8 {
        self.record_type
    }
}

impl EncodeFrame for UnknownType {
    fn encode(self, dst: &mut RingBuffer) -> Result<(), EncodeFrameError> {
        if dst.remaining_mut() < 8 {
            return Err(EncodeFrameError::InsufficientSizeInBuffer);
        }

        dst.put_u8(self.record_type);
        dst.put_bytes(0, 7);

        Ok(())
    }
}

impl DecodeFrame for UnknownType {
    fn decode(src: BytesMut) -> Result<Self, DecodeFrameError> {
        if src.len() != 8 {
            return Err(DecodeFrameError::CorruptedFrame);
        }

        // Check that the last 7 bytes are all 0.
        if (u64::from_be_bytes(src[..].try_into().unwrap()) << 1 * 8) > 0 {
            return Err(DecodeFrameError::CorruptedFrame);
        };

        Ok(UnknownType::new(src[0]))
    }
}
