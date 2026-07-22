use std::collections::VecDeque;

use super::super::framing::{classify_tcp_error, VmFramingError, VmInMemoryFrameReader};
use super::super::process::VmProcessId;
use super::super::tcp::VmTcpRuntime;
use super::{VmHttpStaticError, VmHttpStreamPlan};

/// Lifecycle state for one VM-owned HTTP response stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmHttpStreamState {
    Open,
    Finishing,
    Complete,
    Aborted,
}

/// Inspectable response-stream pressure and progress.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpStreamInfo {
    pub(crate) state: VmHttpStreamState,
    pub(crate) pending_writes: usize,
    pub(crate) max_pending_writes: usize,
    pub(crate) emitted_chunks: usize,
    pub(crate) emitted_bytes: usize,
}

/// One scheduler-visible attempt to move a response chunk onto VM TCP.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmHttpStreamTcpFlush {
    Idle,
    Parked,
    Written {
        bytes: usize,
        state: VmHttpStreamState,
    },
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Internal result of attempting to write one body chunk to VM TCP.
pub(super) enum VmHttpTcpWrite {
    Parked,
    Written(usize),
}

/// Pollable bounded response stream owned by the VM scheduler lane.
#[derive(Debug)]
pub(crate) struct VmHttpResponseStream {
    plan: VmHttpStreamPlan,
    pending: VecDeque<Vec<u8>>,
    state: VmHttpStreamState,
    emitted_chunks: usize,
    emitted_bytes: usize,
}

impl VmHttpResponseStream {
    /// Creates an empty open response stream under a validated plan.
    pub(super) fn new(plan: VmHttpStreamPlan) -> Self {
        Self {
            plan,
            pending: VecDeque::new(),
            state: VmHttpStreamState::Open,
            emitted_chunks: 0,
            emitted_bytes: 0,
        }
    }

    /// Atomically queues bytes, splitting them into plan-sized chunks.
    pub(crate) fn enqueue(&mut self, bytes: Vec<u8>) -> Result<usize, VmHttpStaticError> {
        self.require_open()?;
        if bytes.is_empty() {
            return Err(VmHttpStaticError::InvalidStreamChunk);
        }
        let chunk_count = bytes.len().div_ceil(self.plan.chunk_size);
        if self.pending.len().saturating_add(chunk_count) > self.plan.max_pending_writes {
            return Err(VmHttpStaticError::StreamBackpressure);
        }
        for chunk in bytes.chunks(self.plan.chunk_size) {
            self.pending.push_back(chunk.to_vec());
        }
        Ok(chunk_count)
    }

    /// Flushes one queued chunk and advances finishing streams to completion.
    pub(crate) fn flush_next(&mut self) -> Result<Option<Vec<u8>>, VmHttpStaticError> {
        if self.state == VmHttpStreamState::Aborted {
            return Err(VmHttpStaticError::StreamAborted);
        }
        let Some(chunk) = self.commit_next_chunk() else {
            self.complete_if_drained();
            return Ok(None);
        };
        Ok(Some(chunk))
    }

    /// Moves one queued chunk to VM TCP or parks the writer under backpressure.
    pub(crate) fn flush_next_to_tcp(
        &mut self,
        writer: &mut VmInMemoryFrameReader,
        tcp: &mut VmTcpRuntime,
        process: VmProcessId,
    ) -> Result<VmHttpStreamTcpFlush, VmHttpStaticError> {
        if self.state == VmHttpStreamState::Aborted {
            return Err(VmHttpStaticError::StreamAborted);
        }
        let Some(chunk) = self.pending.front().cloned() else {
            self.complete_if_drained();
            return Ok(if self.state == VmHttpStreamState::Complete {
                VmHttpStreamTcpFlush::Complete
            } else {
                VmHttpStreamTcpFlush::Idle
            });
        };

        match write_or_park(writer, tcp, process, chunk) {
            Ok(VmHttpTcpWrite::Written(bytes)) => {
                let emitted = self
                    .commit_next_chunk()
                    .expect("TCP write succeeded for the queued front chunk");
                debug_assert_eq!(bytes, emitted.len());
                Ok(VmHttpStreamTcpFlush::Written {
                    bytes,
                    state: self.state,
                })
            }
            Ok(VmHttpTcpWrite::Parked) => Ok(VmHttpStreamTcpFlush::Parked),
            Err(error) => Err(self.abort_for_transport(error)),
        }
    }

