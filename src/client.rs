use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    conn::{
        connection::{Connection, ConnectionRecvError, ConnectionSendError},
        endpoint,
        state::client::ParseResponseError,
    },
    request::Request,
    response::Response,
    FastcgiClientError,
};

/// TODO: design API.
pub struct Client<T> {
    connection: Connection<T, endpoint::Client>,
}

impl<T: AsyncRead + AsyncWrite> Client<T> {
    pub fn new(transport: T) -> Self {
        Self {
            connection: Connection::new(transport),
        }
    }
}

impl<T: AsyncRead + AsyncWrite + Unpin> Client<T> {
    pub async fn send(&mut self, req: Request) -> Result<Response, FastcgiClientError> {
        self.send_request(req).await?;

        self.recv_response().await.map_err(FastcgiClientError::from)
    }
}

impl<T: AsyncWrite + Unpin> Client<T> {
    async fn send_request(&mut self, req: Request) -> Result<(), ConnectionSendError> {
        req.send(&mut self.connection).await?;

        Ok(())
    }
}

impl<T: AsyncRead + Unpin> Client<T> {
    async fn recv_response(&mut self) -> Result<Response, ConnectionRecvError<ParseResponseError>> {
        let result = Response::recv(&mut self.connection).await;

        self.connection.close_stream();

        result
    }
}
