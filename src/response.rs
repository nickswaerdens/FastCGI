use crate::{
    meta::DynResponseMetaExt,
    record::{GetValuesResult, Stderr, Stdout, UnknownType},
};

/// TODO: design API.
#[derive(Debug, Default)]
pub struct Response {
    pub(crate) app_status: Option<u32>,
    pub(crate) stdout: Option<Stdout>,
    pub(crate) stderr: Option<Stderr>,
}

enum ManagementResponse {
    UnknownType(UnknownType),
    GetValuesResult(GetValuesResult),
    Custom(Box<dyn DynResponseMetaExt>),
}
