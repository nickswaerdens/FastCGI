use std::marker::PhantomData;

use crate::{
    meta::{self, Meta, Stream},
    record::{Header, Id, IntoRecord, Record},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndOfStream<T: Meta<DataKind = meta::Stream>> {
    _marker: PhantomData<T>,
}

impl<T: Meta<DataKind = meta::Stream>> EndOfStream<T> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T: Meta<DataKind = meta::Stream>> Default for EndOfStream<T> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<T: Meta<DataKind = Stream>> IntoRecord for EndOfStream<T> {
    fn into_record(self, id: Id) -> Record<EndOfStream<T>> {
        Record::from_parts(Header::new(id, T::TYPE), self)
    }
}
