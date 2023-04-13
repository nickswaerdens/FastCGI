use bytes::BytesMut;

use crate::codec::Buffer;

use super::{
    DecodeFrame, DecodeFrameError, EncodeFrame, EncodeFrameError, NameValuePair, NameValuePairs,
};

// GetValues

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetValues(pub NameValuePairs);

impl GetValues {
    pub fn validate(nvp: &NameValuePair) -> bool {
        !nvp.name.inner().is_empty() && nvp.value.is_none()
    }
}

impl EncodeFrame for GetValues {
    fn encode_frame(mut self, buf: &mut Buffer) -> Result<(), EncodeFrameError> {
        self.0
            .encode_chunk(buf)
            .unwrap_or(Err(EncodeFrameError::InsufficientSizeInBuffer))
    }
}

impl DecodeFrame for GetValues {
    fn decode_frame(src: BytesMut) -> Result<Self, DecodeFrameError> {
        Ok(GetValues(NameValuePairs::decode(src, Self::validate)?))
    }
}

// GetValuesResult

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetValuesResult(pub NameValuePairs);

impl GetValuesResult {
    pub fn validate(nvp: &NameValuePair) -> bool {
        !nvp.name.inner().is_empty() && nvp.value.is_some()
    }
}

impl EncodeFrame for GetValuesResult {
    fn encode_frame(mut self, buf: &mut Buffer) -> Result<(), EncodeFrameError> {
        self.0
            .encode_chunk(buf)
            .unwrap_or(Err(EncodeFrameError::InsufficientSizeInBuffer))
    }
}

impl DecodeFrame for GetValuesResult {
    fn decode_frame(src: BytesMut) -> Result<Self, DecodeFrameError> {
        Ok(GetValuesResult(NameValuePairs::decode(
            src,
            Self::validate,
        )?))
    }
}
