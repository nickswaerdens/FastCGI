use super::{Header, Id};

/// Ready to be sent records.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Record<T> {
    pub(crate) header: Header,
    pub(crate) body: T,
}

impl<T> Record<T> {
    pub fn new(id: Id, body: T) -> Self {
        Record {
            header: Header::new(id),
            body,
        }
    }

    /// Maps the type of body `T` to `U`.
    pub fn map<U>(self, f: fn(T) -> U) -> Record<U> {
        Record {
            header: self.header,
            body: f(self.body),
        }
    }

    pub fn get_header(&self) -> &Header {
        &self.header
    }

    pub fn get_header_mut(&mut self) -> &mut Header {
        &mut self.header
    }

    pub fn get_body(&self) -> &T {
        &self.body
    }

    pub fn get_body_mut(&mut self) -> &mut T {
        &mut self.body
    }

    pub fn into_parts(self) -> (Header, T) {
        (self.header, self.body)
    }

    pub const fn from_parts(header: Header, body: T) -> Record<T> {
        Record { header, body }
    }
}

impl<T> AsRef<T> for Record<T> {
    fn as_ref(&self) -> &T {
        &self.body
    }
}

pub(crate) trait IntoRecord: Sized {
    fn into_record(self, header: Header) -> Record<Self>;
}

impl<T> IntoRecord for T {
    fn into_record(self, header: Header) -> Record<T> {
        Record::from_parts(header, self)
    }
}
