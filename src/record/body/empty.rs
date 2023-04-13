use std::marker::PhantomData;

use crate::meta::{self, Meta};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndOfStream<T: Meta<DataKind = meta::Stream>> {
    _marker: PhantomData<fn() -> T>,
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
