use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    conn::{
        connection::{Connection, ConnectionRecvError, ConnectionSendError},
        parser::server::RequestParser,
    },
    record::{
        begin_request::Role, end_request::ProtocolStatus, EndRequest, IntoRecord, RequestPart,
        Stdout,
    },
    request::Request,
    response::Response,
};

/// TODO: design API.
#[derive(Debug)]
pub struct Server<T> {
    connection: Connection<T, RequestParser>,
}

impl<T: AsyncRead + AsyncWrite> Server<T> {
    pub fn new(transport: T) -> Self {
        Self {
            connection: Connection::new(transport),
        }
    }
}

impl<T: AsyncRead + Unpin> Server<T> {
    /// Currently only works with the "full" parser mode.
    pub async fn recv_request(&mut self) -> Result<Option<Request>, ConnectionRecvError> {
        use RequestPart::*;

        let mut request = Request::default();

        loop {
            match self.connection.poll_frame().await {
                Some(Ok(Some(req))) => match req {
                    BeginRequest(x) => {
                        request.role = Some(x.role());
                    }
                    AbortRequest(_) => {
                        self.connection.close_stream();

                        return Ok(None);
                    }
                    Params(x) => {
                        request.params = Some(x);
                    }
                    Stdin(x) => {
                        request.stdin = Some(x);

                        match request.role.unwrap() {
                            Role::Responder | Role::Authorizer => {
                                break;
                            }
                            _ => {}
                        }
                    }
                    Data(x) => {
                        request.data = Some(x);

                        break;
                    }
                    _ => {
                        dbg!("Management records are not yet implemented.");
                    } /*
                      // Management records can be received at any time
                      GetValues(x) => {}
                      Custom(x) => {}
                      */
                },
                Some(Err(e)) => Err(e)?,
                _ => {}
            }
        }

        Ok(Some(request))
    }
}

impl<T: AsyncWrite + Unpin> Server<T> {
    pub async fn send_response(&mut self, res: Response) -> Result<(), ConnectionSendError> {
        match res.stdout {
            Some(x) => {
                let (header, body) = x.into_record(1).into_parts();
                self.connection.feed_stream(header, body).await?;
            }
            None => {
                self.connection.feed_empty::<Stdout>(1).await?;
            }
        }

        if let Some(x) = res.stderr {
            let (header, body) = x.into_record(1).into_parts();
            self.connection.feed_stream(header, body).await?;
        }

        let end_request = EndRequest::new(0, ProtocolStatus::RequestComplete).into_record(1);

        self.connection.feed_frame(end_request).await?;

        // Make sure all the data was written out.
        self.connection.flush().await.unwrap();

        self.connection.close_stream();

        Ok(())
    }
}
