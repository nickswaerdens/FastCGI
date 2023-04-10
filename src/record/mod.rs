pub(crate) mod header;
pub(crate) mod record;

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
pub(crate) use record::*;

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
    meta::{
        Application, Client, Discrete, DynRequestMetaExt, DynResponseMetaExt, Management, Meta,
        Server, Stream,
    },
};

pub const DEFAULT_MAX_PAYLOAD_SIZE: usize = u16::MAX as usize;

/// EncodeFrame trait which is used to encode discrete records.
pub trait EncodeFrame: Meta<DataKind = Discrete> {
    /// Encodes self into a fixed size RingBuffer.
    fn encode(self, dst: &mut Buffer) -> Result<(), EncodeFrameError>;
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
    fn decode(src: BytesMut) -> Result<Self, DecodeFrameError>;
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

macro_rules! impl_from_frame {
    (
        {
            $(
                $frame:ident,
            )+
        } => $kind:ident
    ) => {
        $(
            impl From<$frame> for $kind {
                fn from(value: $frame) -> Self {
                    $kind::$frame(value)
                }
            }
        )+
    };
}

pub enum RequestPart {
    BeginRequest(BeginRequest),
    AbortRequest(AbortRequest),
    Params(Params),
    Stdin(Stdin),
    Data(Data),
    GetValues(GetValues),
    Custom(Box<dyn DynRequestMetaExt>),
}

impl_from_frame! {
    {
        BeginRequest,
        AbortRequest,
        Params,
        Stdin,
        Data,
        GetValues,
    } => RequestPart
}

impl From<Box<dyn DynRequestMetaExt>> for RequestPart {
    fn from(value: Box<dyn DynRequestMetaExt>) -> Self {
        RequestPart::Custom(value)
    }
}

pub enum ResponsePart {
    EndRequest(EndRequest),
    Stdout(Stdout),
    Stderr(Stderr),
    GetValuesResult(GetValuesResult),
    UnknownType(UnknownType),
    Custom(Box<dyn DynResponseMetaExt>),
}

impl_from_frame! {
    {
        EndRequest,
        Stdout,
        Stderr,
        GetValuesResult,
        UnknownType,
    } => ResponsePart
}

impl From<Box<dyn DynResponseMetaExt>> for ResponsePart {
    fn from(value: Box<dyn DynResponseMetaExt>) -> Self {
        ResponsePart::Custom(value)
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
