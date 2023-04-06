use std::{fmt, fs::File, io::Read};

use bytes::{BufMut, Bytes, BytesMut};

use super::{DecodeFrame, DecodeFrameError, EncodeFragment, EncodeFrameError};

pub(crate) enum Kind {
    ByteSlice(Bytes),
    Reader(Box<dyn Read + Send + 'static>),
}

pub struct Data {
    pub(crate) kind: Kind,
}

impl Data {
    pub fn new_bytes(bytes: Bytes) -> Self {
        Self {
            kind: Kind::ByteSlice(bytes),
        }
    }

    /// Constructs a new data reader.
    pub fn new_reader<R: Read + Send + 'static>(reader: R) -> Self {
        Self {
            kind: Kind::Reader(Box::new(reader)),
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

impl From<File> for Data {
    fn from(f: File) -> Self {
        Self::new_reader(f)
    }
}

impl EncodeFragment for Data {
    fn encode_fragment(
        &mut self,
        buf: &mut bytes::buf::Limit<&mut BytesMut>,
    ) -> Option<Result<(), EncodeFrameError>> {
        match &mut self.kind {
            Kind::ByteSlice(bytes) => {
                if bytes.is_empty() {
                    return None;
                }

                let n = buf.remaining_mut().min(bytes.len());

                buf.get_mut().reserve(n);
                buf.put(bytes.split_to(n));
            }
            Kind::Reader(reader) => {
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
    fn decode(src: BytesMut) -> Result<Data, DecodeFrameError> {
        Ok(Data {
            kind: Kind::ByteSlice(src.freeze()),
        })
    }
}

impl fmt::Debug for Data {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct("Data");

        match &self.kind {
            Kind::ByteSlice(bytes) => {
                debug.field("kind", &("Kind::ByteSlice", bytes));
            }
            Kind::Reader(_) => {
                // TODO: Improve this debug implementation.
                debug.field("kind", &"Kind::Reader");
            }
        };

        debug.finish()
    }
}
