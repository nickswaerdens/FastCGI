use crate::{
    codec::Buffer,
    record::{EncodeChunk, EncodeFrameError},
};

pub struct StreamChunker<T: EncodeChunk> {
    inner: Option<T>,
}

impl<T: EncodeChunk> StreamChunker<T> {
    pub fn encode(&mut self, buf: &mut Buffer) -> Option<Result<(), EncodeFrameError>> {
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

pub(crate) trait IntoStreamChunker {
    type Item: EncodeChunk;

    fn into_stream_chunker(self) -> StreamChunker<Self::Item>;
}

impl<T: EncodeChunk> IntoStreamChunker for T {
    type Item = T;

    fn into_stream_chunker(self) -> StreamChunker<Self::Item> {
        StreamChunker { inner: Some(self) }
    }
}
