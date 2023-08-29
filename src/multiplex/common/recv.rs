use std::{
    pin::Pin,
    task::{Context, Poll},
};

use futures::ready;
use pin_project::pin_project;
use tokio::sync::mpsc;

use crate::{
    multiplex::PendingConfig,
    protocol::{
        meta::{self, DataKind, Discrete, Stream},
        parser::defrag::MaximumStreamSizeExceeded,
        record::{Decode, UnknownType},
        transport::Frame,
    },
};

use super::decoder::{DiscreteDecoder, IntoDecoder, StreamDecoder};

pub(crate) trait PollRecv {
    type Output;
    type Error;

    fn poll_recv(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Output, RecvError<Self::Error>>>;
}

#[pin_project]
pub(crate) struct Recv<T, D>
where
    T: IntoDecoder<D>,
    D: AsRecvConfig,
{
    pub(crate) rx: mpsc::Receiver<Frame>,
    pub(crate) decoder: T::Decoder,

    pub(crate) config: D::Config,
}

impl<T> PollRecv for Recv<T, Discrete>
where
    T: meta::ManagementRecord<DataKind = Discrete>
        + Decode
        + IntoDecoder<Discrete, Decoder = DiscreteDecoder>,
{
    type Output = T;
    type Error = T::Error;

    fn poll_recv(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Output, RecvError<Self::Error>>> {
        let this = self.project();

        match ready!(this.rx.poll_recv(cx)) {
            Some(Frame {
                id,
                record_type,
                payload,
            }) => {
                assert_eq!(id, 0);
                assert_eq!(record_type, <T as meta::ManagementRecord>::TYPE);

                Poll::Ready(T::decode(payload).map_err(RecvError::DecodeError))
            }
            None => {
                // Channel closed before receiving the frame.
                Poll::Ready(Err(RecvError::RecvChannelClosedEarly))
            }
        }
    }
}

impl<T> PollRecv for Recv<T, Stream>
where
    T: meta::ManagementRecord<DataKind = Stream>
        + Decode
        + IntoDecoder<Stream, Decoder = StreamDecoder>,
{
    type Output = T;
    type Error = StreamDecodeError<T::Error>;

    fn poll_recv(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Result<Self::Output, RecvError<Self::Error>>> {
        let this = self.project();
        let yield_at = this.config.yield_at;

        for _ in 0..yield_at {
            match ready!(this.rx.poll_recv(cx)) {
                Some(Frame {
                    id,
                    record_type,
                    payload,
                }) => {
                    assert_eq!(id, 0);
                    assert_eq!(record_type, <T as meta::ManagementRecord>::TYPE);

                    if payload.is_empty() {
                        let payload = this.decoder.inner.handle_end_of_stream();

                        let decoded = T::decode(payload).map_err(StreamDecodeError::DecodeError)?;
                        return Poll::Ready(Ok(decoded));
                    } else {
                        this.decoder.inner.insert_payload(payload)?
                    }
                }
                None => {
                    // Channel closed before receiving the end of stream frame.
                    return Poll::Ready(Err(RecvError::RecvChannelClosedEarly));
                }
            }
        }

        // Let the task know we're immediately ready to receive more frames.
        cx.waker().wake_by_ref();

        Poll::Pending
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecvError<T> {
    DecodeError(T),
    UnknownType(UnknownType),
    RecvChannelClosedEarly,
}

pub enum StreamDecodeError<T> {
    DecodeError(T),
    MaximumStreamSizeExceeded(MaximumStreamSizeExceeded),
}

impl<T> From<StreamDecodeError<T>> for RecvError<StreamDecodeError<T>> {
    fn from(value: StreamDecodeError<T>) -> Self {
        RecvError::DecodeError(value)
    }
}

impl<T> From<MaximumStreamSizeExceeded> for RecvError<StreamDecodeError<T>> {
    fn from(value: MaximumStreamSizeExceeded) -> Self {
        RecvError::DecodeError(StreamDecodeError::MaximumStreamSizeExceeded(value))
    }
}

impl<T> From<UnknownType> for RecvError<T> {
    fn from(value: UnknownType) -> Self {
        RecvError::UnknownType(value)
    }
}

// config

pub(crate) trait AsRecvConfig: DataKind {
    type Config: for<'a> From<&'a PendingConfig>;
}

pub(crate) struct RecvDiscreteConfig(());

pub(crate) struct RecvStreamConfig {
    recv_channel_limit: usize,
    yield_at: usize,
    max_stream_payload_size: usize,
}

impl AsRecvConfig for Discrete {
    type Config = RecvDiscreteConfig;
}

impl AsRecvConfig for Stream {
    type Config = RecvStreamConfig;
}

impl From<&PendingConfig> for RecvDiscreteConfig {
    fn from(_: &PendingConfig) -> Self {
        Self(())
    }
}

impl From<&PendingConfig> for RecvStreamConfig {
    fn from(value: &PendingConfig) -> Self {
        Self {
            recv_channel_limit: value.recv_channel_limit,
            yield_at: value.yield_at,
            max_stream_payload_size: value.max_stream_payload_size,
        }
    }
}
