use bytes::{buf::UninitSlice, BufMut};

use super::RingBuffer;

/// A Wrapper struct around a RingBuffer.
///
/// This struct provides write-only access to the underlying RingBuffer.
pub struct Buffer<'buf> {
    inner: &'buf mut RingBuffer,
}

impl RingBuffer {
    /// Adds a write_only method to the underlying RingBuffer.
    pub fn write_only(&mut self) -> Buffer {
        Buffer { inner: self }
    }
}

impl<'buf> Buffer<'buf> {
    /// Returns the number of bytes that the buf can hold.
    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }

    /// Return the read cursor position
    pub fn position(&self) -> u64 {
        self.inner.position()
    }

    /// Return the number of buffered bytes
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Returns `true` if the buf cannot accept any further reads.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns the number of bytes remaining to read.
    pub fn remaining_read(&self) -> usize {
        self.inner.remaining_read()
    }

    /// Returns the remaining write capacity until which the buf becomes full.
    pub fn remaining_write(&self) -> usize {
        self.inner.remaining_write()
    }
}

unsafe impl<'buf> BufMut for Buffer<'buf>
where
    RingBuffer: BufMut,
{
    fn remaining_mut(&self) -> usize {
        self.inner.remaining_write()
    }

    unsafe fn advance_mut(&mut self, cnt: usize) {
        self.inner.advance_mut(cnt)
    }

    fn chunk_mut(&mut self) -> &mut UninitSlice {
        self.inner.chunk_mut()
    }
}
