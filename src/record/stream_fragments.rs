use bytes::{buf::Limit, BufMut, Bytes, BytesMut};
use std::marker::PhantomData;

use crate::{
    meta::{Meta, Stream},
    record::DEFAULT_MAX_PAYLOAD_SIZE,
    types::RecordType,
};

use super::{Empty, EncodeFrameError};

// An encoded byte slice chunk of a stream meta type T.
#[derive(Debug, Clone)]
pub struct StreamFragment<T: Meta<DataKind = Stream>> {
    pub inner: Bytes,
    _marker: PhantomData<T>,
}

impl<T> StreamFragment<T>
where
    T: Meta<DataKind = Stream>,
{
    pub fn empty() -> Empty<Self> {
        Empty::new()
    }
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
pub struct StreamFragmenter<T: Meta<DataKind = Stream>> {
    pub inner: T,

    // This could be a ring buffer if we can guarantee that
    // StreamFragments are consumed before the next iteration,
    // which they currently are.
    buffer: BytesMut,
    max_payload_size: usize,
    _marker: PhantomData<T>,
}

impl<T: Meta<DataKind = Stream>> StreamFragmenter<T> {
    /// Splits off a fragment from the current buffer.
    pub fn split_fragment(&mut self) -> StreamFragment<T> {
        assert!(self.buffer.len() <= self.max_payload_size);

        StreamFragment {
            inner: self.buffer.split().freeze(),
            _marker: PhantomData,
        }
    }

    /// Splits the StreamFragmenter into two mutually exclusive references to
    /// modify the inner type, while also modifying the inner buffer.
    pub fn parts(&mut self) -> (&mut T, Limit<&mut BytesMut>) {
        (
            &mut self.inner,
            (&mut self.buffer).limit(self.max_payload_size),
        )
    }
}

pub trait EncodeFragment {
    type Item;

    fn encode_next(&mut self) -> Result<Option<Self::Item>, EncodeFrameError>;
}

pub(crate) trait IntoStreamFragmenter {
    type Item;
    type IntoIter;

    fn into_stream_fragmenter(self) -> Self::IntoIter;
}

impl<T> IntoStreamFragmenter for T
where
    T: Meta<DataKind = Stream>,
    StreamFragmenter<T>: EncodeFragment,
{
    type Item = StreamFragment<T>;
    type IntoIter = StreamFragmenter<T>;

    fn into_stream_fragmenter(self) -> Self::IntoIter {
        StreamFragmenter {
            inner: self,
            buffer: BytesMut::new(),
            max_payload_size: DEFAULT_MAX_PAYLOAD_SIZE,
            _marker: PhantomData,
        }
    }
}
