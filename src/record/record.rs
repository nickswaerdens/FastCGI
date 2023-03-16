use crate::{meta::Meta, types::RecordType};

use super::{Header, Id, Padding};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Record<T> {
    pub(crate) header: Header,
    pub(crate) body: T,
}

impl<T> Record<T> {
    /// Apply padding to this frame's payload based on the length of the payload.
    pub fn with_adaptive_padding(mut self, f: fn(u16) -> u8) -> Self {
        self.header = self.header.with_adaptive_padding(f);
        self
    }

    /// Apply a static amount padding to this frame's payload.
    pub fn with_static_padding(mut self, n: u8) -> Self {
        self.header = self.header.with_static_padding(n);
        self
    }

    /// Avoid adding padding to the frame's payload.
    pub fn without_padding(mut self) -> Self {
        self.header = self.header.without_padding();
        self
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn id(&self) -> Id {
        self.header.id
    }

    pub fn record_type(&self) -> RecordType {
        self.header.record_type
    }

    pub fn padding(&self) -> Option<Padding> {
        self.header.padding
    }

    pub fn into_parts(self) -> (Header, T) {
        (self.header, self.body)
    }
}

impl<T: Meta> Record<T> {
    pub fn new(id: Id, body: T) -> Self {
        Self {
            header: Header::from_meta::<T>(id),
            body,
        }
    }

    pub fn from_parts(header: Header, body: T) -> Record<T> {
        assert!(header.record_type == T::TYPE);

        Record { header, body }
    }
}

impl<T: Meta> Record<Box<T>> {
    pub fn new_boxed(id: Id, body: Box<T>) -> Self {
        Self {
            header: Header::from_meta::<T>(id),
            body,
        }
    }

    pub fn unbox(self) -> Record<T> {
        Record {
            header: self.header,
            body: *self.body,
        }
    }
}

pub(crate) trait IntoRecord: Sized {
    fn into_record(self, id: Id) -> Record<Self>;
}

impl<T: Meta> IntoRecord for T {
    fn into_record(self, id: Id) -> Record<T> {
        Record::new(id, self)
    }
}

impl<T> AsRef<T> for Record<T> {
    fn as_ref(&self) -> &T {
        &self.body
    }
}
