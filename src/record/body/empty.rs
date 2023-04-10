use std::marker::PhantomData;

use crate::meta::{self, Meta};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Empty<T: Meta<DataKind = meta::Stream>> {
    _marker: PhantomData<fn() -> T>,
}

impl<T: Meta<DataKind = meta::Stream>> Empty<T> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<T: Meta<DataKind = meta::Stream>> Default for Empty<T> {
    fn default() -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

pub trait AsEmpty: Sized + Meta<DataKind = meta::Stream> {
    fn empty() -> Empty<Self>;
}

impl<T: Sized + Meta<DataKind = meta::Stream>> AsEmpty for T {
    fn empty() -> Empty<Self> {
        Empty::new()
    }
}
