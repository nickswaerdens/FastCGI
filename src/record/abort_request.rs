use bytes::BytesMut;

use crate::codec::Buffer;

use super::{DecodeFrame, DecodeFrameError, EncodeFrame, EncodeFrameError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbortRequest;

impl EncodeFrame for AbortRequest {
    fn encode_frame(self, _: &mut Buffer) -> Result<(), EncodeFrameError> {
        Ok(())
    }
}

impl DecodeFrame for AbortRequest {
    fn decode_frame(_: BytesMut) -> Result<AbortRequest, DecodeFrameError> {
        Ok(AbortRequest)
    }
}
