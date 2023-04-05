use std::{fmt, fs::File, io::Read};

use bytes::{BufMut, Bytes, BytesMut};

use super::{
    DecodeFrame, DecodeFrameError, EncodeFragment, EncodeFrameError, StreamFragment,
    StreamFragmenter,
};

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

impl EncodeFragment for StreamFragmenter<Data> {
    type Item = StreamFragment<Data>;

    fn encode_next(&mut self) -> Result<Option<Self::Item>, EncodeFrameError> {
        let (data, mut buffer) = self.parts();

        let fragment = match &mut data.kind {
            Kind::ByteSlice(bytes) => {
                if bytes.is_empty() {
                    return Ok(None);
                }

                let n = buffer.remaining_mut().min(bytes.len());

                buffer.get_mut().reserve(n);
                buffer.put(bytes.split_to(n));

                self.split_fragment()
            }
            Kind::Reader(reader) => {
                let mut handle = reader.take(buffer.remaining_mut() as u64);
                let mut writer = buffer.writer();

                let n = std::io::copy(&mut handle, &mut writer).unwrap();

                if n == 0 {
                    return Ok(None);
                }

                self.split_fragment()
            }
        };

        Ok(Some(fragment))
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
