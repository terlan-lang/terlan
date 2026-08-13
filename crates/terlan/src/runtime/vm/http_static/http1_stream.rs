use super::super::framing::VmInMemoryFrameReader;
use super::super::http::response_wire::{
    write_http1_stream_chunk, write_http1_stream_end, write_http1_stream_head,
};
use super::super::process::VmProcessId;
use super::super::tcp::VmTcpRuntime;
use super::stream::{
    write_or_park, VmHttpResponseStream, VmHttpStreamInfo, VmHttpStreamState, VmHttpTcpWrite,
};
use super::{VmHttpStaticError, VmHttpStreamPlan};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// HTTP/1 stream lifecycle including head and terminal framing phases.
#[cfg(test)]
pub(crate) enum VmHttp1StreamState {
    Starting,
    Open,
    Finishing,
    Finalizing,
    Complete,
    Aborted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// HTTP/1 wire component selected for the next transport write.
#[cfg(test)]
pub(crate) enum VmHttp1StreamPart {
    Head,
    Chunk,
    End,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Scheduler-visible outcome of one HTTP/1 stream flush attempt.
#[cfg(test)]
pub(crate) enum VmHttp1StreamTcpFlush {
    Idle,
    Parked {
        part: VmHttp1StreamPart,
    },
    Written {
        part: VmHttp1StreamPart,
        bytes: usize,
        state: VmHttp1StreamState,
    },
    Complete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Inspectable HTTP/1 framing and body-stream progress.
#[cfg(test)]
pub(crate) struct VmHttp1StreamInfo {
    pub(crate) state: VmHttp1StreamState,
    pub(crate) body: VmHttpStreamInfo,
    pub(crate) wire_bytes: usize,
}

#[derive(Debug)]
/// Bounded response stream that serializes HTTP/1 framing onto VM TCP.
pub(crate) struct VmHttp1ResponseStream {
    body: VmHttpResponseStream,
    head: Option<Vec<u8>>,
    chunk_wire: Option<Vec<u8>>,
    end: Vec<u8>,
    end_sent: bool,
    wire_bytes: usize,
}

#[cfg(test)]
impl VmHttp1ResponseStream {
    /// Creates a stream with validated response head and terminal framing.
    pub(super) fn new(
        plan: VmHttpStreamPlan,
        response: ::http::Response<()>,
        close_connection: bool,
    ) -> Result<Self, VmHttpStaticError> {
        let mut head = Vec::new();
        write_http1_stream_head(&mut head, &response, close_connection)
            .map_err(|_| VmHttpStaticError::InvalidStreamResponse)?;
        let mut end = Vec::new();
        write_http1_stream_end(&mut end).map_err(|_| VmHttpStaticError::InvalidStreamResponse)?;
        Ok(Self {
            body: VmHttpResponseStream::new(plan),
            head: Some(head),
            chunk_wire: None,
            end,
            end_sent: false,
            wire_bytes: 0,
        })
    }

    /// Queues body bytes using the stream plan's bounded chunking policy.
    pub(crate) fn enqueue(&mut self, bytes: Vec<u8>) -> Result<usize, VmHttpStaticError> {
        self.body.enqueue(bytes)
    }

    /// Stops admission and schedules terminal framing after queued chunks.
    pub(crate) fn finish(&mut self) -> Result<(), VmHttpStaticError> {
        self.body.finish()
    }

    /// Aborts active framing and discards queued body chunks.
    pub(crate) fn abort(&mut self) -> Result<usize, VmHttpStaticError> {
        match self.state() {
            VmHttp1StreamState::Complete => Err(VmHttpStaticError::StreamClosed),
            VmHttp1StreamState::Aborted => Err(VmHttpStaticError::StreamAborted),
            VmHttp1StreamState::Starting
            | VmHttp1StreamState::Open
            | VmHttp1StreamState::Finishing
            | VmHttp1StreamState::Finalizing => {
                self.head = None;
                self.chunk_wire = None;
                Ok(self.body.force_abort())
            }
        }
    }

