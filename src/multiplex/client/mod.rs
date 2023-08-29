mod config;
mod management;
mod pending;

use crate::{
    protocol::{
        meta::{self},
        record::{ApplicationRecord, ApplicationRecordType, Decode, ManagementRecord, Record},
        transport::{DecodeCodecError, FastCgiCodec, Frame},
    },
    request::Request,
    ApplicationId, MANAGEMENT_ID,
};
pub use config::*;
use futures::{Future, Sink, Stream, TryStream};
pub use pending::*;
use pin_project::pin_project;
use slab::Slab;
use std::{
    borrow::Cow,
    collections::VecDeque,
    pin::Pin,
    sync::Arc,
    task::{ready, Context, Poll},
};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    sync::{mpsc, oneshot},
};
use tokio_util::{codec::Framed, sync::PollSender};

/// Commands take priority over records.
#[derive(Debug)]
pub(crate) enum Command {
    Register {
        tx_id: oneshot::Sender<Option<ApplicationId>>,
        tx: mpsc::Sender<Frame>,
    },
    Abort {
        id: ApplicationId,
    },
}

/// Client is cheap to clone and uses Arc internally.
#[derive(Debug, Clone)]
pub struct Client {
    tx_command: mpsc::Sender<Command>,
    tx_management: mpsc::Sender<ManagementRecord>,
    tx: mpsc::Sender<ApplicationRecord>,

    shared: Arc<Shared>,
}

#[derive(Debug)]
struct Shared {
    config: PendingConfig,
}

#[pin_project]
#[derive(Debug)]
struct ClientReceiver<T> {
    #[pin]
    transport: Framed<T, FastCgiCodec>,
    rx_command: mpsc::Receiver<Command>,
    rx_management: mpsc::Receiver<ManagementRecord>,
    rx: mpsc::Receiver<ApplicationRecord>,

    #[pin]
    pending: Slab<mpsc::Sender<Frame>>,
    aborting: VecDeque<ApplicationId>,

    state: State,
    config: ReceiverConfig,
}

#[derive(Debug, Default, PartialEq, Eq)]
enum State {
    // The default running state sends and receives message to and from the IO.
    #[default]
    Running,

    // Gracefully shutdown the sink part of the mutex receiver.
    StoppedSending,

    // Finally, receive frames from the IO until all pending requests have been fulfilled
    // and the mux receiver task is completely finished.
    ReceiveOnly,
}

impl Client {
    pub fn new<T>(transport: T) -> Self
    where
        T: AsyncRead + AsyncWrite + Send + 'static,
    {
        Self::with_config(transport, Config::default())
    }

    fn with_config<T>(transport: T, config: Config) -> Self
    where
        T: AsyncRead + AsyncWrite + Send + 'static,
    {
        let Config {
            send_channel_limit,
            receiver_config,
            pending_config,
        } = config;

        let (tx_command, rx_command) = mpsc::channel(send_channel_limit);
        let (tx_management, rx_management) = mpsc::channel(send_channel_limit);
        let (tx, rx) = mpsc::channel(send_channel_limit);

        tokio::spawn({
            let receiver = ClientReceiver {
                transport: Framed::new(transport, FastCgiCodec::new()),

                rx_command,
                rx_management,
                rx,

                pending: Slab::new(),
                aborting: VecDeque::new(),

                state: State::default(),

                config: receiver_config,
            };

            async move {
                // TODO: handle this error somehow, maybe with closure from new argument.
                receiver.await;
            }
        });

        Self {
            tx_command,
            tx_management,
            tx,
            shared: Arc::new(Shared {
                config: pending_config,
            }),
        }
    }

    /// Attempts to construct a pending request.
    ///
    /// This may fail if no ID can be assigned to this request. In that case, return the
    /// request that failed alongside an error.
    pub async fn send(&self, req: Request) -> Result<Pending, (Request, IdAssignError)> {
        let (pending_tx, pending_rx) = mpsc::channel(self.shared.config.recv_channel_limit);
        let mut tx_command = PollSender::new(self.tx_command.clone());

        match RegisterId::new(&mut tx_command, pending_tx).await {
            Ok(id) => {
                let tx = PollSender::new(self.tx.clone());

                Ok(Pending::new(
                    id,
                    req,
                    tx,
                    tx_command,
                    pending_rx,
                    &self.shared.config,
                ))
            }
            Err(err) => Err((req, err)),
        }
    }

    pub async fn send_management<R>(&self, req: R) -> Result<R::Dual, ()>
    where
        R: meta::ManagementRecord<Endpoint = meta::Client>,
        R::Dual: Decode,
    {
        todo!()
    }
}

