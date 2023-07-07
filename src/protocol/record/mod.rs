pub(crate) mod abort_request;
pub(crate) mod begin_request;
pub mod common;
pub(crate) mod data;
pub(crate) mod end_request;
pub(crate) mod get_values;
pub(crate) mod header;
pub mod params;
pub mod standard;
pub mod types;
pub(crate) mod unknown_type;

use super::meta::{self, MetaCore};
use crate::{impl_std_meta, ApplicationId};
pub(crate) use abort_request::*;
pub(crate) use begin_request::*;
use bytes::{buf::Limit, BytesMut};
pub use common::*;
pub use data::*;
pub(crate) use end_request::*;
pub use get_values::*;
pub(crate) use header::*;
pub use params::*;
pub use standard::*;
use std::num::NonZeroU16;
pub use types::*;
pub(crate) use unknown_type::*;

pub type EncodeBuffer<'a> = Limit<&'a mut BytesMut>;

/// EncodeRecord trait which is used to encode discrete records.
pub trait EncodeRecord: MetaCore<DataKind = meta::Discrete> {
    /// Encodes self into a fixed size RingBuffer.
    fn encode_record(self, dst: &mut EncodeBuffer) -> Result<(), EncodeRecordError>;
}

/// EncodeChunk trait which is used to encode size-limited chunks of stream records.
pub trait EncodeChunk: MetaCore<DataKind = meta::Stream> {
    /// Encode a fragment of the data into the buffer.
    ///
    /// - This method should encode the next chunk of data when called in succession.
    /// - The final representation of buf is what will be sent as the chunk body.
    ///
    /// Returns None if there's no more data to be written.
    fn encode_chunk(&mut self, buf: &mut EncodeBuffer) -> Option<Result<(), EncodeRecordError>>;
}

pub trait Decode: Sized + MetaCore {
    fn decode(src: BytesMut) -> Result<Self, DecodeError>;
}

#[derive(Debug)]
pub(crate) struct Record {
    pub(crate) header: Header,
    pub(crate) body: BytesMut,
}

impl Record {
    pub fn into_parts(self) -> (Header, BytesMut) {
        (self.header, self.body)
    }
}

#[derive(Debug)]
pub(crate) struct ApplicationRecord {
    pub id: ApplicationId,
    pub record_type: RecordType,
    pub body: BytesMut,
}

impl ApplicationRecord {
    pub fn new(id: ApplicationId, record_type: RecordType, body: BytesMut) -> Self {
        Self {
            id,
            record_type,
            body,
        }
    }

    pub fn empty<T: MetaCore<DataKind = meta::Stream>>(id: ApplicationId) -> Self {
        Self {
            id,
            record_type: T::TYPE,
            body: BytesMut::new(),
        }
    }

    pub fn abort(id: ApplicationId) -> Self {
        Self {
            id,
            record_type: AbortRequest::TYPE,
            body: BytesMut::new(),
        }
    }

    pub fn with_padding(self, padding: Option<Padding>) -> Record {
        Record {
            header: Header {
                id: self.id.get(),
                record_type: self.record_type,
                padding,
            },
            body: self.body,
        }
    }

    pub fn as_parts(&self) -> (NonZeroU16, RecordType, &BytesMut) {
        (self.id, self.record_type, &self.body)
    }

    pub fn into_parts(self) -> (NonZeroU16, RecordType, BytesMut) {
        (self.id, self.record_type, self.body)
    }
}

#[derive(Debug)]
pub(crate) struct ManagementRecord {
    pub record_type: RecordType,
    pub body: BytesMut,
}

impl_std_meta! {
    (BeginRequest, meta::Application, meta::Discrete);
    (AbortRequest, meta::Application, meta::Discrete);
    (EndRequest, meta::Application, meta::Discrete);
    (Params, meta::Application, meta::Stream);
    (Stdin, meta::Application, meta::Stream);
    (Stdout, meta::Application, meta::Stream);
    (Stderr, meta::Application, meta::Stream);
    (Data, meta::Application, meta::Stream);
    (GetValues, meta::Management, meta::Discrete);
    (GetValuesResult, meta::Management, meta::Discrete);
    (UnknownType, meta::Management, meta::Discrete);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeRecordError {
    InsufficientSizeInBuffer,
    MaxFrameSizeExceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeError {
    CorruptedFrame,
    InsufficientDataInBuffer,
}
