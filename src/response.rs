use crate::protocol::record::{Stderr, Stdout};

#[derive(Debug)]
pub struct Response {
    pub(crate) stdout: Option<Stdout>,
    pub(crate) stderr: Option<Stderr>,
    pub(crate) app_status: u32,
}

impl Response {
    pub fn builder() -> ResponseBuilder<Init> {
        ResponseBuilder::new()
    }

    /*
    pub(crate) async fn send<T: AsyncWrite + Unpin>(
        self,
        connection: &mut Connection<T, endpoint::Server>,
    ) -> Result<(), ConnectionSendError> {
        // Id should be received from the connection.
        let id = 1;

        // TODO: Stdout and Stderr should be interleaved here.
        // Currently not possible due to &mut connection.
        if let Some(stdout) = self.stdout {
            connection.feed_stream(stdout.into_record(id)).await?;
        } else {
            let eof = EndOfStream::<Stdout>::new().into_record(id);
            connection.feed_empty(eof).await?;
        };

        if let Some(stderr) = self.stderr {
            connection.feed_stream(stderr.into_record(id)).await?;
        } else {
            // Optional
            let eof = EndOfStream::<Stderr>::new().into_record(id);
            connection.feed_empty(eof).await?;
        };

        // TODO: connection handles the other cases of ProtocolStatus.
        let end_request =
            EndRequest::new(self.app_status, ProtocolStatus::RequestComplete).into_record(id);
        connection.feed_frame(end_request).await?;

        // Make sure all the data was written out.
        // connection.flush().await?;
        connection.close_stream();

        Ok(())
    }

    pub(crate) async fn recv<T: AsyncRead + Unpin>(
        connection: &mut Connection<T, endpoint::Client>,
    ) -> Result<Self, ConnectionRecvError<ParseResponseError>> {
        let mut builder = Response::builder();

        let response = loop {
            if let Some(result) = connection.poll_frame().await {
                match result? {
                    Part::Stdout(Some(stdout)) => builder = builder.stdout(stdout),
                    Part::Stderr(Some(stderr)) => builder = builder.stderr(stderr),
                    Part::EndRequest(end_request) => match end_request.get_protocol_status() {
                        ProtocolStatus::RequestComplete => {
                            let app_status = end_request.get_app_status();
                            break builder.app_status(app_status).build();
                        }
                        status => {
                            connection.close_stream();

                            Err(status)?;
                        }
                    },
                    _ => {
                        // Ignore empty Stdout & Stderr
                    }
                }
            }
        };

        Ok(response)
    }
    */

    pub fn get_stdout(&self) -> Option<&Stdout> {
        self.stdout.as_ref()
    }

    pub fn get_stderr(&self) -> Option<&Stderr> {
        self.stderr.as_ref()
    }

    pub fn get_app_status(&self) -> u32 {
        self.app_status
    }

    pub(crate) fn into_parts(self) -> (Option<Stdout>, Option<Stderr>, u32) {
        (self.stdout, self.stderr, self.app_status)
    }
}

mod sealed {
    use super::*;

    pub trait Sealed {}

    impl Sealed for Init {}
    impl Sealed for StatusSet {}
}

pub trait BuilderState: sealed::Sealed {}

pub struct Init;
pub struct StatusSet {
    app_status: u32,
}

impl BuilderState for Init {}
impl BuilderState for StatusSet {}

pub struct ResponseBuilder<S: BuilderState> {
    stdout: Option<Stdout>,
    stderr: Option<Stderr>,
    state: S,
}

impl<T: BuilderState> ResponseBuilder<T> {
    pub fn stdout(mut self, stdout: Stdout) -> Self {
        self.stdout = Some(stdout);
        self
    }

    pub fn stderr(mut self, stderr: Stderr) -> Self {
        self.stderr = Some(stderr);
        self
    }
}

impl ResponseBuilder<Init> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn app_status(self, app_status: u32) -> ResponseBuilder<StatusSet> {
        ResponseBuilder {
            stdout: self.stdout,
            stderr: self.stderr,
            state: StatusSet { app_status },
        }
    }
}

impl ResponseBuilder<StatusSet> {
    pub fn build(self) -> Response {
        Response {
            stdout: self.stdout,
            stderr: self.stderr,
            app_status: self.state.app_status,
        }
    }
}

impl Default for ResponseBuilder<Init> {
    fn default() -> Self {
        Self {
            stdout: None,
            stderr: None,
            state: Init,
        }
    }
}
