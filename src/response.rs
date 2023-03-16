use crate::{
    meta::DynResponseMetaExt,
    record::{GetValuesResult, Stderr, Stdout, UnknownType},
};

/// TODO: design API.
#[derive(Debug)]
pub struct Response {
    pub(crate) app_status: Option<u32>,
    pub(crate) stdout: Option<Stdout>,
    pub(crate) stderr: Option<Stderr>,
}

impl Default for Response {
    fn default() -> Self {
        Response {
            app_status: None,
            stdout: None,
            stderr: None,
        }
    }
}

enum ManagementResponse {
    UnknownType(UnknownType),
    GetValuesResult(GetValuesResult),
    Custom(Box<dyn DynResponseMetaExt>),
}
