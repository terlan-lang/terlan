//! Pull-driven body for the finite chunks supplied to `Response.stream`.

use std::collections::VecDeque;

use bytes::Bytes;

/// Keeps source chunks separate and emits at most one bounded write per pull.
/// The transport requests the next chunk only when it has write capacity, so
/// there is at most one pending write, within every admitted positive bound.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpResponseChunks {
    source: VecDeque<Bytes>,
    chunk_size: usize,
}

/// A streaming bound was zero, negative, or outside the host index range.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InvalidHttpStreamLimits;

impl std::fmt::Display for InvalidHttpStreamLimits {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("stream limits must be positive host-sized integers")
    }
}

impl std::error::Error for InvalidHttpStreamLimits {}

impl VmHttpResponseChunks {
    /// Validates source limits before any headers or body bytes are emitted.
    pub(crate) fn new(
        source: Vec<Bytes>,
        chunk_size: i64,
        max_pending_writes: i64,
    ) -> Result<Self, InvalidHttpStreamLimits> {
        let invalid = InvalidHttpStreamLimits;
        let chunk_size = usize::try_from(chunk_size).map_err(|_| invalid)?;
        let max_pending_writes = usize::try_from(max_pending_writes).map_err(|_| invalid)?;
        if chunk_size == 0 || max_pending_writes == 0 {
            return Err(invalid);
        }
        Ok(Self {
            source: source
                .into_iter()
                .filter(|chunk| !chunk.is_empty())
                .collect(),
            chunk_size,
        })
    }

    /// Transfers the next byte slice without joining or copying source chunks.
    pub(crate) fn next_chunk(&mut self) -> Option<Bytes> {
        let chunk = self.source.front_mut()?;
        if chunk.len() <= self.chunk_size {
            self.source.pop_front()
        } else {
            Some(chunk.split_to(self.chunk_size))
        }
    }

    /// Reports completion without advancing the stream.
    pub(crate) fn is_complete(&self) -> bool {
        self.source.is_empty()
    }
}
