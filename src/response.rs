use crate::{
    meta::DynResponseMetaExt,
    record::{GetValuesResult, Stderr, Stdout, UnknownType},
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
