use super::{Decode, DecodeError, EncodeBuffer, EncodeChunk, EncodeRecordError};
use bytes::{BufMut, Bytes, BytesMut};
use std::{fmt, fs::File, io::Read};

#[derive(Debug)]
pub struct Data {
    kind: Kind,
}

enum Kind {
    ByteSlice(Bytes),
    Reader {
        inner: Box<dyn Read + Send + 'static>,
        length: u64,
    },
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
            kind: Kind::Reader {
                inner: Box::new(reader),
                length,
            },
        }
    }

    pub fn length(&self) -> u64 {
        match &self.kind {
            Kind::ByteSlice(bytes) => bytes.len() as u64,
            Kind::Reader { length, .. } => *length,
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
    fn encode_chunk(&mut self, buf: &mut EncodeBuffer) -> Option<Result<(), EncodeRecordError>> {
        match &mut self.kind {
            Kind::ByteSlice(bytes) => {
                if bytes.is_empty() {
                    return None;
                }

                let n = buf.remaining_mut().min(bytes.len());

                buf.put(bytes.split_to(n));
            }
            Kind::Reader { inner, .. } => {
                let mut handle = inner.take(buf.remaining_mut() as u64);
                let mut writer = buf.writer();

                // TODO: handle this unwrap.
                let n = std::io::copy(&mut handle, &mut writer).unwrap();

                if n == 0 {
                    return None;
                }
            }
        };

        Some(Ok(()))
    }
}

impl Decode for Data {
    type Error = DecodeError;

    fn decode(src: BytesMut) -> Result<Data, Self::Error> {
        Ok(Data {
            kind: Kind::ByteSlice(src.freeze()),
        })
    }
}

impl fmt::Debug for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("Kind");

        match self {
            Kind::ByteSlice(bytes) => {
                debug.field("ByteSlice", &format!("{:?}", bytes));
            }
            Kind::Reader { length, .. } => {
                // TODO: Improve this debug implementation.
                debug.field("Reader", &format!("length: {}", length));
            }
        };

        debug.finish()
    }
}
