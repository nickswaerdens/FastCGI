use std::marker::PhantomData;

use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::Framed;

use crate::{
    codec::{DecodeCodecError, EncodeCodecError, FastCgiCodec, Frame},
    meta::{self, Meta},
    record::{Empty, EncodeFrame, EncodeFrameError, Header, IntoStreamChunker, Record},
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

        dbg!("Closed the stream");
    }
}

impl<T, P> Connection<T, P>
where
    T: AsyncRead + Unpin,
    P: Endpoint,
    P::State: Default,
{
    /// Poll for the next, parsed frame.
    pub async fn poll_frame(
        &mut self,
    ) -> Option<Result<Option<<P::State as State>::Output>, ConnectionRecvError>> {
        loop {
            let frame = match self.transport.next().await {
                Some(Ok(x)) => x,
                Some(Err(e)) => return Some(Err(e).map_err(ConnectionRecvError::from)),
                _ => return None,
            };

            let record = self
                .poll_frame_inner(frame)
                .map_err(ConnectionRecvError::from);

            match record {
                Ok(Some(x)) => return Some(Ok(Some(x))),
                Err(e) => return Some(Err(e)),
                _ => {}
            }
        }
    }

    fn poll_frame_inner(
        &mut self,
        frame: Frame,
    ) -> Result<Option<<P::State as State>::Output>, ConnectionRecvError> {
        let record = match self.streams.as_mut() {
            Some(stream) => stream.parse(frame)?,
            None => {
                let mut stream = Stream::default();
                let record = stream.parse(frame)?;

                self.streams.replace(stream);
                record
            }
        };

        Ok(record)
    }
}

impl<T, P> Connection<T, P>
where
    T: AsyncWrite + Unpin,
    P: Endpoint,
{
    pub async fn feed_frame<D>(&mut self, record: Record<D>) -> Result<(), ConnectionSendError>
    where
        D: EncodeFrame,
    {
        self.transport
            .feed(record)
            .await
            .map_err(ConnectionSendError::from)
    }

    pub async fn feed_stream<S>(&mut self, record: Record<S>) -> Result<(), ConnectionSendError>
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

        let empty_record = record.map(|_| Empty::<S::Item>::new());

        self.transport
            .feed(empty_record)
            .await
            .map_err(ConnectionSendError::from)
    }

    pub(crate) async fn feed_empty<S>(&mut self, header: Header) -> Result<(), ConnectionSendError>
    where
        S: Meta<DataKind = meta::Stream>,
    {
        let record = Record::from_parts(header, Empty::<S>::new());

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
pub enum ConnectionRecvError {
    DecodeCodecError(DecodeCodecError),
    ParserError(ParseError),
    UnexpectedEndOfInput,
    StdIoError(std::io::Error),
}

impl From<DecodeCodecError> for ConnectionRecvError {
    fn from(value: DecodeCodecError) -> Self {
        ConnectionRecvError::DecodeCodecError(value)
    }
}

impl From<ParseError> for ConnectionRecvError {
    fn from(value: ParseError) -> Self {
        ConnectionRecvError::ParserError(value)
    }
}

impl From<std::io::Error> for ConnectionRecvError {
    fn from(value: std::io::Error) -> Self {
        ConnectionRecvError::StdIoError(value)
    }
}
