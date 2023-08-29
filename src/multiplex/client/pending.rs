use super::{config::PendingConfig, Command};
use crate::{
    protocol::{
        parser::response::{ParseResponseError, Parser, Part, Transition},
        record::{
            ApplicationRecord, ApplicationRecordType, BeginRequest, Data, EncodeRecordError,
            IntoStreamChunker, Params, ProtocolStatusError, Stderr, Stdin, Stdout, StreamChunker,
        },
        transport::Frame,
    },
    request::{Request, RoleTyped},
    response::Response,
    ApplicationId,
};
use bytes::{BufMut, BytesMut};
use futures::{Future, Sink};
use pin_project::pin_project;
use std::{
    pin::Pin,
    task::{ready, Context, Poll},
    time::Instant,
};
use tokio::sync::{mpsc, oneshot};
use tokio_util::sync::{PollSendError, PollSender};

#[pin_project]
#[derive(Debug)]
pub(crate) struct RegisterId<'a> {
    #[pin]
    tx_command: &'a mut PollSender<Command>,
    tx: mpsc::Sender<Frame>,

    #[pin]
    id_receiver: Option<oneshot::Receiver<Option<ApplicationId>>>,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Running,
    StoppedSending,
    ReceiveOnly,
}

#[pin_project]
#[derive(Debug)]
pub struct Pending {
    id: ApplicationId,
    state: State,

    // send
    #[pin]
    tx: PollSender<ApplicationRecord>,
    #[pin]
    tx_command: PollSender<Command>,
    encode_buf: BytesMut,
    request: PartialRequest,

    // recv
    rx: mpsc::Receiver<Frame>,
    parser: Parser,
    response: PartialResponse,

    // config
    expires_at: Instant,
    yield_at: usize,
}

#[pin_project]
#[derive(Debug)]
struct Cleanup {
    id: ApplicationId,

    #[pin]
    tx_command: PollSender<Command>,
    abort: bool,
}

impl<'a> RegisterId<'a> {
    pub(crate) fn new(tx_command: &'a mut PollSender<Command>, tx: mpsc::Sender<Frame>) -> Self {
        Self {
            tx_command,
            tx,
            id_receiver: None,
        }
    }
}

impl Pending {
    pub(crate) fn new(
        id: ApplicationId,
        req: Request,
        tx: PollSender<ApplicationRecord>,
        tx_command: PollSender<Command>,
        rx: mpsc::Receiver<Frame>,
        config: &PendingConfig,
    ) -> Self {
        let (keep_conn, params, stdin, role) = req.into_parts();

        Pending {
            id,
            state: State::default(),

            tx,
            tx_command,
            encode_buf: BytesMut::new(),
            request: PartialRequest {
                keep_conn: Some(keep_conn),
                params: Some(params.into_stream_chunker()),
                stdin: stdin.map(IntoStreamChunker::into_stream_chunker),
                role: role.map(|data| Some(IntoStreamChunker::into_stream_chunker(data))),
            },

            rx,
            parser: Parser::new(config.max_stream_payload_size),
            response: PartialResponse::default(),

            expires_at: Instant::now()
                .checked_add(config.timeout)
                .expect("config.timeout exceeded it's maximum value."),
            yield_at: config.yield_at,
        }
    }

