use crate::protocol::{
    meta::{self, DataKind},
    record::{
        EncodeBuffer, EncodeChunk, EncodeRecord, EncodeRecordError, IntoStreamChunker,
        StreamChunker,
    },
};

pub(crate) trait IntoEncoder<T: DataKind> {
    type Encoder;

    fn encoder(self) -> Self::Encoder;
}

pub(crate) struct DiscreteEncoder<T: EncodeRecord> {
    pub(crate) inner: Option<T>,
}

pub(crate) struct StreamEncoder<T: EncodeChunk> {
    pub(crate) inner: StreamChunker<T>,
}

impl<T: EncodeRecord> DiscreteEncoder<T> {
    pub fn encode(&mut self, dst: &mut EncodeBuffer) -> Result<(), EncodeRecordError> {
        self.inner
            .take()
            // TODO: turn this into a more specific error here, differentiate between discrete and stream errors.
            .map_or(Err(EncodeRecordError::MaxFrameSizeExceeded), |record| {
                record.encode_record(dst)
            })
    }
}

impl<T: EncodeChunk> StreamEncoder<T> {
    pub fn encode(&mut self, dst: &mut EncodeBuffer) -> Option<Result<(), EncodeRecordError>> {
        self.inner.encode(dst)
    }
}

impl<T> IntoEncoder<meta::Discrete> for T
where
    T: EncodeRecord,
{
    type Encoder = DiscreteEncoder<T>;

    fn encoder(self) -> Self::Encoder {
        DiscreteEncoder { inner: Some(self) }
    }
}

impl<T> IntoEncoder<meta::Stream> for T
where
    T: EncodeChunk + IntoStreamChunker,
{
    type Encoder = StreamEncoder<T>;

    fn encoder(self) -> Self::Encoder {
        StreamEncoder {
            inner: self.into_stream_chunker(),
        }
    }
}
