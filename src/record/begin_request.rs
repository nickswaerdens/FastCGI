use bytes::{BufMut, BytesMut};

use crate::{
    codec::Buffer,
    record::{DecodeFrame, EncodeFrame},
};

use super::{DecodeFrameError, EncodeFrameError};

// TODO: remove pub after request API design.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Role {
    Responder = 1,
    Authorizer = 2,
    Filter = 3,
}

impl TryFrom<u16> for Role {
    type Error = DecodeFrameError;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Ok(match value {
            1 => Self::Responder,
            2 => Self::Authorizer,
            3 => Self::Filter,
            _ => return Err(DecodeFrameError::CorruptedFrame),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BeginRequest {
    role: Role,
    keep_conn: bool,
}

impl BeginRequest {
    pub fn new(role: Role) -> Self {
        Self {
            role,
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

    pub fn get_role(&self) -> Role {
        self.role
    }

    pub fn get_keep_conn(&self) -> bool {
        self.keep_conn
    }
}

impl EncodeFrame for BeginRequest {
    fn encode(self, dst: &mut Buffer) -> Result<(), EncodeFrameError> {
        if dst.remaining_mut() < 8 {
            return Err(EncodeFrameError::InsufficientSizeInBuffer);
        }

        dst.put_u16(self.role as u16);
        dst.put_u8(self.keep_conn as u8);
        dst.put_bytes(0, 5);

        Ok(())
    }
}

impl DecodeFrame for BeginRequest {
    fn decode(src: BytesMut) -> Result<BeginRequest, DecodeFrameError> {
        if src.len() != 8 {
            return Err(DecodeFrameError::InsufficientDataInBuffer);
        }

        let role = u16::from_be_bytes(src[..2].try_into().unwrap()).try_into()?;

        // Check if the last 5 bytes are all 0.
        if (u64::from_be_bytes(src[..].try_into().unwrap()) << (3 * 8)) > 0 {
            return Err(DecodeFrameError::CorruptedFrame);
        }

        let begin_request = BeginRequest::new(role);

        if src[2] > 0 {
            Ok(begin_request.keep_conn())
        } else {
            Ok(begin_request)
        }
    }
}
