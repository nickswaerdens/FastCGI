use crate::protocol::record::RecordType;
use bytes::BytesMut;

/// Unparsed record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Frame {
    pub(crate) id: u16,
    pub(crate) record_type: RecordType,
    pub(crate) payload: BytesMut,
}

impl Frame {
    pub(crate) fn new(id: u16, record_type: RecordType, payload: BytesMut) -> Self {
        Self {
            id,
            record_type,
            payload,
        }
    }

    pub fn as_parts(&self) -> (u16, RecordType, &BytesMut) {
        (self.id, self.record_type, &self.payload)
    }

    pub fn into_parts(self) -> (u16, RecordType, BytesMut) {
        (self.id, self.record_type, self.payload)
    }
}
