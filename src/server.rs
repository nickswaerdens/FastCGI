use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    conn::{
        connection::{Connection, ConnectionRecvError, ConnectionSendError},
        endpoint,
        state::server::ParseRequestError,
    },
    request::Request,
    response::Response,
    FastcgiServerError,
};

/// TODO: design API.
#[derive(Debug)]
pub struct Server<T> {
    connection: Connection<T, endpoint::Server>,
}

impl<T: AsyncRead + AsyncWrite> Server<T> {
    pub fn new(transport: T) -> Self {
        Self {
            connection: Connection::new(transport),
        }
    }
}

impl<T: AsyncRead + AsyncWrite + Unpin> Server<T> {
    pub async fn handle_request(
        &mut self,
        f: impl Fn(Result<Request, FastcgiServerError>) -> Response,
    ) -> Result<(), FastcgiServerError> {
        if let Some(result) = self.recv_request().await.transpose() {
            let result = result.map_err(|e| {
                // TODO: log this.
                println!("[SERVER]: Request rejected: {:?}", e);
                FastcgiServerError::from(e)
            });

            self.send_response(f(result)).await?
        } else {
            // TODO: log this.
            println!("[SERVER]: Request was aborted.");
        }

        Ok(())
    }
}

impl<T: AsyncRead + Unpin> Server<T> {
    async fn recv_request(
        &mut self,
    ) -> Result<Option<Request>, ConnectionRecvError<ParseRequestError>> {
        let result = Request::recv(&mut self.connection).await;

        self.connection.close_stream();

        result
    }
}

impl<T: AsyncWrite + Unpin> Server<T> {
    async fn send_response(&mut self, res: Response) -> Result<(), ConnectionSendError> {
        res.send(&mut self.connection).await
    }
}
