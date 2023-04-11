use crate::{
    impl_from_frame,
    meta::DynRequestMetaExt,
    record::{begin_request::Role, AbortRequest, BeginRequest, Data, GetValues, Params, Stdin},
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

pub enum Part {
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
    } => Part
}

impl From<Box<dyn DynRequestMetaExt>> for Part {
    fn from(value: Box<dyn DynRequestMetaExt>) -> Self {
        Part::Custom(value)
    }
}
