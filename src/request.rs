use crate::{
    meta::DynRequestMetaExt,
    record::{begin_request::Role, Data, GetValues, Params, Stdin},
};

/// TODO: design API.
#[derive(Debug)]
pub struct Request {
    pub(crate) role: Role,
    pub(crate) params: Option<Params>,
    pub(crate) stdin: Option<Stdin>,
    pub(crate) data: Option<Data>,
}

impl Default for Request {
    fn default() -> Self {
        Self {
            role: Role::Responder,
            params: Default::default(),
            stdin: Default::default(),
            data: Default::default(),
        }
    }
}

enum ManagementRequest {
    GetValues(GetValues),
    Custom(Box<dyn DynRequestMetaExt>),
}
