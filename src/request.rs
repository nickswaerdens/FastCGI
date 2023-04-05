use crate::{
    meta::DynRequestMetaExt,
    record::{begin_request::Role, Data, GetValues, Params, Stdin},
};

/// TODO: design API.
#[derive(Debug, Default)]
pub struct Request {
    pub(crate) role: Option<Role>,
    pub(crate) params: Option<Params>,
    pub(crate) stdin: Option<Stdin>,
    pub(crate) data: Option<Data>,
}

enum ManagementRequest {
    GetValues(GetValues),
    Custom(Box<dyn DynRequestMetaExt>),
}
