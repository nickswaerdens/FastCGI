use std::{
    pin::Pin,
    task::{Context, Poll},
};

use bytes::{BufMut, BytesMut};
use futures::{ready, Sink};
use pin_project::pin_project;
use tokio_util::sync::{PollSendError, PollSender};

use crate::{
    multiplex::{Command, PendingConfig},
    protocol::{
        meta::{self, DataKind, Discrete, Stream},
        record::{EncodeChunk, EncodeRecord, EncodeRecordError, ManagementRecord},
    },
};

use super::encoder::{DiscreteEncoder, IntoEncoder, StreamEncoder};

pub(crate) trait PollSend {
    fn poll_send(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), SendError>>;

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), SendError>>;
}

#[pin_project]
pub(crate) struct Send<T, D>
where
    T: IntoEncoder<D>,
    D: AsSendConfig,
{
    #[pin]
    pub(crate) tx_command: PollSender<Command>,
    pub(crate) encoder: T::Encoder,
    pub(crate) buf: BytesMut,

    pub(crate) config: D::Config,
}

impl<T> PollSend for Send<T, Discrete>
where
    T: meta::ManagementRecord<DataKind = Discrete>
        + EncodeRecord
        + IntoEncoder<Discrete, Encoder = DiscreteEncoder<T>>,
{
    fn poll_send(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), SendError>> {
        let mut this = self.project();

        ready!(this.tx_command.as_mut().poll_reserve(cx)?);

        this.encoder
            .encode(&mut this.buf.limit(u16::MAX as usize))?;

        let record = ManagementRecord::new(<T as meta::ManagementRecord>::TYPE, this.buf.split());

        /*
        tx.as_mut()
            .send_item(Command::Sendmeta::ManagementRecord(record))
            // This error doesn't happen if we get through poll_reserve without
            // error.
            .unwrap();
        */

        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), SendError>> {
        self.project()
            .tx_command
            .poll_close(cx)
            .map_err(SendError::from)
    }
}

impl<T> PollSend for Send<T, Stream>
where
    T: meta::ManagementRecord<DataKind = Stream>
        + EncodeChunk
        + IntoEncoder<Stream, Encoder = StreamEncoder<T>>,
{
    fn poll_send(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), SendError>> {
        let this = self.project();
        let mut tx = this.tx_command;

        let mut iteration_count = 0;
        while tx.poll_reserve(cx)?.is_ready() {
            if this
                .encoder
                .encode(&mut this.buf.limit(u16::MAX as usize))
                .transpose()?
                .is_some()
            {
                let record =
                    ManagementRecord::new(<T as meta::ManagementRecord>::TYPE, this.buf.split());

                /*
                tx.send_item(Command::Sendmeta::ManagementRecord(record))
                    // This error doesn't happen if we get through poll_reserve without
                    // error.
                    .unwrap();
                */

                iteration_count += 1;
                if iteration_count == this.config.yield_at {
                    cx.waker().wake_by_ref();

                    break;
                }
            } else {
                // Handle end of stream.
                let record = ManagementRecord::empty::<T>();

                /*
                tx.send_item(Command::Sendmeta::ManagementRecord(record))
                    // This error doesn't happen if we get through poll_reserve without
                    // error.
                    .unwrap();
                */

                return Poll::Ready(Ok(()));
            }
        }

        Poll::Pending
    }

    fn poll_close(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), SendError>> {
        self.project()
            .tx_command
            .poll_close(cx)
            .map_err(SendError::from)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendError {
    SenderError,
    EncodeError(EncodeRecordError),
}

impl From<PollSendError<Command>> for SendError {
    fn from(_: PollSendError<Command>) -> Self {
        SendError::SenderError
    }
}

impl From<EncodeRecordError> for SendError {
    fn from(value: EncodeRecordError) -> Self {
        SendError::EncodeError(value)
    }
}

// config

pub(crate) trait AsSendConfig: DataKind {
    type Config: for<'a> From<&'a PendingConfig>;
}

pub(crate) struct SendDiscreteConfig(());

pub(crate) struct SendStreamConfig {
    yield_at: usize,
}

impl AsSendConfig for Discrete {
    type Config = SendDiscreteConfig;
}

impl AsSendConfig for Stream {
    type Config = SendStreamConfig;
}

impl From<&PendingConfig> for SendDiscreteConfig {
    fn from(_: &PendingConfig) -> Self {
        Self(())
    }
}

impl From<&PendingConfig> for SendStreamConfig {
    fn from(value: &PendingConfig) -> Self {
        Self {
            yield_at: value.yield_at,
        }
    }
}
