use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Instant,
};

use bytes::BytesMut;
use futures::{ready, Future};
use pin_project::pin_project;
use tokio::sync::mpsc;
use tokio_util::sync::{PollSendError, PollSender};

use crate::{
    multiplex::common::{
        decoder::IntoDecoder,
        encoder::IntoEncoder,
        recv::{AsRecvConfig, PollRecv, Recv, RecvError},
        send::{AsSendConfig, PollSend, Send, SendError},
    },
    protocol::{
        meta::{Client, ManagementRecord, Server},
        transport::Frame,
    },
};

use super::{Command, PendingConfig};

#[derive(Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Sending,
    StoppedSending,
    Flushed,
}

#[pin_project]
struct Pending<T>
where
    T: ManagementRecord<Endpoint = Client> + IntoEncoder<T::DataKind>,
    T::DataKind: AsSendConfig,

    T::Dual:
        ManagementRecord<Endpoint = Server> + IntoDecoder<<T::Dual as ManagementRecord>::DataKind>,
    <T::Dual as ManagementRecord>::DataKind: AsRecvConfig,
{
    state: State,

    #[pin]
    send: Send<T, T::DataKind>,

    #[pin]
    recv: Recv<T::Dual, <T::Dual as ManagementRecord>::DataKind>,

    expires_at: Instant,
}

impl<T> Pending<T>
where
    T: ManagementRecord<Endpoint = Client> + IntoEncoder<T::DataKind>,
    T::DataKind: AsSendConfig,

    T::Dual:
        ManagementRecord<Endpoint = Server> + IntoDecoder<<T::Dual as ManagementRecord>::DataKind>,
    <T::Dual as ManagementRecord>::DataKind: AsRecvConfig,

    Send<T, T::DataKind>: PollSend,
    Recv<T::Dual, <T::Dual as ManagementRecord>::DataKind>: PollRecv,
{
    pub fn new(
        req: T,
        tx_command: PollSender<Command>,
        rx: mpsc::Receiver<Frame>,
        config: &PendingConfig,
    ) -> Self {
        Self {
            state: State::default(),

            send: Send {
                tx_command,
                encoder: req.encoder(),
                buf: BytesMut::new(),

                config: config.into(),
            },

            recv: Recv {
                rx,
                decoder: <T::Dual as IntoDecoder<<T::Dual as ManagementRecord>::DataKind>>::decoder(
                    config.into(),
                ),

                config: config.into(),
            },

            expires_at: Instant::now()
                .checked_add(config.timeout)
                .expect("config.timeout exceeded it's maximum value."),
        }
    }
}

impl<T> Future for Pending<T>
where
    T: ManagementRecord<Endpoint = Client> + IntoEncoder<T::DataKind>,
    T::DataKind: AsSendConfig,

    T::Dual:
        ManagementRecord<Endpoint = Server> + IntoDecoder<<T::Dual as ManagementRecord>::DataKind>,
    <T::Dual as ManagementRecord>::DataKind: AsRecvConfig,

    Send<T, T::DataKind>: PollSend,
    Recv<T::Dual, <T::Dual as ManagementRecord>::DataKind>: PollRecv,
{
    type Output = Result<
        <Recv<T::Dual, <T::Dual as ManagementRecord>::DataKind> as PollRecv>::Output,
        PendingError<<Recv<T::Dual, <T::Dual as ManagementRecord>::DataKind> as PollRecv>::Error>,
    >;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let mut send = this.send;
        let mut recv = this.recv;

        if *this.expires_at <= Instant::now() {
            send.tx_command.abort_send();
            recv.rx.close();

            return Poll::Ready(Err(PendingError::Expired));
        }

        // Send the full record.
        if *this.state == State::Sending {
            ready!(send.as_mut().poll_send(cx)?);

            *this.state = State::StoppedSending;
        }

        // Ensure all data was sent.
        if *this.state == State::StoppedSending {
            ready!(send.as_mut().poll_close(cx)?);

            *this.state = State::Flushed;
        }

        // Wait for a response.
        recv.poll_recv(cx).map_err(PendingError::from)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PendingError<T> {
    Expired,
    SendError(SendError),
    RecvError(RecvError<T>),
}

impl<T> From<PollSendError<Command>> for PendingError<T> {
    fn from(_: PollSendError<Command>) -> Self {
        PendingError::SendError(SendError::SenderError)
    }
}

impl<T> From<SendError> for PendingError<T> {
    fn from(value: SendError) -> Self {
        PendingError::SendError(value)
    }
}

impl<T> From<RecvError<T>> for PendingError<T> {
    fn from(value: RecvError<T>) -> Self {
        PendingError::RecvError(value)
    }
}

#[cfg(test)]
mod tests {
    use core::panic;

    use crate::protocol::record::{GetValues, NameValuePairs};

    use super::*;

    #[tokio::test]
    async fn compile_client() {
        let config = PendingConfig::default();

        let (tx, _rx) = mpsc::channel(16);
        let (_tx, rx) = mpsc::channel(16);

        let tx = PollSender::new(tx);

        let record = GetValues(NameValuePairs::new());
        let pending = Pending::new(record, tx, rx, &config).await;

        dbg!(pending);

        panic!("This test is not supposed to be run yet. It only exists to test compile-time errors right now.");
    }
}
