use bytes::BytesMut;

use crate::codec::Buffer;

use super::{
    DecodeFrame, DecodeFrameError, EncodeChunk, EncodeFrameError, NameValuePair, NameValuePairs,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Params(pub NameValuePairs);

impl Params {
    pub fn validate(nvp: &NameValuePair) -> bool {
        !nvp.name.inner().is_empty() && nvp.value.is_some()
    }
}

// Params
impl EncodeChunk for Params {
    fn encode_chunk(&mut self, buf: &mut Buffer) -> Option<Result<(), EncodeFrameError>> {
        self.0.encode_chunk(buf)
    }
}

impl DecodeFrame for Params {
    fn decode(src: BytesMut) -> Result<Self, DecodeFrameError> {
        Ok(Params(NameValuePairs::decode(src, Self::validate)?))
    }
}
