use bytes::{BufMut, BytesMut};

use crate::codec::Buffer;

use super::{DecodeFrame, DecodeFrameError, EncodeFrame, EncodeFrameError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ProtocolStatus {
    RequestComplete = 0,
    CantMpxConn = 1,
    Overloaded = 2,
    UnknownRole = 3,
}

impl From<u8> for ProtocolStatus {
    fn from(value: u8) -> Self {
        match value {
            0 => Self::RequestComplete,
            1 => Self::CantMpxConn,
            2 => Self::Overloaded,
            _ => Self::UnknownRole,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndRequest {
    app_status: u32,
    protocol_status: ProtocolStatus,
}

impl EndRequest {
    pub fn new(app_status: u32, protocol_status: ProtocolStatus) -> Self {
        Self {
            app_status,
            protocol_status,
        }
    }

    pub fn get_app_status(&self) -> u32 {
        self.app_status
    }

    pub fn get_protocol_status(&self) -> ProtocolStatus {
        self.protocol_status
    }
}

impl EncodeFrame for EndRequest {
    fn encode(self, dst: &mut Buffer) -> Result<(), EncodeFrameError> {
        if dst.remaining_mut() < 8 {
            return Err(EncodeFrameError::InsufficientSizeInBuffer);
        }

        dst.put_u32(self.app_status);
        dst.put_u8(self.protocol_status as u8);
        dst.put_bytes(0, 3);

        Ok(())
    }
}

impl DecodeFrame for EndRequest {
    fn decode(src: BytesMut) -> Result<EndRequest, DecodeFrameError> {
        if src.len() != 8 {
            return Err(DecodeFrameError::CorruptedFrame);
        }

        // Check that the last 3 bytes are all 0.
        if (u64::from_be_bytes(src[..].try_into().unwrap()) << (5 * 8)) > 0 {
            return Err(DecodeFrameError::CorruptedFrame);
        };

        let app_status = u32::from_be_bytes(src[..4].try_into().unwrap());
        let protocol_status = ProtocolStatus::from(src[4]);

        Ok(EndRequest::new(app_status, protocol_status))
    }
}
