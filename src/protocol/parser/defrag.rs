use bytes::{BufMut, BytesMut};

/// Temporarily stores received stream frames of the same record type.
///
/// The default maximum size of the payload is 64MB (1024 full frames).
#[derive(Debug)]
pub(crate) struct Defrag {
    payloads: Vec<BytesMut>,
    max_total_payload: usize,
    current_total_payload: usize,
}

impl Defrag {
    pub(crate) fn new(max_total_payload: usize) -> Self {
        Self {
            payloads: Vec::new(),
            max_total_payload,
            current_total_payload: 0,
        }
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

    pub(crate) fn handle_end_of_stream(&mut self) -> BytesMut {
        if self.payloads.is_empty() {
            return BytesMut::new();
        }

        // Should this much space be reserved beforehand?
        // The frames drain iter could be chunked, with memory being reserved for each chunk
        // instead.
        let mut buffer = BytesMut::with_capacity(self.current_total_payload);

        for payload in self.payloads.drain(..) {
            buffer.put(payload);
        }

        buffer
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
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
