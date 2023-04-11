use crate::{
    impl_from_frame,
    meta::DynResponseMetaExt,
    record::{EndRequest, GetValuesResult, Stderr, Stdout, UnknownType},
};

/// TODO: design API.
#[derive(Debug, Default)]
pub struct Response {
    pub(crate) stdout: Option<Stdout>,
    pub(crate) stderr: Option<Stderr>,
    pub(crate) app_status: Option<u32>,
}

enum ManagementResponse {
    UnknownType(UnknownType),
    GetValuesResult(GetValuesResult),
    Custom(Box<dyn DynResponseMetaExt>),
}

pub enum Part {
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
    } => Part
}

impl From<Box<dyn DynResponseMetaExt>> for Part {
    fn from(value: Box<dyn DynResponseMetaExt>) -> Self {
        Part::Custom(value)
    }
}
