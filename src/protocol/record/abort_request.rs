use std::convert::Infallible;

use super::{Decode, DecodeError, EncodeBuffer, EncodeRecord, EncodeRecordError};
use bytes::BytesMut;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbortRequest;

impl EncodeRecord for AbortRequest {
    fn encode_record(self, _: &mut EncodeBuffer) -> Result<(), EncodeRecordError> {
        Ok(())
    }
}

impl Decode for AbortRequest {
    type Error = Infallible;

    fn decode(_: BytesMut) -> Result<AbortRequest, Self::Error> {
        Ok(AbortRequest)
    }
}
