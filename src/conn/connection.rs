use futures::{SinkExt, StreamExt};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio_util::codec::{Encoder, Framed};

use crate::{
    codec::{DecodeCodecError, EncodeCodecError, FastCgiCodec, Frame},
    meta::{self, Meta},
    record::{
        Data, Empty, EncodeFrame, EncodeFrameError, Header, Id, IntoStreamFragmenter, Record,
    },
};

use super::{
    parser::{Parser, ParserError},
    stream::Stream,
};

#[derive(Debug)]
pub(crate) struct Connection<T, P: Parser> {
    transport: Framed<T, FastCgiCodec>,

    // Currently supports simplexed connections only.
    streams: Option<Stream<P>>,
}

impl<T: AsyncRead + AsyncWrite, P: Parser> Connection<T, P> {
    pub fn new(transport: T) -> Self {
        Self {
            transport: Framed::new(transport, FastCgiCodec::new()),

            streams: None,
        }
    }
}

impl<T, P> Connection<T, P>
where
    P: Parser,
{
    pub(crate) fn close_stream(&mut self) {
        // TODO
        self.streams = None;

        dbg!("Closed the stream");
    }
}

impl<T, P> Connection<T, P>
where
    T: AsyncRead + Unpin,
    P: Parser,
{
    /// Poll for the next, parsed frame.
    pub(crate) async fn poll_frame(
        &mut self,
    ) -> Option<Result<Option<P::Output>, ConnectionRecvError>> {
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

    fn poll_frame_inner(&mut self, frame: Frame) -> Result<Option<P::Output>, ConnectionRecvError> {
        let record = match self.streams.as_mut() {
            Some(stream) => stream.parse_frame(frame)?,
            None => {
                let mut stream = Stream::default();
                let record = stream.parse_frame(frame)?;

                self.streams = Some(stream);
                record
            }
        };

        Ok(record)
    }
}

impl<T, P> Connection<T, P>
where
    T: AsyncWrite + Unpin,
    P: Parser,
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
        S: IntoStreamFragmenter,
    {
        let (header, data) = record.into_parts();

        for fragment in data.into_stream_fragmenter() {
            let fragment = fragment?;

            self.transport
                .feed(Record::from_parts(header, fragment))
                .await?;
        }

        self.transport
            .feed(Record::from_parts(header, Empty::<S::Item>::new()))
            .await?;

        Ok(())
    }

    pub(crate) async fn feed_empty<R>(&mut self, id: Id) -> Result<(), ConnectionSendError>
    where
        Empty<R>: Meta<DataKind = meta::Stream>,
    {
        self.transport
            .feed(Record::from_parts(
                Header::from_meta::<Empty<R>>(id),
                Empty::<R>::new(),
            ))
            .await?;

        Ok(())
    }

    pub(crate) async fn flush(&mut self) -> Result<(), ConnectionSendError>
    where
        FastCgiCodec: Encoder<Record<Empty<Data>>>,
    {
        // TODO: Figure out this type annotation, currently set to Empty<Data> as it doesn't appear to do anything.
        <Framed<T, FastCgiCodec> as SinkExt<Record<Empty<Data>>>>::flush(&mut self.transport)
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
    ParserError(ParserError),
    UnexpectedEndOfInput,
    StdIoError(std::io::Error),
}

impl From<DecodeCodecError> for ConnectionRecvError {
    fn from(value: DecodeCodecError) -> Self {
        ConnectionRecvError::DecodeCodecError(value)
    }
}

impl From<ParserError> for ConnectionRecvError {
    fn from(value: ParserError) -> Self {
        ConnectionRecvError::ParserError(value)
    }
}

impl From<std::io::Error> for ConnectionRecvError {
    fn from(value: std::io::Error) -> Self {
        ConnectionRecvError::StdIoError(value)
    }
}
