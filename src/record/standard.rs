use bytes::BytesMut;

use crate::codec::Buffer;

use super::{ByteSlice, DecodeFrame, DecodeFrameError, EncodeChunk, EncodeFrameError};

// Stdin

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stdin(pub ByteSlice);

impl EncodeChunk for Stdin {
    fn encode_chunk(&mut self, buf: &mut Buffer) -> Option<Result<(), EncodeFrameError>> {
        self.0.encode_chunk(buf)
    }
}

impl DecodeFrame for Stdin {
    fn decode(src: BytesMut) -> Result<Self, DecodeFrameError> {
        Ok(Stdin(ByteSlice::decode(
            src,
            ByteSlice::validate_non_empty,
        )?))
    }
}

// Stdout

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stdout(pub ByteSlice);

impl EncodeChunk for Stdout {
    fn encode_chunk(&mut self, buf: &mut Buffer) -> Option<Result<(), EncodeFrameError>> {
        self.0.encode_chunk(buf)
    }
}

impl DecodeFrame for Stdout {
    fn decode(src: BytesMut) -> Result<Self, DecodeFrameError> {
        Ok(Stdout(ByteSlice::decode(
            src,
            ByteSlice::validate_non_empty,
        )?))
    }
}

// Stderr

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stderr(pub ByteSlice);

impl EncodeChunk for Stderr {
    fn encode_chunk(&mut self, buf: &mut Buffer) -> Option<Result<(), EncodeFrameError>> {
        self.0.encode_chunk(buf)
    }
}

impl DecodeFrame for Stderr {
    fn decode(src: BytesMut) -> Result<Self, DecodeFrameError> {
        Ok(Stderr(ByteSlice::decode(
            src,
            ByteSlice::validate_non_empty,
        )?))
    }
}
