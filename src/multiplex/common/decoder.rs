use crate::{
    multiplex::PendingConfig,
    protocol::{
        meta::{self, DataKind},
        parser::defrag::Defrag,
        record::Decode,
    },
};

pub(crate) trait IntoDecoder<T: DataKind>: Decode<DataKind = T> {
    type Decoder;
    type Config: for<'a> From<&'a PendingConfig>;

    fn decoder(config: Self::Config) -> Self::Decoder;
}

pub(crate) struct DiscreteDecoder;

pub(crate) struct StreamDecoder {
    pub(crate) inner: Defrag,
}

impl<T: Decode<DataKind = meta::Discrete>> IntoDecoder<meta::Discrete> for T {
    type Decoder = DiscreteDecoder;
    type Config = DiscreteDecoderConfig;

    fn decoder(_: Self::Config) -> Self::Decoder {
        DiscreteDecoder
    }
}

impl<T: Decode<DataKind = meta::Stream>> IntoDecoder<meta::Stream> for T {
    type Decoder = StreamDecoder;
    type Config = StreamDecoderConfig;

    fn decoder(config: Self::Config) -> Self::Decoder {
        StreamDecoder {
            inner: Defrag::new(config.max_total_payload),
        }
    }
}

pub(crate) struct DiscreteDecoderConfig(());

pub(crate) struct StreamDecoderConfig {
    max_total_payload: usize,
}

impl From<&PendingConfig> for DiscreteDecoderConfig {
    fn from(_: &PendingConfig) -> Self {
        Self(())
    }
}

impl From<&PendingConfig> for StreamDecoderConfig {
    fn from(value: &PendingConfig) -> Self {
        Self {
            max_total_payload: value.max_stream_payload_size,
        }
    }
}
