use std::marker::PhantomData;

use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::Framed;

use crate::{
    codec::{DecodeCodecError, EncodeCodecError, FastCgiCodec, Frame},
    meta::{self, Meta},
    record::{
        EncodeFrame, EncodeFrameError, EndOfStream, Header, IntoStreamChunker, ProtocolStatus,
        Record,
    },
};

use super::{
    endpoint::Endpoint,
    state::{ParseError, State},
    stream::Stream,
};

#[derive(Debug)]
pub(crate) struct Connection<T, P: Endpoint> {
    transport: Framed<T, FastCgiCodec>,

    // Currently supports simplexed connections only.
    streams: Option<Stream<P::State>>,
    _marker: PhantomData<P>,
}

impl<T: AsyncRead + AsyncWrite, P: Endpoint> Connection<T, P> {
    pub fn new(transport: T) -> Self {
        Self {
            transport: Framed::new(transport, FastCgiCodec::new()),

            streams: None,
            _marker: PhantomData,
        }
    }
}

impl<T, P> Connection<T, P>
where
    P: Endpoint,
{
    pub fn close_stream(&mut self) {
        // TODO
        self.streams.take();

        // TODO, log this.
        // dbg!("Closed the stream");
    }
}

impl<T, P> Connection<T, P>
where
    T: AsyncRead + Unpin,
    P: Endpoint,
{
    /// Poll for the next, parsed frame.
    pub async fn poll_frame(
        &mut self,
    ) -> Option<
        Result<
            Option<<P::State as State>::Output>,
            ConnectionRecvError<<P::State as State>::Error>,
        >,
    > {
        loop {
            let frame = match self.transport.next().await {
                Some(Ok(frame)) => frame,
                Some(Err(e)) => return Some(Err(e).map_err(ConnectionRecvError::from)),
                _ => return None,
            };

            if frame.header.id == 0 {
                // Handle management frames.
                dbg!("Frame ignored: management records are currently not supported.");
            } else {
                match self.poll_frame_inner(frame) {
                    Ok(part) => return Some(Ok(part)),
                    Err(e) => return Some(Err(ConnectionRecvError::from(e))),
                }
            }
        }
    }

    fn poll_frame_inner(
        &mut self,
        frame: Frame,
    ) -> Result<Option<<P::State as State>::Output>, <P::State as State>::Error> {
        if let Some(stream) = self.streams.as_mut() {
            Ok(stream.parse(frame)?)
        } else {
            // Create a new stream state.
            // TODO: id must be available.
            let mut stream = Stream::default();
            let record = stream.parse(frame)?;

            self.streams.replace(stream);
            Ok(record)
        }
    }
}

impl<T, P> Connection<T, P>
where
    T: AsyncWrite + Unpin,
    P: Endpoint,
{
    pub(crate) async fn feed_frame<D>(
        &mut self,
        record: Record<D>,
    ) -> Result<(), ConnectionSendError>
    where
        D: EncodeFrame,
    {
        self.transport
            .feed(record)
            .await
            .map_err(ConnectionSendError::from)
    }

    pub(crate) async fn feed_stream<S>(
        &mut self,
        record: Record<S>,
    ) -> Result<(), ConnectionSendError>
    where
        S: IntoStreamChunker,
    {
        let mut record = record.map(|body| body.into_stream_chunker());

        loop {
            if record.body.is_empty() {
                break;
            }

            self.transport.feed(&mut record).await?;
        }

        let record = record.map(|_| EndOfStream::<S::Item>::new());

        self.transport
            .feed(record)
            .await
            .map_err(ConnectionSendError::from)
    }

    pub(crate) async fn feed_empty<S>(&mut self, header: Header) -> Result<(), ConnectionSendError>
    where
        S: Meta<DataKind = meta::Stream>,
    {
        let record = Record::from_parts(header, EndOfStream::<S>::new());

        self.transport
            .feed(record)
            .await
            .map_err(ConnectionSendError::from)
    }

    pub(crate) async fn flush(&mut self) -> Result<(), ConnectionSendError> {
        // TODO: Figure out this necessary type annotation, currently set to () as it doesn't appear to do anything.
        <Framed<T, FastCgiCodec> as SinkExt<()>>::flush(&mut self.transport)
            .await
            .map_err(ConnectionSendError::from)
    }
}

#[derive(Debug)]
pub enum ConnectionSendError {
    EncodeCodecError(EncodeCodecError),
    EncodeFrameError(EncodeFrameError),
}

impl From<EncodeCodecError> for ConnectionSendError {
    fn from(value: EncodeCodecError) -> Self {
        ConnectionSendError::EncodeCodecError(value)
    }
}

impl From<EncodeFrameError> for ConnectionSendError {
    fn from(value: EncodeFrameError) -> Self {
        ConnectionSendError::EncodeFrameError(value)
    }
}

#[derive(Debug)]
pub enum ConnectionRecvError<T: ParseError> {
    DecodeCodecError(DecodeCodecError),
    ParserError(T),
    ProtocolStatus(ProtocolStatus),
    UnexpectedEndOfInput,
    StdIoError(std::io::Error),
}

impl<T: ParseError> From<DecodeCodecError> for ConnectionRecvError<T> {
    fn from(value: DecodeCodecError) -> Self {
        ConnectionRecvError::DecodeCodecError(value)
    }
}

impl<T: ParseError> From<T> for ConnectionRecvError<T> {
    fn from(value: T) -> Self {
        ConnectionRecvError::ParserError(value)
    }
}

impl<T: ParseError> From<ProtocolStatus> for ConnectionRecvError<T> {
    fn from(value: ProtocolStatus) -> Self {
        ConnectionRecvError::ProtocolStatus(value)
    }
}

impl<T: ParseError> From<std::io::Error> for ConnectionRecvError<T> {
    fn from(value: std::io::Error) -> Self {
        ConnectionRecvError::StdIoError(value)
    }
}
