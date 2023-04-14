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
    impl_std_meta,
    meta::{Application, Discrete, Management, Meta, Stream},
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

impl<T> Record<T>
where
    T: IntoStreamChunker,
{
    pub fn map_to_chunker(self) -> Record<StreamChunker<T::Item>> {
        Record {
            header: self.header,
            body: self.body.into_stream_chunker(),
        }
    }
}

impl<T: EncodeChunk> Record<StreamChunker<T>> {
    pub fn map_to_empty(self) -> Record<EndOfStream<T>> {
        Record {
            header: self.header,
            body: EndOfStream::new(),
        }
    }
}

impl<T> AsRef<T> for Record<T> {
    fn as_ref(&self) -> &T {
        &self.body
    }
}

pub(crate) trait IntoRecord: Sized {
    fn into_record(self, id: Id) -> Record<Self>;
}

impl<T: Meta> IntoRecord for T {
    fn into_record(self, id: Id) -> Record<T> {
        Record::from_parts(Header::new(id, T::TYPE), self)
    }
}

impl_std_meta! {
    (BeginRequest, Application, Discrete);
    (AbortRequest, Application, Discrete);
    (EndRequest, Application, Discrete);
    (Params, Application, Stream);
    (Stdin, Application, Stream);
    (Stdout, Application, Stream);
    (Stderr, Application, Stream);
    (Data, Application, Stream);
    (GetValues, Management, Discrete);
    (GetValuesResult, Management, Discrete);
    (UnknownType, Management, Discrete);
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
