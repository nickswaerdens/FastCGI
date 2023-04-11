use tokio::io::{AsyncRead, AsyncWrite};

use crate::{
    conn::{
        connection::{Connection, ConnectionRecvError, ConnectionSendError},
        endpoint,
    },
    record::{
        begin_request::Role, end_request::ProtocolStatus, BeginRequest, EndRequest, Header,
        IntoRecord, IntoStreamChunker, RequestPart,
    },
    request::Request,
    response::Response,
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

impl<T: AsyncRead + Unpin> Server<T> {
    /// Currently only works with the "full" parser mode.
    pub async fn recv_request(&mut self) -> Result<Option<Request>, ConnectionRecvError> {
        let begin_request = self.await_begin_request().await?;

        let mut request = Request {
            role: begin_request.get_role(),
            ..Default::default()
        };

        loop {
            match self.connection.poll_frame().await {
                Some(Ok(Some(req))) => match req {
                    /*
                    Should no longer be received.
                    BeginRequest(x) => {
                        request.role = Some(x.get_role());
                    }
                    */
                    RequestPart::AbortRequest(_) => {
                        self.connection.close_stream();

                        return Ok(None);
                    }
                    RequestPart::Params(x) => {
                        request.params = Some(x);
                    }
                    RequestPart::Stdin(x) => {
                        request.stdin = Some(x);

                        match request.role {
                            Role::Responder | Role::Authorizer => {
                                break;
                            }
                            _ => {}
                        }
                    }
                    RequestPart::Data(x) => {
                        request.data = Some(x);

                        break;
                    }
                    _ => {
                        dbg!("Management records are not yet implemented.");
                    } /*
                      // Management records can be received at any time
                      // These records should be sent to a separate channel.
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

    async fn await_begin_request(&mut self) -> Result<BeginRequest, ConnectionRecvError> {
        loop {
            match self.connection.poll_frame().await {
                Some(Ok(Some(req))) => match req {
                    RequestPart::BeginRequest(begin_request) => {
                        return Ok(BeginRequest::new(begin_request.get_role()));
                    }
                    _ => {
                        dbg!("Management records are not yet implemented.");
                    } /*
                      // Management records can be received at any time
                      // These records should be sent to a separate channel.
                      GetValues(x) => {}
                      Custom(x) => {}
                      */
                },
                Some(Err(e)) => Err(e)?,
                _ => {}
            }
        }
    }
}

impl<T: AsyncWrite + Unpin> Server<T> {
    pub async fn send_response(&mut self, res: Response) -> Result<(), ConnectionSendError> {
        let header = Header::new(1);

        // TODO: Stdout and Stderr should be interleaved here.
        self.send_stream(header, res.stdout).await?;

        if res.stderr.is_some() {
            self.send_stream(header, res.stderr).await?;
        }

        let end_request = EndRequest::new(0, ProtocolStatus::RequestComplete).into_record(header);
        self.connection.feed_frame(end_request).await?;

        // Make sure all the data was written out.
        self.connection.flush().await?;
        self.connection.close_stream();

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
