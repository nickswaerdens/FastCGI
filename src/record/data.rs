use std::{fmt, fs::File, io::Read};

use bytes::{BufMut, Bytes, BytesMut};

use crate::codec::Buffer;

use super::{DecodeFrame, DecodeFrameError, EncodeChunk, EncodeFrameError};

// TODO: temporarily pub
enum Kind {
    ByteSlice(Bytes),
    Reader((Box<dyn Read + Send + 'static>, u64)),
}

#[derive(Debug)]
pub struct Data {
    kind: Kind,
}

impl Data {
    pub fn new_bytes(bytes: Bytes) -> Self {
        Self {
            kind: Kind::ByteSlice(bytes),
        }
    }

    /// Constructs a new data reader.
    pub fn new_reader<R: Read + Send + 'static>(reader: R, length: u64) -> Self {
        Self {
            kind: Kind::Reader((Box::new(reader), length)),
        }
    }

    pub fn length(&self) -> u64 {
        match &self.kind {
            Kind::ByteSlice(bytes) => bytes.len() as u64,
            Kind::Reader((_, length)) => *length,
        }
    }

    pub fn byte_slice(&self) -> Option<&Bytes> {
        if let Kind::ByteSlice(ref bytes) = self.kind {
            Some(bytes)
        } else {
            None
        }
    }
}

impl From<&'static [u8]> for Data {
    fn from(value: &'static [u8]) -> Self {
        Self::new_bytes(Bytes::from_static(value))
    }
}

impl From<&'static str> for Data {
    fn from(value: &'static str) -> Self {
        Self::new_bytes(Bytes::from_static(value.as_bytes()))
    }
}

impl From<Bytes> for Data {
    fn from(value: Bytes) -> Self {
        Self::new_bytes(value)
    }
}

impl TryFrom<File> for Data {
    type Error = std::io::Error;

    fn try_from(f: File) -> Result<Self, Self::Error> {
        let metadata = f.metadata()?.len();

        Ok(Self::new_reader(f, metadata))
    }
}

impl EncodeChunk for Data {
    fn encode_chunk(&mut self, buf: &mut Buffer) -> Option<Result<(), EncodeFrameError>> {
        match &mut self.kind {
            Kind::ByteSlice(bytes) => {
                if bytes.is_empty() {
                    return None;
                }

                let n = buf.remaining_mut().min(bytes.len());

                buf.put(bytes.split_to(n));
            }
            Kind::Reader((reader, _)) => {
                let mut handle = reader.take(buf.remaining_mut() as u64);
                let mut writer = buf.writer();

                let n = std::io::copy(&mut handle, &mut writer).unwrap();

                if n == 0 {
                    return None;
                }
            }
        };

        Some(Ok(()))
    }
}

impl DecodeFrame for Data {
    fn decode_frame(src: BytesMut) -> Result<Data, DecodeFrameError> {
        Ok(Data {
            kind: Kind::ByteSlice(src.freeze()),
        })
    }
}

impl fmt::Debug for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("Data");

        match self {
            Kind::ByteSlice(bytes) => {
                format!("ByteSlice: {:?}", bytes);
            }
            Kind::Reader(_) => {
                // TODO: Improve this debug implementation.
                "Reader".to_string();
            }
        };

        debug.finish()
    }
}
