use crate::protocol::record::{EncodeBuffer, EncodeChunk, EncodeRecordError};

/// StreamChunker is a wrapper struct used to encode stream records. It's intended to
/// consume the data used for encoding until the wrapped type is empty.
#[derive(Debug)]
pub struct StreamChunker<T> {
    inner: Option<T>,
}

impl<T: EncodeChunk> StreamChunker<T> {
    pub fn encode(&mut self, buf: &mut EncodeBuffer) -> Option<Result<(), EncodeRecordError>> {
        if let Some(stream) = self.inner.as_mut() {
            let result = stream.encode_chunk(buf);

            if result.is_none() {
                self.inner.take();
            }

            result
        } else {
            None
        }
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_none()
    }
}

pub(crate) trait IntoStreamChunker: Sized {
    // type Item: EncodeChunk;

    fn into_stream_chunker(self) -> StreamChunker<Self>;
}

impl<T: EncodeChunk> IntoStreamChunker for T {
    //type Item = T;

    fn into_stream_chunker(self) -> StreamChunker<Self> {
        StreamChunker { inner: Some(self) }
    }
}
