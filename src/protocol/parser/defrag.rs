use bytes::{BufMut, BytesMut};

/// Temporarily stores received stream frames of the same record type.
///
/// The default maximum size of the payload is 64MB (1024 full frames). This can be adjusted
/// with `with_max_payload_size`. As the project is at an early stage, it's recommended to
/// manually set the maximum to avoid unexpected changes to the maximum payload size in the
/// future.
#[derive(Debug)]
pub(crate) struct Defrag {
    payloads: Vec<BytesMut>,
    max_total_payload: usize,
    current_total_payload: usize,
}

impl Defrag {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_max_payload_size(mut self, n: usize) -> Self {
        self.max_total_payload = n;
        self
    }

    pub(crate) fn insert_payload(
        &mut self,
        payload: BytesMut,
    ) -> Result<(), MaximumStreamSizeExceeded> {
        let new_size = self.current_total_payload + payload.len();

        if self.max_total_payload < new_size {
            Err(MaximumStreamSizeExceeded::new(
                new_size,
                self.max_total_payload,
            ))?;
        }

        self.payloads.push(payload);
        self.current_total_payload = new_size;

        Ok(())
    }

    pub(crate) fn handle_end_of_stream(&mut self) -> Option<BytesMut> {
        if self.payloads.is_empty() {
            return None;
        }

        // Should this much space be reserved beforehand?
        // The frames drain iter could be chunked, with memory being reserved for each chunk
        // instead.
        let mut buffer = BytesMut::with_capacity(self.current_total_payload);

        for payload in self.payloads.drain(..) {
            buffer.put(payload);
        }

        Some(buffer)
    }
}

impl Default for Defrag {
    fn default() -> Self {
        Self {
            payloads: Vec::new(),
            max_total_payload: 0x4000000, // 64 MB
            current_total_payload: 0,
        }
    }
}

pub struct MaximumStreamSizeExceeded {
    size: usize,
    limit: usize,
}

impl MaximumStreamSizeExceeded {
    pub fn new(size: usize, limit: usize) -> Self {
        Self { size, limit }
    }
}

impl std::fmt::Debug for MaximumStreamSizeExceeded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "The stream has exceeded it's maximum allowed size [{} < {}].",
            self.size, self.limit
        )
    }
}
