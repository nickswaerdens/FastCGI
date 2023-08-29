use crate::protocol::record::Padding;
use std::time::Duration;

#[derive(Debug)]
pub struct Config {
    pub send_channel_limit: usize,

    pub receiver_config: ReceiverConfig,
    pub pending_config: PendingConfig,
}

#[derive(Debug)]
pub struct ReceiverConfig {
    pub yield_sender_after: usize,
    pub yield_receiver_after: Option<usize>,
    pub padding: Option<Padding>,
    // max_conns: NonZeroU16,
    // max_reqs: NonZeroU16,
    // mpsx_conns: NonZeroU16,
}

#[derive(Debug)]
pub struct PendingConfig {
    pub recv_channel_limit: usize,
    pub timeout: Duration,
    pub yield_at: usize,
    pub max_stream_payload_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            send_channel_limit: 32,
            receiver_config: Default::default(),
            pending_config: Default::default(),
        }
    }
}

impl Default for ReceiverConfig {
    fn default() -> Self {
        Self {
            yield_sender_after: 32,
            yield_receiver_after: None,
            padding: Some(Padding::default()),
        }
    }
}

impl Default for PendingConfig {
    fn default() -> Self {
        Self {
            recv_channel_limit: 32,
            timeout: Duration::from_secs(60),
            yield_at: 32,
            max_stream_payload_size: 0x4000000, // 64 MB
        }
    }
}
