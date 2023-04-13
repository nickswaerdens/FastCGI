pub(crate) mod header;

pub mod abort_request;
pub mod begin_request;
pub mod body;
pub mod data;
pub mod end_request;
pub mod get_values;
pub mod params;
pub mod standard;
pub mod types;
pub mod unknown_type;

// Re-export
pub(crate) use header::*;

pub use abort_request::*;
pub use begin_request::*;
pub use body::*;
pub use data::*;
pub use end_request::*;
pub use get_values::*;
pub use params::*;
pub use standard::*;
pub use types::*;
pub use unknown_type::*;

use bytes::BytesMut;

use crate::{
    codec::Buffer,
    meta::{Application, Client, Discrete, Management, Meta, Server, Stream},
};

pub const DEFAULT_MAX_PAYLOAD_SIZE: usize = u16::MAX as usize;

/// EncodeFrame trait which is used to encode discrete records.
pub trait EncodeFrame: Meta<DataKind = Discrete> {
    /// Encodes self into a fixed size RingBuffer.
    fn encode_frame(self, dst: &mut Buffer) -> Result<(), EncodeFrameError>;
}

/// EncodeChunk trait which is used to encode size-limited chunks of stream records.
pub trait EncodeChunk: Meta<DataKind = Stream> {
    /// Encode a fragment of the data into the buffer.
    ///
    /// This method should encode the next chunk of data when called in succession.
    ///
    /// Returns None if there's no more data to be written.
    fn encode_chunk(&mut self, buf: &mut Buffer) -> Option<Result<(), EncodeFrameError>>;
}

pub trait DecodeFrame: Sized + Meta {
    fn decode_frame(src: BytesMut) -> Result<Self, DecodeFrameError>;
}

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

#[macro_export]
macro_rules! build_enum_with_from_impls {
    (
        $vis:vis $name:ident {
            $($variant:tt $(($fool:ty))?,)*
        }
    ) => {
        #[derive(Debug)]
        $vis enum $name {
            $($variant $(($fool))?,)*
        }

        macro_rules! impl_from {
            ($inner:tt $frame:ty) => {
                impl From<$frame> for $name {
                    fn from(value: $frame) -> Self {
                        $name::$inner(value)
                    }
                }

                impl TryFrom<$name> for $frame {
                    type Error = $name;

                    fn try_from(kind: $name) -> Result<Self, Self::Error> {
                        match kind {
                            $name::$inner(frame) => Ok(frame),
                            e => Err(e),
                        }
                    }
                }
            };
            ($inner:tt) => {
                // Do nothing as `From` cannot be implemented for unit-like enum variants.
            };
        }

        $(
            impl_from!($variant $($fool)?);
        )*
    }
}

/// Implements the `Meta` trait for standard record types.
macro_rules! impl_std_meta {
    // Slightly adjusted from: https://stackoverflow.com/a/61189128.
    // Doesn't support module paths nor 'where' constraints.
    (
        $(
            ($variant:ident $(< $( $lt:tt $( : $clt:tt $(+ $dlt:tt )* )? ),+ >)?, $role:ident, $rkind:ident, $dkind:ident);
        )+
    ) => {
        $(
            impl $(< $( $lt $( : $clt $(+ $dlt )* )? ),+ >)? Meta for $variant $(< $( $lt ),+ >)?
            {
                const TYPE: RecordType = RecordType::Standard(Standard::$variant);
                type SentBy = $role;
                type RecordKind = $rkind;
                type DataKind = $dkind;
            }
        )+
    }
}

impl_std_meta! {
    (BeginRequest, Client, Application, Discrete);
    (AbortRequest, Client, Application, Discrete);
    (EndRequest, Server, Application, Discrete);
    (Params, Client, Application, Stream);
    (Stdin, Client, Application, Stream);
    (Stdout, Server, Application, Stream);
    (Stderr, Server, Application, Stream);
    (Data, Client, Application, Stream);
    (GetValues, Client, Management, Discrete);
    (GetValuesResult, Server, Management, Discrete);
    (UnknownType, Server, Management, Discrete);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeFrameError {
    InsufficientSizeInBuffer,
    MaxFrameSizeExceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeFrameError {
    CorruptedFrame,
    InsufficientDataInBuffer,
}
