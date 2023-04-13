pub mod client;
pub mod codec;
pub mod conn;
pub mod meta;
pub mod record;
pub mod request;
pub mod response;
pub mod server;

use conn::{
    connection::{ConnectionRecvError, ConnectionSendError},
    ParseRequestError, ParseResponseError,
};

pub const FCGI_VERSION_1: u8 = 1;

pub const MANAGEMENT_ID: u16 = 0;

#[derive(Debug)]
pub enum FastcgiClientError {
    Send(ConnectionSendError),
    Recv(ConnectionRecvError<ParseResponseError>),
}

#[derive(Debug)]
pub enum FastcgiServerError {
    Send(ConnectionSendError),
    Recv(ConnectionRecvError<ParseRequestError>),
}

impl From<ConnectionSendError> for FastcgiClientError {
    fn from(value: ConnectionSendError) -> Self {
        FastcgiClientError::Send(value)
    }
}

impl From<ConnectionRecvError<ParseResponseError>> for FastcgiClientError {
    fn from(value: ConnectionRecvError<ParseResponseError>) -> Self {
        FastcgiClientError::Recv(value)
    }
}

impl From<ConnectionSendError> for FastcgiServerError {
    fn from(value: ConnectionSendError) -> Self {
        FastcgiServerError::Send(value)
    }
}

impl From<ConnectionRecvError<ParseRequestError>> for FastcgiServerError {
    fn from(value: ConnectionRecvError<ParseRequestError>) -> Self {
        FastcgiServerError::Recv(value)
    }
}
