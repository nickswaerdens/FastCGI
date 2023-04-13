use bytes::{Buf, BufMut, BytesMut};

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

    pub fn encode<B: BufMut>(self, dst: &mut B) -> Result<(), EncodeFrameError> {
        if dst.remaining_mut() < 8 {
            return Err(EncodeFrameError::InsufficientSizeInBuffer);
        }

        dst.put_u32(self.app_status);
        dst.put_u8(self.protocol_status as u8);
        dst.put_bytes(0, 3);

        Ok(())
    }

    pub fn decode(mut src: BytesMut) -> Result<EndRequest, DecodeFrameError> {
        if src.len() != 8 {
            return Err(DecodeFrameError::CorruptedFrame);
        }

        // Check that the last 3 bytes are all 0.
        if (u64::from_be_bytes(src[..].try_into().unwrap()) << (5 * 8)) > 0 {
            return Err(DecodeFrameError::CorruptedFrame);
        };

        let app_status = src.get_u32();
        let protocol_status = src.get_u8().into();

        Ok(EndRequest::new(app_status, protocol_status))
    }

    pub fn get_app_status(&self) -> u32 {
        self.app_status
    }

    pub fn get_protocol_status(&self) -> ProtocolStatus {
        self.protocol_status
    }
}

impl EncodeFrame for EndRequest {
    fn encode_frame(self, dst: &mut Buffer) -> Result<(), EncodeFrameError> {
        self.encode(dst)
    }
}

impl DecodeFrame for EndRequest {
    fn decode_frame(src: BytesMut) -> Result<EndRequest, DecodeFrameError> {
        Self::decode(src)
    }
}

mod tests {
    use super::*;

    #[test]
    fn encode_decode() {
        let end_request = EndRequest::new(1, ProtocolStatus::RequestComplete);

        let mut buf = BytesMut::with_capacity(8);

        end_request.encode(&mut buf).unwrap();

        let result = EndRequest::decode(buf).unwrap();

        assert_eq!(end_request, result);
    }
}