impl<T> Future for ClientReceiver<T>
where
    Framed<T, FastCgiCodec>: Sink<Record> + Stream<Item = Result<Frame, DecodeCodecError>>,
{
    type Output = Result<(), ClientReceiverError<Framed<T, FastCgiCodec>>>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        let mut transport = this.transport;

        let yield_sender_after = this.config.yield_sender_after;
        let yield_receiver_after = this.config.yield_receiver_after;

        let mut iteration_count = 0;
        loop {
            // Handle commands.
            if let Poll::Ready(Some(command)) = this.rx_command.poll_recv(cx) {
                match command {
                    Command::Register { tx_id, tx } => {
                        if *this.state == State::Running
                            && (this.pending.len() as u16) < u16::MAX - 1
                        {
                            // Register a channel for receiving frames, sends the assigned id back
                            // over the one-shot channel.
                            let entry = this.pending.vacant_entry();
                            let id =
                                unsafe { ApplicationId::new_unchecked((entry.key() + 1) as u16) };

                            if tx_id.send(Some(id)).is_ok() {
                                entry.insert(tx);
                            }
                        } else {
                            let _ = tx_id.send(None);
                        }
                    }
                    Command::Abort { id } => {
                        let key = (id.get() - 1) as usize;

                        if this.pending.contains(key) {
                            this.aborting.push_back(id);
                            this.pending.remove(key);
                        }
                    }
                }
            }
            // if transport is ready, check if we need to send anything.
            else if transport
                .as_mut()
                .poll_ready(cx)
                .map_err(ClientReceiverError::from_sink_error)?
                .is_ready()
            {
                // Abort requests first.
                if !this.aborting.is_empty() {
                    if let Some(id) = this.aborting.swap_remove_back(0) {
                        let record = ApplicationRecord::abort(id);

                        transport
                            .as_mut()
                            .start_send(record.with_padding(this.config.padding))
                            .map_err(ClientReceiverError::from_sink_error)?;
                    }
                }
                // then send management requests.
                else if let Poll::Ready(inner) = this.rx_management.poll_recv(cx) {
                    match inner {
                        Some(record) => {
                            transport
                                .as_mut()
                                .start_send(record.with_padding(this.config.padding))
                                .map_err(ClientReceiverError::from_sink_error)?;
                        }
                        None => {
                            // TODO: handle this case.
                        }
                    }
                }
                // lastly send application requests.
                else if *this.state == State::Running {
                    match this.rx.poll_recv(cx) {
                        Poll::Ready(Some(record)) => {
                            if !this.pending.contains((record.id.get() - 1) as usize) {
                                continue;
                            }

                            transport
                                .as_mut()
                                .start_send(record.with_padding(this.config.padding))
                                .map_err(ClientReceiverError::from_sink_error)?;

                            // Yield if frames have been send for a while.
                            // Let the task know that we're immediately ready to progress more.
                            iteration_count += 1;
                            if iteration_count == yield_sender_after {
                                cx.waker().wake_by_ref();
                                break;
                            }
                        }
                        Poll::Ready(None) => {
                            *this.state = State::StoppedSending;
                            break;
                        }
                        Poll::Pending => {
                            // nothing to do.
                            break;
                        }
                    }
                } else {
                    break;
                }
            }
            // No more work to do, transport is busy, or we need to yield.
            else {
                break;
            }
        }

        // Flush the transport sink.
        if !this.pending.is_empty() {
            match *this.state {
                State::Running => {
                    let _ = transport
                        .as_mut()
                        .poll_flush(cx)
                        .map_err(ClientReceiverError::from_sink_error)?;
                }
                State::StoppedSending => {
                    // If we've stopped sending frames, close the sink and start only receiving the
                    // pending responses.
                    if transport
                        .as_mut()
                        .poll_close(cx)
                        .map_err(ClientReceiverError::from_sink_error)?
                        .is_ready()
                    {
                        *this.state = State::ReceiveOnly
                    }
                }
                _ => {
                    // We don't do anything here, as we are only receiving the pending responses.
                }
            }
        }

        // Handle receiving frames from the IO.
        iteration_count = 0;
        while !this.pending.is_empty() {
            match ready!(transport.as_mut().try_poll_next(cx)) {
                Some(Ok(
                    frame @ Frame {
                        id, record_type, ..
                    },
                )) => {
                    if id == MANAGEMENT_ID {
                        // TODO: Handle management records.
                    } else {
                        // Handle application records.
                        let key = (id - 1) as usize;

                        if this.pending.contains(key) {
                            let tx = if record_type == ApplicationRecordType::EndRequest {
                                Cow::Owned(this.pending.remove(key))
                            } else {
                                Cow::Borrowed(unsafe { this.pending.get_unchecked(key) })
                            };

                            if tx.try_send(frame).is_err() {
                                drop(match tx {
                                    Cow::Borrowed(_) => this.pending.remove(key),
                                    Cow::Owned(tx) => tx,
                                });

                                // TODO: log this error, recv channel was either closed or full.
                            }
                        } else {
                            // Entry not found so we don't know where to send this frame, all we can
                            // do is ignore it.
                            //
                            // TODO: log this.
                        }
                    }
                }
                Some(Err(DecodeCodecError::IncompatibleVersion)) => {
                    // Is this recoverable?
                    println!("Received a response with an incompatible FastCGI version.");
                }
                Some(Err(DecodeCodecError::CorruptedHeader)) => {
                    // Is this recoverable?
                }
                Some(Err(DecodeCodecError::StdIoError(err))) => {
                    // Is this recoverable?
                }
                None => {
                    // Error, transport terminated and we can no longer receive frames.
                    // return Poll::Ready(Err())
                }
            };

            if let Some(inner) = yield_receiver_after {
                // Yield after receiving frames for a while.
                // Let the task know that we're immediately ready to make progress again.
                iteration_count += 1;
                if iteration_count == inner {
                    cx.waker().wake_by_ref();
                    break;
                }
            }
        }

        // Close if done.
        if this.pending.is_empty() {
            match *this.state {
                State::StoppedSending => {
                    return transport
                        .poll_close(cx)
                        .map_err(ClientReceiverError::from_sink_error);
                }
                State::ReceiveOnly => {
                    // Transport was already closed.
                    return Poll::Ready(Ok(()));
                }
                _ => {
                    // Nothing to do, because we may still start sending and receiving new frames
                    // again.
                }
            }
        }

        Poll::Pending
    }
}

