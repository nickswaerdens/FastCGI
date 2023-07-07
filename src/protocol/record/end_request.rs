use super::{Decode, DecodeError, EncodeBuffer, EncodeRecord, EncodeRecordError};
use bytes::{Buf, BufMut, BytesMut};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndRequest {
    app_status: u32,
    protocol_status: ProtocolStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ProtocolStatus {
    RequestComplete = 0,
    CantMpxConn = 1,
    Overloaded = 2,
    UnknownRole = 3,
}

impl EndRequest {
    pub fn new(app_status: u32, protocol_status: ProtocolStatus) -> Self {
        Self {
            app_status,
            protocol_status,
        }
    }

    pub fn encode<B: BufMut>(self, dst: &mut B) -> Result<(), EncodeRecordError> {
        if dst.remaining_mut() < 8 {
            return Err(EncodeRecordError::InsufficientSizeInBuffer);
        }

        dst.put_u32(self.app_status);
        dst.put_u8(self.protocol_status as u8);
        dst.put_bytes(0, 3);

        Ok(())
    }

    pub fn decode(mut src: BytesMut) -> Result<EndRequest, DecodeError> {
        if src.len() != 8 {
            return Err(DecodeError::CorruptedFrame);
        }

        // Check that the last 3 bytes are all 0.
        if (u64::from_be_bytes(src[..].try_into().unwrap()) << (5 * 8)) > 0 {
            return Err(DecodeError::CorruptedFrame);
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

    pub fn into_parts(self) -> (u32, ProtocolStatus) {
        (self.app_status, self.protocol_status)
    }
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

#[derive(Debug)]
pub enum ProtocolStatusError {
    CantMpxConn,
    Overloaded,
    UnknownRole,
}

impl From<ProtocolStatus> for Result<(), ProtocolStatusError> {
    fn from(val: ProtocolStatus) -> Self {
        match val {
            ProtocolStatus::RequestComplete => Ok(()),
            ProtocolStatus::CantMpxConn => Err(ProtocolStatusError::CantMpxConn),
            ProtocolStatus::Overloaded => Err(ProtocolStatusError::Overloaded),
            ProtocolStatus::UnknownRole => Err(ProtocolStatusError::UnknownRole),
        }
    }
}

impl EncodeRecord for EndRequest {
    fn encode_record(self, dst: &mut EncodeBuffer) -> Result<(), EncodeRecordError> {
        self.encode(dst)
    }
}

impl Decode for EndRequest {
    fn decode(src: BytesMut) -> Result<EndRequest, DecodeError> {
        Self::decode(src)
    }
}

#[cfg(test)]
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
