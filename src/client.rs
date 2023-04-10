use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    conn::{
        connection::{Connection, ConnectionRecvError, ConnectionSendError},
        parser::client::ResponseParser,
    },
    record::{
        begin_request::Role, end_request::ProtocolStatus, BeginRequest, Data, Header, IntoRecord,
        Params, ResponsePart, Stdin,
    },
    request::Request,
    response::Response,
};

/// TODO: design API.
pub struct Client<T> {
    connection: Connection<T, ResponseParser>,
}

impl<T: AsyncRead + AsyncWrite> Client<T> {
    pub fn new(transport: T) -> Self {
        Self {
            connection: Connection::new(transport),
        }
    }
}

impl<T: AsyncWrite + Unpin> Client<T> {
    pub async fn send_request(&mut self, req: Request) -> Result<(), ConnectionSendError> {
        let header = Header::new(1);

        let begin_request = BeginRequest::new_filter().keep_conn().into_record(header);
        self.connection.feed_frame(begin_request).await?;

        match req.params {
            Some(params) => {
                self.connection
                    .feed_stream(params.into_record(header))
                    .await?;
            }
            None => {
                self.connection.feed_empty::<Params>(header).await?;
            }
        }

        match req.stdin {
            Some(stdin) => {
                self.connection
                    .feed_stream(stdin.into_record(header))
                    .await?;
            }
            None => {
                self.connection.feed_empty::<Stdin>(header).await?;
            }
        }

        if req.role == Some(Role::Filter) {
            match req.data {
                Some(data) => {
                    self.connection
                        .feed_stream(data.into_record(header))
                        .await?;
                }
                None => {
                    self.connection.feed_empty::<Data>(header).await?;
                }
            }
        }

        // Make sure all the data was written out.
        self.connection.flush().await.unwrap();

        Ok(())
    }
}

impl<T: AsyncRead + Unpin> Client<T> {
    /// Currently only works with the "full" parser mode.
    pub async fn recv_response(&mut self) -> Result<Response, ConnectionRecvError> {
        use ResponsePart::*;

        let mut response = Response::default();

        loop {
            match self.connection.poll_frame().await {
                Some(Ok(Some(res))) => match res {
                    Stdout(x) => {
                        response.stdout = Some(x);
                    }
                    Stderr(x) => {
                        response.stderr = Some(x);
                    }
                    EndRequest(end_request) => {
                        match end_request.get_protocol_status() {
                            ProtocolStatus::RequestComplete => {
                                response.app_status = Some(end_request.get_app_status());

                                break;
                            }
                            _ => {
                                // Return error with protocol status.
                                todo!()
                            }
                        }
                    }
                    _ => {
                        dbg!("Management records are not yet implemented.");
                    } /*
                      // Management records can be received at any time
                      UnknownType(x) => {}
                      GetValuesResult(x) => {}
                      Custom(x) => {}
                      */
                },
                Some(Err(e)) => Err(e)?,
                _ => {}
            }
        }

        self.connection.close_stream();

        Ok(response)
    }
}
