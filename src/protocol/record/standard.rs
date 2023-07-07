use super::{ByteSlice, Decode, DecodeError, EncodeBuffer, EncodeChunk, EncodeRecordError};
use bytes::{Bytes, BytesMut};

// Stdin

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stdin(pub ByteSlice);

impl EncodeChunk for Stdin {
    fn encode_chunk(&mut self, buf: &mut EncodeBuffer) -> Option<Result<(), EncodeRecordError>> {
        self.0.encode_chunk(buf)
    }
}

impl Decode for Stdin {
    fn decode(src: BytesMut) -> Result<Self, DecodeError> {
        Ok(Stdin(ByteSlice::decode(
            src,
            ByteSlice::validate_non_empty,
        )?))
    }
}

impl AsRef<ByteSlice> for Stdin {
    fn as_ref(&self) -> &ByteSlice {
        &self.0
    }
}

impl AsRef<Bytes> for Stdin {
    fn as_ref(&self) -> &Bytes {
        self.0.as_ref()
    }
}

// Stdout

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stdout(pub ByteSlice);

impl EncodeChunk for Stdout {
    fn encode_chunk(&mut self, buf: &mut EncodeBuffer) -> Option<Result<(), EncodeRecordError>> {
        self.0.encode_chunk(buf)
    }
}

impl Decode for Stdout {
    fn decode(src: BytesMut) -> Result<Self, DecodeError> {
        Ok(Stdout(ByteSlice::decode(
            src,
            ByteSlice::validate_non_empty,
        )?))
    }
}

impl AsRef<ByteSlice> for Stdout {
    fn as_ref(&self) -> &ByteSlice {
        &self.0
    }
}

impl AsRef<Bytes> for Stdout {
    fn as_ref(&self) -> &Bytes {
        self.0.as_ref()
    }
}

// Stderr

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stderr(pub ByteSlice);

impl EncodeChunk for Stderr {
    fn encode_chunk(&mut self, buf: &mut EncodeBuffer) -> Option<Result<(), EncodeRecordError>> {
        self.0.encode_chunk(buf)
    }
}

impl Decode for Stderr {
    fn decode(src: BytesMut) -> Result<Self, DecodeError> {
        Ok(Stderr(ByteSlice::decode(
            src,
            ByteSlice::validate_non_empty,
        )?))
    }
}

impl AsRef<ByteSlice> for Stderr {
    fn as_ref(&self) -> &ByteSlice {
        &self.0
    }
}

impl AsRef<Bytes> for Stderr {
    fn as_ref(&self) -> &Bytes {
        self.0.as_ref()
    }
}
