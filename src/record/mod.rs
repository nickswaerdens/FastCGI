pub mod begin_request;
pub mod byte_slice;
pub mod data;
pub mod empty;
pub mod end_request;
pub mod header;
pub mod nvps;
pub mod record;
pub mod stream_fragments;
pub mod unknown_type;

use bytes::BytesMut;

// Re-export
pub(crate) use begin_request::BeginRequest;
pub(crate) use data::*;
pub(crate) use end_request::EndRequest;
pub(crate) use unknown_type::UnknownType;

pub use byte_slice::*;
pub(crate) use empty::*;
pub(crate) use header::*;
pub use nvps::*;
pub(crate) use record::*;
pub use stream_fragments::*;

use crate::{
    codec::RingBuffer,
    meta::{
        Application, Client, Discrete, DynRequestMetaExt, DynResponseMetaExt, Management, Meta,
        Server, Stream,
    },
    types::{RecordType, Standard},
};

pub const DEFAULT_MAX_PAYLOAD_SIZE: usize = u16::MAX as usize;

pub trait EncodeFrame {
    /// Encodes self into a fixed size RingBuffer. It's not needed to reserve any space before writing.
    fn encode(self, dst: &mut RingBuffer) -> Result<(), EncodeFrameError>;
}

pub trait DecodeFrame: Sized {
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

// Markers used to create specific instances of types which are then connected to a specific record type `Meta` trait.
mod markers {

    // AbortRequest
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum AbortRequest {}

    //  NameValuePairs
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Params {}

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum GetValues {}

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum GetValuesResult {}

    // ByteSlice
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Stdin {}

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Stdout {}

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Stderr {}
}

pub type AbortRequest = Empty<markers::AbortRequest>;
pub type Params = NameValuePairs<NameValuePair, markers::Params>;
pub type Stdin = ByteSlice<markers::Stdin>;
pub type Stdout = ByteSlice<markers::Stdout>;
pub type Stderr = ByteSlice<markers::Stderr>;
pub type GetValues = NameValuePairs<NameEmptyPair, markers::GetValues>;
pub type GetValuesResult = NameValuePairs<NameValuePair, markers::GetValuesResult>;

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