    fn poll_inner(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Response, PendingError>> {
        let this = self.project();
        let mut tx = this.tx;

        if *this.expires_at <= Instant::now() {
            return Poll::Ready(Err(PendingError::Expired));
        }

        let request = this.request;
        let yield_at = *this.yield_at;

        if *this.state == State::Running {
            let id = this.id;
            let buf = this.encode_buf;

            let mut iteration_count = 0;

            if let Some(keep_conn) = request.keep_conn {
                // We didn't send any data yet, so we will return polling early as
                // we know we don't need to flush nor receive any data.
                ready!(tx.as_mut().poll_reserve(cx)?);

                BeginRequest::from_parts(&request.role, keep_conn)
                    .encode(&mut buf.limit(u16::MAX as usize))?;

                tx.as_mut()
                    .send_item(ApplicationRecord::new(
                        *id,
                        ApplicationRecordType::BeginRequest,
                        buf.split(),
                    ))
                    // This error doesn't happen if we get through poll_reserve without error.
                    .unwrap();

                request.keep_conn.take();
                iteration_count += 1;
            }

            // Encode & send stream record chunks of the request.
            'sending: {
                macro_rules! send_stream {
                    ($chunker:expr, $record_type:ident) => {
                        while let Some(chunker) = $chunker.as_mut() {
                            if tx.as_mut().poll_reserve(cx)?.is_pending() {
                                break 'sending;
                            }

                            let record = match chunker
                                .encode(&mut buf.limit(u16::MAX as usize))
                                .transpose()?
                            {
                                Some(_) => ApplicationRecord::new(
                                    *id,
                                    ApplicationRecordType::$record_type,
                                    buf.split(),
                                ),
                                None => {
                                    $chunker.take();

                                    ApplicationRecord::empty::<$record_type>(*id)
                                }
                            };

                            tx.as_mut()
                                .send_item(record)
                                // This error doesn't happen if we get through poll_reserve without
                                // error.
                                .unwrap();

                            iteration_count += 1;
                            if iteration_count == yield_at {
                                cx.waker().wake_by_ref();
                                break 'sending;
                            }
                        }
                    };
                }

                send_stream!(request.params, Params);
                send_stream!(request.stdin, Stdin);

                if let RoleTyped::Filter(data) = &mut request.role {
                    send_stream!(data, Data);
                }

                // We've finished sending the entire request.
                *this.state = State::StoppedSending
            }
        }

        match *this.state {
            State::Running => {
                let _ = tx.as_mut().poll_flush(cx)?;
            }
            State::StoppedSending => {
                if tx.as_mut().poll_close(cx).is_ready() {
                    *this.state = State::ReceiveOnly
                }
            }
            State::ReceiveOnly => {}
        }

        // We won't receive anything until params has fully been sent.
        if request.params.is_none() {
            let response = this.response;

            for _ in 0..yield_at {
                match ready!(this.rx.poll_recv(cx)) {
                    Some(frame) => {
                        let transition = Transition::parse(frame)?;

                        // Parser ensures none of the fields can be overwritten.
                        match this.parser.parse(transition)? {
                            Some(Part::Stdout(stdout)) => response.stdout = stdout,
                            Some(Part::Stderr(stderr)) => response.stderr = stderr,
                            Some(Part::EndRequest(end_request)) => {
                                Result::<_, ProtocolStatusError>::from(
                                    end_request.get_protocol_status(),
                                )?;

                                return Poll::Ready(Ok(Response {
                                    stdout: response.stdout.take(),
                                    stderr: response.stderr.take(),
                                    app_status: end_request.get_app_status(),
                                }));
                            }
                            None => {
                                // Nothing to do, parser is combining the record fragments.
                            }
                        }
                    }
                    None => {
                        // Channel closed before receiving an end request.
                        return Poll::Ready(Err(PendingError::RecvChannelClosedEarly));
                    }
                }
            }
        }

        Poll::Pending
    }
}

impl<'a> Future for RegisterId<'a> {
    type Output = Result<ApplicationId, IdAssignError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let mut tx_command = this.tx_command;
        let mut id_receiver = this.id_receiver;

        if id_receiver.is_none() {
            ready!(tx_command.poll_reserve(cx)?);

            // Attempt to register a new pending request.
            let (tx_id, rx_id) = oneshot::channel();
            tx_command
                .as_mut()
                .send_item(Command::Register {
                    tx_id,
                    tx: this.tx.clone(),
                })
                // This error doesn't happen if we get through poll_reserve without error.
                .unwrap();

            id_receiver.replace(rx_id);
        };

        let _ = tx_command.as_mut().poll_flush(cx)?;

        let recv = id_receiver
            .as_mut()
            .as_pin_mut()
            .expect("id_receiver should've been set.");

