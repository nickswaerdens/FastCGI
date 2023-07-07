use super::{
    Decode, DecodeError, EncodeBuffer, EncodeChunk, EncodeRecordError, NameValuePair,
    NameValuePairs,
};
use crate::request::{Filter, RoleState};
use bytes::BytesMut;
use std::{marker::PhantomData, net::IpAddr, time::SystemTime};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Params {
    inner: NameValuePairs,
}

impl Params {
    pub fn validate(nvp: &NameValuePair) -> bool {
        !nvp.name.inner().is_empty() && nvp.value.is_some()
    }

    pub fn insert_nvp(mut self, nvp: NameValuePair) -> Self {
        self.inner = self.inner.insert_nvp(nvp);
        self
    }

    pub fn builder<R: RoleState>() -> ParamsBuilder<Init, R> {
        ParamsBuilder::new()
    }
}

impl EncodeChunk for Params {
    fn encode_chunk(&mut self, buf: &mut EncodeBuffer) -> Option<Result<(), EncodeRecordError>> {
        self.inner.encode_chunk(buf)
    }
}

impl Decode for Params {
    fn decode(src: BytesMut) -> Result<Self, DecodeError> {
        Ok(Params {
            inner: NameValuePairs::decode(src, Self::validate)?,
        })
    }
}

pub trait BuilderState: Sized {
    /// Params is not allowed to be empty.
    fn transmute_once<R: RoleState>(builder: ParamsBuilder<Self, R>) -> ParamsBuilder<Build, R>;
}

pub struct Init;
pub struct Build;

impl BuilderState for Init {
    fn transmute_once<R: RoleState>(builder: ParamsBuilder<Self, R>) -> ParamsBuilder<Build, R> {
        ParamsBuilder {
            inner: builder.inner,
            _marker: PhantomData,
        }
    }
}

impl BuilderState for Build {
    fn transmute_once<R: RoleState>(builder: ParamsBuilder<Self, R>) -> ParamsBuilder<Build, R> {
        builder
    }
}

pub struct ParamsBuilder<S: BuilderState, R: RoleState> {
    inner: Params,
    _marker: PhantomData<(S, R)>,
}

impl<R: RoleState> ParamsBuilder<Init, R> {
    pub fn new() -> Self {
        Self::default()
    }
}

impl<S: BuilderState, R: RoleState> ParamsBuilder<S, R> {
    pub fn server_port(mut self, port: u16) -> ParamsBuilder<Build, R> {
        let nvp = NameValuePair::new("SERVER_PORT", port.to_string()).unwrap();
        self.inner = self.inner.insert_nvp(nvp);

        S::transmute_once(self)
    }

    pub fn server_addr(mut self, addr: IpAddr) -> ParamsBuilder<Build, R> {
        let nvp = NameValuePair::new("SERVER_ADDR", addr.to_string()).unwrap();
        self.inner = self.inner.insert_nvp(nvp);

        S::transmute_once(self)
    }
}

impl<S: BuilderState> ParamsBuilder<S, Filter> {
    pub(crate) fn data_last_mod(
        mut self,
        data_last_mod: SystemTime,
    ) -> ParamsBuilder<Build, Filter> {
        let data_last_mod = data_last_mod
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("Last modified must be >= 1970-01-01 00:00:00 UTC")
            .as_secs();

        let nvp = NameValuePair::new("FCGI_DATA_LAST_MOD", data_last_mod.to_string()).unwrap();
        self.inner = self.inner.insert_nvp(nvp);

        S::transmute_once(self)
    }

    pub(crate) fn data_length(mut self, data_length: u64) -> ParamsBuilder<Build, Filter> {
        let nvp = NameValuePair::new("FCGI_DATA_LENGTH", data_length.to_string()).unwrap();
        self.inner = self.inner.insert_nvp(nvp);

        S::transmute_once(self)
    }
}

impl<R: RoleState> ParamsBuilder<Build, R> {
    pub fn build(self) -> Params {
        self.inner
    }
}

impl<R: RoleState> Default for ParamsBuilder<Init, R> {
    fn default() -> Self {
        ParamsBuilder {
            inner: Params {
                inner: NameValuePairs::default(),
            },
            _marker: PhantomData,
        }
    }
}
