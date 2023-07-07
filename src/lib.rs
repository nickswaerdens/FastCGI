use std::num::NonZeroU16;

mod macros;
pub mod multiplex;
pub mod protocol;
pub mod request;
pub mod response;

pub const FCGI_VERSION_1: u8 = 1;

pub const HEADER_SIZE: u8 = 8;
pub const DEFAULT_MAX_PAYLOAD_SIZE: u16 = u16::MAX;

pub(crate) const MANAGEMENT_ID: u16 = 0;
pub(crate) type ApplicationId = NonZeroU16;