        match ready!(recv.poll(cx)?) {
            Some(id) => Poll::Ready(Ok(id)),
            None => {
                // No more id's available, max capacity reached.
                // Attempt to register our sender again after a wait.
                id_receiver.take();
                Poll::Pending
            }
        }
    }
}

impl Future for Pending {
    type Output = Result<Response, PendingError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        // Poll the inner future.
        let inner = ready!(self.as_mut().poll_inner(cx));

        // If there was an error, and the begin_request was sent, we must attempt to abort the
        // request.
        let abort = inner.as_ref().is_err_and(|err| {
            // Abort any in-progress sends immediately.
            //
            // TODO: This may abort the sending of a begin_request, which causes us
            //       to send an abort request for a request which wasn't received by the server.
            self.tx.abort_send();

            err.is_abort_required() && self.request.keep_conn.is_none()
        });

        // Spawn a cleanup task before returning the response or error. This
        // task attempts to abort this request if it failed.
        tokio::spawn(Cleanup {
            id: self.id,
            tx_command: self.tx_command.clone(),
            abort,
        });

        Poll::Ready(inner)
    }
}

impl Future for Cleanup {
    type Output = Result<(), PollSendError<Command>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let mut tx_command = this.tx_command;

        if *this.abort {
            // An error here means we won't free the id.
            // This doesn't matter as the error is very unlikely
            // to be recoverable, and the MUX will shutdown.
            ready!(tx_command.as_mut().poll_reserve(cx))?;

            tx_command
                .as_mut()
                .send_item(Command::Abort { id: *this.id })
                .unwrap();

            *this.abort = false;
        }

        // Flush & close this sender.
        tx_command.poll_close(cx)
    }
}

// PartialRequest
//
// Everything is stored in options to be able to identify when
// one of the fields has finished sending.
// keep_conn corresponds to the begin_request record.
#[derive(Debug)]
struct PartialRequest {
    keep_conn: Option<bool>,
    params: Option<StreamChunker<Params>>,
    stdin: Option<StreamChunker<Stdin>>,
    role: RoleTyped<Option<StreamChunker<Data>>>,
}

#[derive(Debug, Default)]
struct PartialResponse {
    stdout: Option<Stdout>,
    stderr: Option<Stderr>,
}

#[derive(Debug, Clone)]
pub enum IdAssignError {
    IdRecvError(oneshot::error::RecvError),
    SenderError,
}

#[derive(Debug)]
pub enum PendingError {
    Expired,
    SenderError,
    EncodeError(EncodeRecordError),
    ParseError(ParseResponseError),
    RecvChannelClosedEarly,
    ProtocolStatusError(ProtocolStatusError),
}

impl PendingError {
    fn is_abort_required(&self) -> bool {
        // Both variants indicate that the end_request was sent by the server,
        // and therefore, we should not send an abort_request.
        !matches!(
            self,
            PendingError::ProtocolStatusError(_)
                | PendingError::ParseError(ParseResponseError::DecodeEndRequestError(_))
        )
    }
}

impl From<PollSendError<ApplicationRecord>> for PendingError {
    fn from(_: PollSendError<ApplicationRecord>) -> Self {
        PendingError::SenderError
    }
}

impl From<EncodeRecordError> for PendingError {
    fn from(value: EncodeRecordError) -> Self {
        PendingError::EncodeError(value)
    }
}

impl From<ParseResponseError> for PendingError {
    fn from(value: ParseResponseError) -> Self {
        PendingError::ParseError(value)
    }
}

impl From<ProtocolStatusError> for PendingError {
    fn from(value: ProtocolStatusError) -> Self {
        PendingError::ProtocolStatusError(value)
    }
}

impl From<PollSendError<Command>> for IdAssignError {
    fn from(_: PollSendError<Command>) -> Self {
        IdAssignError::SenderError
    }
}

impl From<oneshot::error::RecvError> for IdAssignError {
    fn from(value: oneshot::error::RecvError) -> Self {
        IdAssignError::IdRecvError(value)
    }
}
