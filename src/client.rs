use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    conn::{
        connection::{Connection, ConnectionRecvError, ConnectionSendError},
        endpoint,
    },
    record::{
        begin_request::Role, end_request::ProtocolStatus, BeginRequest, Header, IntoRecord,
        IntoStreamChunker,
    },
    request::Request,
    response::{Part, Response},
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

impl<T: AsyncWrite + Unpin> Client<T> {
    pub async fn send_request(&mut self, req: Request) -> Result<(), ConnectionSendError> {
        let header = Header::new(1);

        let begin_request = BeginRequest::new(req.role).keep_conn().into_record(header);

        self.connection.feed_frame(begin_request).await?;

        self.send_stream(header, req.params).await?;
        self.send_stream(header, req.stdin).await?;

        if req.role == Role::Filter {
            self.send_stream(header, req.data).await?;
        }

        // Make sure all the data was written out.
        self.connection.flush().await?;

        Ok(())
    }

    async fn send_stream<S: IntoStreamChunker>(
        &mut self,
        header: Header,
        stream: Option<S>,
    ) -> Result<(), ConnectionSendError> {
        if let Some(data) = stream {
            self.connection.feed_stream(data.into_record(header)).await
        } else {
            self.connection.feed_empty::<S::Item>(header).await
        }
    }
}

impl<T: AsyncRead + Unpin> Client<T> {
    /// Currently only works with the "full" parser mode.
    pub async fn recv_response(&mut self) -> Result<Response, ConnectionRecvError> {
        let mut response = Response::default();

        loop {
            match self.connection.poll_frame().await {
                Some(Ok(Some(res))) => match res {
                    Part::Stdout(x) => {
                        response.stdout = Some(x);
                    }
                    Part::Stderr(x) => {
                        response.stderr = Some(x);
                    }
                    Part::EndRequest(end_request) => {
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
