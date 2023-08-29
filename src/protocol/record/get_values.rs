use super::{
    Decode, DecodeError, EncodeBuffer, EncodeRecord, EncodeRecordError, NameValuePair,
    NameValuePairs,
};
use bytes::BytesMut;

// GetValues

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GetValues(pub NameValuePairs);

impl GetValues {
    pub fn validate(nvp: &NameValuePair) -> bool {
        !nvp.name.inner().is_empty() && nvp.value.is_none()
    }
}

impl EncodeRecord for GetValues {
    fn encode_record(mut self, buf: &mut EncodeBuffer) -> Result<(), EncodeRecordError> {
        self.0
            .encode_chunk(buf)
            .unwrap_or(Err(EncodeRecordError::InsufficientSizeInBuffer))
    }
}

impl Decode for GetValues {
    type Error = DecodeError;

    fn decode(src: BytesMut) -> Result<Self, Self::Error> {
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

impl EncodeRecord for GetValuesResult {
    fn encode_record(mut self, buf: &mut EncodeBuffer) -> Result<(), EncodeRecordError> {
        self.0
            .encode_chunk(buf)
            .unwrap_or(Err(EncodeRecordError::InsufficientSizeInBuffer))
    }
}

impl Decode for GetValuesResult {
    type Error = DecodeError;

    fn decode(src: BytesMut) -> Result<Self, Self::Error> {
        Ok(GetValuesResult(NameValuePairs::decode(
            src,
            Self::validate,
        )?))
    }
}
