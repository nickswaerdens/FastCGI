use std::marker::PhantomData;

use crate::{
    meta::{Meta, Stream},
    types::RecordType,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Empty<T> {
    _marker: PhantomData<fn() -> T>,
}

impl<T> Empty<T> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T> Default for Empty<T> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

// Implement `Meta` for empty types which are of kind `Stream` and thus, can be empty.
// Stream records always send an empty record to indicate the end of a stream.
impl<T: Meta<DataKind = Stream>> Meta for Empty<T> {
    const TYPE: RecordType = T::TYPE;
    type SentBy = T::SentBy;
    type RecordKind = T::RecordKind;
    type DataKind = T::DataKind;
}
