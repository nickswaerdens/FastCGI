use bytes::{buf::Limit, BufMut, Bytes, BytesMut};
use std::marker::PhantomData;

use crate::{
    meta::{Meta, Stream},
    record::DEFAULT_MAX_PAYLOAD_SIZE,
    types::RecordType,
};

use super::EncodeFrameError;

// An encoded byte slice chunk of a stream meta type T.
#[derive(Debug, Clone)]
pub struct StreamFragment<T: Meta<DataKind = Stream>> {
    pub inner: Bytes,
    _marker: PhantomData<T>,
}

impl<T: Meta<DataKind = Stream>> Meta for StreamFragment<T> {
    const TYPE: RecordType = T::TYPE;
    type SentBy = T::SentBy;
    type RecordKind = T::RecordKind;
    type DataKind = T::DataKind;
}

/// Used to implement `Iterator` for stream meta types T.
///
/// This allows iterators to work with stream fragments.
#[derive(Debug)]
pub struct StreamFragmenter<T: EncodeFragment> {
    pub inner: T,

    // This could be a ring buffer if we can guarantee that
    // StreamFragments are consumed before the next iteration,
    // which they currently are.
    buffer: BytesMut,
    max_payload_size: usize,
    _marker: PhantomData<T>,
}

impl<T: EncodeFragment> StreamFragmenter<T> {
    /// Splits off a fragment from the current buffer.
    pub fn split_fragment(&mut self) -> StreamFragment<T> {
        assert!(self.buffer.len() <= self.max_payload_size);

        StreamFragment {
            inner: self.buffer.split().freeze(),
            _marker: PhantomData,
        }
    }

    pub fn encode_next(&mut self) -> Option<Result<StreamFragment<T>, EncodeFrameError>> {
        let option = self
            .inner
            .encode_fragment(&mut (&mut self.buffer).limit(self.max_payload_size));

        match option {
            Some(Ok(_)) => Some(Ok(self.split_fragment())),
            Some(Err(e)) => Some(Err(e)),
            None => None,
        }
    }
}

pub trait EncodeFragment: Meta<DataKind = Stream> {
    fn encode_fragment(
        &mut self,
        buf: &mut Limit<&mut BytesMut>,
    ) -> Option<Result<(), EncodeFrameError>>;
}

pub(crate) trait IntoStreamFragmenter {
    type Item: EncodeFragment;

    fn into_stream_fragmenter(self) -> StreamFragmenter<Self::Item>;
}

impl<T: EncodeFragment> IntoStreamFragmenter for T {
    type Item = T;

    fn into_stream_fragmenter(self) -> StreamFragmenter<Self::Item> {
        StreamFragmenter {
            inner: self,
            buffer: BytesMut::new(),
            max_payload_size: DEFAULT_MAX_PAYLOAD_SIZE,
            _marker: PhantomData,
        }
    }
}

impl<T: EncodeFragment> Iterator for StreamFragmenter<T> {
    type Item = Result<StreamFragment<T>, EncodeFrameError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.encode_next()
    }
}