enum ClientReceiverError<T>
where
    T: Sink<Record>,
{
    DecodeCodecError(DecodeCodecError),
    SendError(mpsc::error::SendError<Record>),
    SinkError(<T as Sink<Record>>::Error),
}

impl<T> ClientReceiverError<T>
where
    T: Sink<Record>,
{
    // Can't implement this using the `From` trait due to conflict with the core library.
    pub(crate) fn from_sink_error(e: <T as Sink<Record>>::Error) -> Self {
        ClientReceiverError::SinkError(e)
    }
}

impl<T> From<DecodeCodecError> for ClientReceiverError<T>
where
    T: Sink<Record>,
{
    fn from(value: DecodeCodecError) -> Self {
        ClientReceiverError::DecodeCodecError(value)
    }
}

impl<T> From<mpsc::error::SendError<Record>> for ClientReceiverError<T>
where
    T: Sink<Record>,
{
    fn from(value: mpsc::error::SendError<Record>) -> Self {
        ClientReceiverError::SendError(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{protocol::record::Params, request::Request};
    use futures::{stream, StreamExt};
    use std::net::{Ipv4Addr, SocketAddr};
    use tokio::net::{TcpListener, TcpStream};

    #[tokio::test]
    async fn client_send_parallel() {
        let port = 8080;
        let addr = Ipv4Addr::new(127, 0, 0, 1);

        let stream = TcpStream::connect(SocketAddr::new(addr.into(), port))
            .await
            .unwrap();

        let client = Client::new(stream);

        let data = b"Some data.";

        let requests = [
            Request::builder()
                .keep_conn()
                .params(Params::builder().server_port(port).server_addr(addr.into()))
                .data_now(&data[..])
                .build(),
            Request::builder()
                .keep_conn()
                .params(Params::builder().server_port(port).server_addr(addr.into()))
                .data_now(&data[..])
                .build(),
        ];

        stream::iter(requests)
            .map(|request| {
                let client_ref = client.clone();

                tokio::spawn(async move {
                    let pending = client_ref.send(request).await.unwrap();

                    pending.await
                })
            })
            .buffer_unordered(2)
            .for_each(|result| async { todo!() })
            .await;
    }

    #[tokio::test]
    async fn server_receive_bytes() {
        let listener = TcpListener::bind("127.0.0.1:8080").await.unwrap();

        loop {
            let (stream, _) = listener.accept().await.unwrap();

            handle_connection(stream).await
        }
    }

    async fn handle_connection(stream: TcpStream) {
        let mut msg = vec![0; 1024];

        loop {
            stream.readable().await.unwrap();

            match stream.try_read(&mut msg) {
                Ok(n) => {
                    msg.truncate(n);
                    break;
                }
                Err(ref e) if e.kind() == tokio::io::ErrorKind::WouldBlock => {
                    continue;
                }
                Err(e) => Err(e).unwrap(),
            }
        }

        println!("received: {:?}", msg);
    }
}