    /// Stops admission and completes after already accepted chunks flush.
    pub(crate) fn finish(&mut self) -> Result<(), VmHttpStaticError> {
        match self.state {
            VmHttpStreamState::Open if self.pending.is_empty() => {
                self.state = VmHttpStreamState::Complete;
                Ok(())
            }
            VmHttpStreamState::Open => {
                self.state = VmHttpStreamState::Finishing;
                Ok(())
            }
            VmHttpStreamState::Finishing | VmHttpStreamState::Complete => Ok(()),
            VmHttpStreamState::Aborted => Err(VmHttpStaticError::StreamAborted),
        }
    }

    /// Aborts an active stream and discards every pending chunk.
    pub(crate) fn abort(&mut self) -> Result<usize, VmHttpStaticError> {
        match self.state {
            VmHttpStreamState::Open | VmHttpStreamState::Finishing => {
                let discarded = self.pending.len();
                self.pending.clear();
                self.state = VmHttpStreamState::Aborted;
                Ok(discarded)
            }
            VmHttpStreamState::Complete => Err(VmHttpStaticError::StreamClosed),
            VmHttpStreamState::Aborted => Err(VmHttpStaticError::StreamAborted),
        }
    }

    /// Returns a deterministic stream snapshot for schedulers and diagnostics.
    pub(crate) fn inspect(&self) -> VmHttpStreamInfo {
        VmHttpStreamInfo {
            state: self.state,
            pending_writes: self.pending.len(),
            max_pending_writes: self.plan.max_pending_writes,
            emitted_chunks: self.emitted_chunks,
            emitted_bytes: self.emitted_bytes,
        }
    }

    /// Borrows the next queued chunk without advancing stream state.
    pub(super) fn front_chunk(&self) -> Option<&[u8]> {
        self.pending.front().map(Vec::as_slice)
    }

    /// Commits and accounts the next queued chunk after a successful write.
    pub(super) fn commit_next_chunk(&mut self) -> Option<Vec<u8>> {
        let chunk = self.pending.pop_front()?;
        self.emitted_chunks = self.emitted_chunks.saturating_add(1);
        self.emitted_bytes = self.emitted_bytes.saturating_add(chunk.len());
        self.complete_if_drained();
        Some(chunk)
    }

    /// Completes a finishing stream once every accepted chunk is drained.
    pub(super) fn complete_if_drained(&mut self) {
        if self.pending.is_empty() && self.state == VmHttpStreamState::Finishing {
            self.state = VmHttpStreamState::Complete;
        }
    }

    /// Forces terminal abort and returns the number of discarded chunks.
    pub(super) fn force_abort(&mut self) -> usize {
        let discarded = self.pending.len();
        self.pending.clear();
        self.state = VmHttpStreamState::Aborted;
        discarded
    }

    /// Aborts and maps a transport failure into the HTTP stream error taxonomy.
    pub(super) fn abort_for_transport(&mut self, error: VmFramingError) -> VmHttpStaticError {
        self.pending.clear();
        self.state = VmHttpStreamState::Aborted;
        match error {
            VmFramingError::Closed | VmFramingError::FramingEof => {
                VmHttpStaticError::StreamTransportClosed
            }
            VmFramingError::Cancelled => VmHttpStaticError::StreamTransportCancelled,
            VmFramingError::FramingOverflow
            | VmFramingError::InvalidFrame
            | VmFramingError::BackpressureExceeded
            | VmFramingError::Timeout => VmHttpStaticError::StreamTransportInvalid,
        }
    }

    /// Requires an admission-open stream before accepting new bytes.
    fn require_open(&self) -> Result<(), VmHttpStaticError> {
        match self.state {
            VmHttpStreamState::Open => Ok(()),
            VmHttpStreamState::Aborted => Err(VmHttpStaticError::StreamAborted),
            VmHttpStreamState::Finishing | VmHttpStreamState::Complete => {
                Err(VmHttpStaticError::StreamClosed)
            }
        }
    }
}

/// Writes one chunk or parks its owner when VM TCP applies backpressure.
pub(super) fn write_or_park(
    writer: &mut VmInMemoryFrameReader,
    tcp: &mut VmTcpRuntime,
    process: VmProcessId,
    bytes: Vec<u8>,
) -> Result<VmHttpTcpWrite, VmFramingError> {
    match writer.write(tcp, bytes) {
        Ok(bytes) => Ok(VmHttpTcpWrite::Written(bytes)),
        Err(VmFramingError::BackpressureExceeded) => {
            match tcp.park_send(writer.stream(), process) {
                Ok(true) => Ok(VmHttpTcpWrite::Parked),
                Ok(false) => Err(VmFramingError::InvalidFrame),
                Err(error) => Err(classify_tcp_error(&error)),
            }
        }
        Err(error) => Err(error),
    }
}