    /// Flushes the next head, chunk, or terminal part through VM TCP.
    pub(crate) fn flush_next_to_tcp(
        &mut self,
        writer: &mut VmInMemoryFrameReader,
        tcp: &mut VmTcpRuntime,
        process: VmProcessId,
    ) -> Result<VmHttp1StreamTcpFlush, VmHttpStaticError> {
        if self.state() == VmHttp1StreamState::Aborted {
            return Err(VmHttpStaticError::StreamAborted);
        }
        if let Some(head) = self.head.clone() {
            return self.flush_part(VmHttp1StreamPart::Head, head, writer, tcp, process);
        }
        if let Some(chunk) = self.body.front_chunk() {
            if self.chunk_wire.is_none() {
                let mut wire = Vec::new();
                write_http1_stream_chunk(&mut wire, chunk)
                    .map_err(|_| VmHttpStaticError::InvalidStreamResponse)?;
                self.chunk_wire = Some(wire);
            }
            return self.flush_part(
                VmHttp1StreamPart::Chunk,
                self.chunk_wire
                    .clone()
                    .expect("front chunk has cached HTTP wire bytes"),
                writer,
                tcp,
                process,
            );
        }
        self.body.complete_if_drained();
        if self.body.inspect().state == VmHttpStreamState::Complete && !self.end_sent {
            return self.flush_part(
                VmHttp1StreamPart::End,
                self.end.clone(),
                writer,
                tcp,
                process,
            );
        }
        Ok(if self.state() == VmHttp1StreamState::Complete {
            VmHttp1StreamTcpFlush::Complete
        } else {
            VmHttp1StreamTcpFlush::Idle
        })
    }

    /// Returns a deterministic stream snapshot for scheduling and diagnostics.
    pub(crate) fn inspect(&self) -> VmHttp1StreamInfo {
        VmHttp1StreamInfo {
            state: self.state(),
            body: self.body.inspect(),
            wire_bytes: self.wire_bytes,
        }
    }

    /// Attempts one framed transport write and commits it only after success.
    fn flush_part(
        &mut self,
        part: VmHttp1StreamPart,
        wire: Vec<u8>,
        writer: &mut VmInMemoryFrameReader,
        tcp: &mut VmTcpRuntime,
        process: VmProcessId,
    ) -> Result<VmHttp1StreamTcpFlush, VmHttpStaticError> {
        match write_or_park(writer, tcp, process, wire) {
            Ok(VmHttpTcpWrite::Parked) => Ok(VmHttp1StreamTcpFlush::Parked { part }),
            Ok(VmHttpTcpWrite::Written(bytes)) => {
                self.commit_part(part);
                self.wire_bytes = self.wire_bytes.saturating_add(bytes);
                Ok(VmHttp1StreamTcpFlush::Written {
                    part,
                    bytes,
                    state: self.state(),
                })
            }
            Err(error) => Err(self.body.abort_for_transport(error)),
        }
    }

    /// Advances local framing state after one complete TCP write.
    fn commit_part(&mut self, part: VmHttp1StreamPart) {
        match part {
            VmHttp1StreamPart::Head => self.head = None,
            VmHttp1StreamPart::Chunk => {
                self.body
                    .commit_next_chunk()
                    .expect("HTTP chunk write commits one queued body chunk");
                self.chunk_wire = None;
            }
            VmHttp1StreamPart::End => self.end_sent = true,
        }
    }

    /// Derives the combined HTTP/1 framing and body-stream state.
    fn state(&self) -> VmHttp1StreamState {
        let body_state = self.body.inspect().state;
        if body_state == VmHttpStreamState::Aborted {
            return VmHttp1StreamState::Aborted;
        }
        if self.end_sent {
            return VmHttp1StreamState::Complete;
        }
        if self.head.is_some() {
            return VmHttp1StreamState::Starting;
        }
        match body_state {
            VmHttpStreamState::Open => VmHttp1StreamState::Open,
            VmHttpStreamState::Finishing => VmHttp1StreamState::Finishing,
            VmHttpStreamState::Complete => VmHttp1StreamState::Finalizing,
            VmHttpStreamState::Aborted => VmHttp1StreamState::Aborted,
        }
    }
}
