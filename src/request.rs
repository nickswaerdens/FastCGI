use crate::protocol::record::{
    begin_request::Role, params, Data, Params, ParamsBuilder, Stdin, StreamChunker,
};
use std::time::SystemTime;

#[derive(Debug)]
pub struct Request {
    pub(crate) keep_conn: bool,
    pub(crate) params: Params,
    pub(crate) stdin: Option<Stdin>,
    pub(crate) role: RoleTyped<Data>,
}

impl Request {
    pub fn builder() -> RequestBuilder<Init> {
        RequestBuilder::new()
    }

    pub fn get_keep_conn(&self) -> bool {
        self.keep_conn
    }

    pub fn get_params(&self) -> &Params {
        &self.params
    }

    pub fn get_stdin(&self) -> Option<&Stdin> {
        self.stdin.as_ref()
    }

    pub fn get_role(&self) -> &RoleTyped<Data> {
        &self.role
    }

    pub fn get_data(&self) -> Option<&Data> {
        if let RoleTyped::Filter(ref data) = self.role {
            Some(data)
        } else {
            None
        }
    }

    pub(crate) fn into_parts(self) -> (bool, Params, Option<Stdin>, RoleTyped<Data>) {
        (self.keep_conn, self.params, self.stdin, self.role)
    }
}

mod sealed {
    use super::*;

    pub trait Sealed {}

    // FilterType
    impl Sealed for Data {}
    impl Sealed for StreamChunker<Data> {}
    impl Sealed for Option<StreamChunker<Data>> {}

    // RoleState
    impl Sealed for Responder {}
    impl Sealed for Authorizer {}
    impl Sealed for Filter {}

    impl Sealed for Init {}
    impl<R: RoleState> Sealed for ParamsSet<R> {}
    impl Sealed for FilterSelected {}
}

pub trait FilterType: sealed::Sealed {}

impl FilterType for Data {}
impl FilterType for StreamChunker<Data> {}
impl FilterType for Option<StreamChunker<Data>> {}

#[derive(Debug)]
pub enum RoleTyped<T: FilterType> {
    Responder,
    Authorizer,
    Filter(T),
}

impl<T: FilterType> RoleTyped<T> {
    pub(crate) fn map<F, U>(self, f: F) -> RoleTyped<U>
    where
        F: FnOnce(T) -> U,
        U: FilterType,
    {
        match self {
            Self::Responder => RoleTyped::Responder,
            Self::Authorizer => RoleTyped::Authorizer,
            Self::Filter(data) => RoleTyped::Filter(f(data)),
        }
    }
}

impl<T: FilterType> From<&RoleTyped<T>> for Role {
    fn from(role: &RoleTyped<T>) -> Self {
        match role {
            RoleTyped::Responder => Role::Responder,
            RoleTyped::Authorizer => Role::Authorizer,
            RoleTyped::Filter(_) => Role::Filter,
        }
    }
}

pub trait RoleState: sealed::Sealed {}

pub struct Responder;
pub struct Authorizer;

pub struct Filter {
    data: Data,
}

impl RoleState for Responder {}
impl RoleState for Authorizer {}
impl RoleState for Filter {}

pub trait BuilderState: sealed::Sealed {}

pub struct Init;

pub struct ParamsSet<R: RoleState> {
    params: ParamsBuilder<params::Build, R>,
}

pub struct FilterSelected {
    params: ParamsBuilder<params::Build, Filter>,
    data: Data,
}

impl BuilderState for Init {}
impl<R: RoleState> BuilderState for ParamsSet<R> {}
impl BuilderState for FilterSelected {}

pub struct RequestBuilder<S: BuilderState> {
    keep_conn: bool,
    stdin: Option<Stdin>,
    state: S,
}

impl<S: BuilderState> RequestBuilder<S> {
    pub fn keep_conn(mut self) -> Self {
        self.keep_conn = true;
        self
    }

    pub fn stdin(mut self, stdin: Stdin) -> Self {
        self.stdin = Some(stdin);
        self
    }
}

impl RequestBuilder<Init> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn params<R: RoleState>(
        self,
        params: ParamsBuilder<params::Build, R>,
    ) -> RequestBuilder<ParamsSet<R>> {
        RequestBuilder {
            keep_conn: self.keep_conn,
            stdin: self.stdin,
            state: ParamsSet { params },
        }
    }
}

impl RequestBuilder<ParamsSet<Filter>> {
    /// Automatically adds the 'DATA_LAST_MOD' and 'DATA_LENGTH' params.
    pub fn data(
        mut self,
        data: impl Into<Data>,
        data_last_mod: impl Into<SystemTime>,
    ) -> RequestBuilder<FilterSelected> {
        let data = data.into();

        self.state.params = self.state.params.data_last_mod(data_last_mod.into());
        self.state.params = self.state.params.data_length(data.length());

        RequestBuilder {
            keep_conn: self.keep_conn,
            stdin: self.stdin,
            state: FilterSelected {
                params: self.state.params,
                data,
            },
        }
    }

    /// Automatically adds the 'DATA_LAST_MOD' and 'DATA_LENGTH' params.
    pub fn data_now(self, data: impl Into<Data>) -> RequestBuilder<FilterSelected> {
        Self::data(self, data, SystemTime::now())
    }
}

impl RequestBuilder<ParamsSet<Responder>> {
    pub fn build(self) -> Request {
        Request {
            params: self.state.params.build(),
            stdin: self.stdin,
            role: RoleTyped::Responder,
            keep_conn: self.keep_conn,
        }
    }
}

impl RequestBuilder<ParamsSet<Authorizer>> {
    pub fn build(self) -> Request {
        Request {
            params: self.state.params.build(),
            stdin: self.stdin,
            role: RoleTyped::Authorizer,
            keep_conn: self.keep_conn,
        }
    }
}

impl RequestBuilder<FilterSelected> {
    pub fn build(self) -> Request {
        Request {
            params: self.state.params.build(),
            stdin: self.stdin,
            role: RoleTyped::Filter(self.state.data),
            keep_conn: self.keep_conn,
        }
    }
}

impl Default for RequestBuilder<Init> {
    fn default() -> Self {
        Self {
            keep_conn: false,
            stdin: None,
            state: Init,
        }
    }
}
