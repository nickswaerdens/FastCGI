use super::{DecodeError, EncodeBuffer, EncodeRecordError};
use crate::protocol::record::{Decode, EncodeRecord};
use bytes::{BufMut, BytesMut};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeginRequest {
    role: Role,
    keep_conn: bool,
}

// TODO: remove pub after request API design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Role {
    Responder = 1,
    Authorizer = 2,
    Filter = 3,
}

impl BeginRequest {
    pub fn new(role: impl Into<Role>) -> Self {
        Self {
            role: role.into(),
            keep_conn: false,
        }
    }

    pub fn new_responder() -> Self {
        Self::new(Role::Responder)
    }

    pub fn new_authorizer() -> Self {
        Self::new(Role::Authorizer)
    }

    pub fn new_filter() -> Self {
        Self::new(Role::Filter)
    }

    pub fn keep_conn(mut self) -> Self {
        self.keep_conn = true;
        self
    }

    pub fn encode<B: BufMut>(self, dst: &mut B) -> Result<(), EncodeRecordError> {
        if dst.remaining_mut() < 8 {
            return Err(EncodeRecordError::InsufficientSizeInBuffer);
        }

        dst.put_u16(self.role as u16);
        dst.put_u8(self.keep_conn as u8);
        dst.put_bytes(0, 5);

        Ok(())
    }

    fn decode(src: BytesMut) -> Result<BeginRequest, DecodeError> {
        if src.len() != 8 {
            return Err(DecodeError::InsufficientDataInBuffer);
        }

        let role: Role = u16::from_be_bytes(src[..2].try_into().unwrap()).try_into()?;

        // Check if the last 5 bytes are all 0.
        if (u64::from_be_bytes(src[..].try_into().unwrap()) << (3 * 8)) > 0 {
            return Err(DecodeError::CorruptedFrame);
        }

        let begin_request = BeginRequest::new(role);

        if src[2] > 0 {
            Ok(begin_request.keep_conn())
        } else {
            Ok(begin_request)
        }
    }

    pub fn get_role(&self) -> Role {
        self.role
    }

    pub fn get_keep_conn(&self) -> bool {
        self.keep_conn
    }

    pub fn from_parts(role: impl Into<Role>, keep_conn: bool) -> Self {
        Self {
            role: role.into(),
            keep_conn,
        }
    }
}

impl EncodeRecord for BeginRequest {
    fn encode_record(self, dst: &mut EncodeBuffer) -> Result<(), EncodeRecordError> {
        self.encode(dst)
    }
}

impl Decode for BeginRequest {
    type Error = DecodeError;

    fn decode(src: BytesMut) -> Result<BeginRequest, Self::Error> {
        Self::decode(src)
    }
}

impl TryFrom<u16> for Role {
    type Error = DecodeError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Ok(match value {
            1 => Self::Responder,
            2 => Self::Authorizer,
            3 => Self::Filter,
            _ => return Err(DecodeError::CorruptedFrame),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_decode() {
        let begin_request = BeginRequest::new(Role::Filter).keep_conn();

        let mut buf = BytesMut::with_capacity(8);

        begin_request.encode(&mut buf).unwrap();

        let result = BeginRequest::decode(buf).unwrap();

        assert_eq!(begin_request, result);
    }
}
