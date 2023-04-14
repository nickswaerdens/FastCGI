use std::time::SystemTime;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    await_variant, build_enum_with_from_impls,
    conn::{
        connection::{Connection, ConnectionRecvError, ConnectionSendError},
        endpoint, ParseRequestError,
    },
    meta::DynRequestMetaExt,
    record::{
        begin_request, params, AbortRequest, BeginRequest, Data, EndOfStream, GetValues, Id,
        IntoRecord, Params, ParamsBuilder, Stdin,
    },
};

#[derive(Debug)]
pub struct Request {
    keep_conn: bool,
    params: Params,
    stdin: Option<Stdin>,
    role: Role,
}

impl Request {
    pub fn builder() -> RequestBuilder<Init> {
        RequestBuilder::new()
    }

    pub(crate) async fn send<T: AsyncWrite + Unpin>(
        self,
        connection: &mut Connection<T, endpoint::Client>,
    ) -> Result<(), ConnectionSendError> {
        // Available Id should be received from the connection.
        let id = 1;

        let begin_request =
            BeginRequest::from_parts((&self.role).into(), self.keep_conn).into_record(id);

        connection.feed_frame(begin_request).await?;

        let result = self.send_inner(id, connection).await;

        // Attempt to send an abort request on error.
        if result.is_err() {
            connection.feed_frame(AbortRequest.into_record(id)).await?;
        }

        // Make sure all the data was written out.
        connection.flush().await?;

        result
    }

    async fn send_inner<T: AsyncWrite + Unpin>(
        self,
        id: Id,
        connection: &mut Connection<T, endpoint::Client>,
    ) -> Result<(), ConnectionSendError> {
        connection.feed_stream(self.params.into_record(id)).await?;

        if let Some(stdin) = self.stdin {
            connection.feed_stream(stdin.into_record(id)).await?;
        } else {
            let eof = EndOfStream::<Stdin>::new().into_record(id);
            connection.feed_empty(eof).await?;
        }

        if let Role::Filter(data) = self.role {
            connection.feed_stream(data.into_record(id)).await?;
        }

        Ok(())
    }

    pub(crate) async fn recv<T: AsyncRead + Unpin>(
        connection: &mut Connection<T, endpoint::Server>,
    ) -> Result<Option<Self>, ConnectionRecvError<ParseRequestError>> {
        // A channel should be used here instead which receives request parts
        // based on the request id.

        // The stream state guarantees that none of the expects and unreachable! can fail.

        let begin_request = loop {
            if let Some(result) = connection.poll_frame().await {
                break BeginRequest::try_from(result?).expect("An unexpected error occured.");
            }
        };

        let params = await_variant!(connection, Part::Params);
        let stdin = await_variant!(connection, Part::Stdin);

        let role = match begin_request.get_role() {
            begin_request::Role::Responder => Role::Responder,
            begin_request::Role::Authorizer => Role::Authorizer,
            begin_request::Role::Filter => {
                let data = await_variant!(connection, Part::Data);

                Role::Filter(data)
            }
        };

        Ok(Some(Request {
            keep_conn: begin_request.get_keep_conn(),
            params,
            stdin,
            role,
        }))
    }

    pub fn get_keep_conn(&self) -> bool {
        self.keep_conn
    }

    pub fn get_params(&self) -> &Params {
        &self.params
    }

    pub fn get_stdin(&self) -> &Option<Stdin> {
        &self.stdin
    }

    pub fn get_role(&self) -> &Role {
        &self.role
    }

    pub fn get_data(&self) -> Option<&Data> {
        if let Role::Filter(ref data) = self.role {
            Some(data)
        } else {
            None
        }
    }

    pub(crate) fn into_parts(self) -> (bool, Params, Option<Stdin>, Role) {
        (self.keep_conn, self.params, self.stdin, self.role)
    }
}

#[derive(Debug)]
pub enum Role {
    Responder,
    Authorizer,
    Filter(Data),
}

impl From<&Role> for begin_request::Role {
    fn from(role: &Role) -> Self {
        match role {
            Role::Responder => begin_request::Role::Responder,
            Role::Authorizer => begin_request::Role::Authorizer,
            Role::Filter(_) => begin_request::Role::Filter,
        }
    }
}

mod sealed {
    use super::*;

    pub trait Sealed {}

    impl Sealed for Responder {}
    impl Sealed for Authorizer {}
    impl Sealed for Filter {}

    impl Sealed for Init {}
    impl<R: RoleTyped> Sealed for ParamsSet<R> {}
    impl Sealed for FilterSelected {}
}

pub trait RoleTyped: sealed::Sealed {}

pub struct Responder;
pub struct Authorizer;

pub struct Filter {
    data: Data,
}

impl RoleTyped for Responder {}
impl RoleTyped for Authorizer {}
impl RoleTyped for Filter {}

pub trait BuilderState: sealed::Sealed {}

pub struct Init;

pub struct ParamsSet<R: RoleTyped> {
    params: ParamsBuilder<params::Build, R>,
}

pub struct FilterSelected {
    params: ParamsBuilder<params::Build, Filter>,
    data: Data,
}

impl BuilderState for Init {}
impl<R: RoleTyped> BuilderState for ParamsSet<R> {}
impl BuilderState for FilterSelected {}

pub struct RequestBuilder<S: BuilderState> {
    keep_conn: bool,
    stdin: Option<Stdin>,
    state: S,
}

impl RequestBuilder<Init> {
    pub fn new() -> Self {
        Self::default()
    }
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
    pub fn params<R: RoleTyped>(
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
    pub fn data(
        mut self,
        data: Data,
        data_last_mod: impl Into<SystemTime>,
    ) -> RequestBuilder<FilterSelected> {
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
}

impl RequestBuilder<ParamsSet<Responder>> {
    pub fn build(self) -> Request {
        Request {
            params: self.state.params.build(),
            stdin: self.stdin,
            role: Role::Responder,
            keep_conn: self.keep_conn,
        }
    }
}

impl RequestBuilder<ParamsSet<Authorizer>> {
    pub fn build(self) -> Request {
        Request {
            params: self.state.params.build(),
            stdin: self.stdin,
            role: Role::Authorizer,
            keep_conn: self.keep_conn,
        }
    }
}

impl RequestBuilder<FilterSelected> {
    pub fn build(self) -> Request {
        Request {
            params: self.state.params.build(),
            stdin: self.stdin,
            role: Role::Filter(self.state.data),
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

build_enum_with_from_impls! {
    pub(crate) Part {
        BeginRequest(BeginRequest),
        AbortRequest,
        Params(Params),
        Stdin(Option<Stdin>),
        Data(Data),
    }
}

enum ManagementRequest {
    GetValues(GetValues),
    Custom(Box<dyn DynRequestMetaExt>),
}

impl From<Box<dyn DynRequestMetaExt>> for ManagementRequest {
    fn from(value: Box<dyn DynRequestMetaExt>) -> Self {
        ManagementRequest::Custom(value)
    }
}
