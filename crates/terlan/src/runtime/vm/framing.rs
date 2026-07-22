#![allow(dead_code)]

//! Deterministic byte framing over VM-owned TCP streams.

use super::packet::{
    decode_packet, VmDecodedPacket, VmPacketDecodeOutcome, VmPacketMode, VmPacketOptions,
};
use super::tcp::{VmTcpRuntime, VmTcpStream};

#[cfg(test)]
#[path = "framing_test.rs"]
mod framing_test;

#[cfg(test)]
#[path = "framing_packet_decode_test.rs"]
mod framing_packet_decode_test;

/// Typed stream framing failure.
///
/// Inputs: VM TCP lifecycle, buffer, and frame state.
/// Output: stable framing error classification.
/// Transformation: keeps protocol layers away from stringly socket failures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmFramingError {
    FramingEof,
    FramingOverflow,
    InvalidFrame,
    BackpressureExceeded,
    Closed,
    Timeout,
    Cancelled,
}

/// Stateful frame reader for one VM-owned byte stream.
///
/// Inputs:
/// - A stream handle and a bounded staging capacity.
///
/// Output:
/// - Deterministic exact, delimiter, and length-prefixed frames.
///
/// Transformation:
/// - Drains VM TCP byte chunks into a private bounded buffer so partial frames
///   survive scheduler polls without hidden host async state.
#[derive(Debug)]
pub(crate) struct VmInMemoryFrameReader {
    stream: VmTcpStream,
    buffer: Vec<u8>,
    buffer_limit: usize,
}

impl VmInMemoryFrameReader {
    /// Creates a bounded frame reader for one VM TCP stream.
    pub(crate) fn new(stream: VmTcpStream, buffer_limit: usize) -> Result<Self, VmFramingError> {
        if buffer_limit == 0 {
            return Err(VmFramingError::InvalidFrame);
        }
        Ok(Self {
            stream,
            buffer: Vec::new(),
            buffer_limit,
        })
    }

    /// Returns the stream this reader consumes.
    pub(crate) fn stream(&self) -> VmTcpStream {
        self.stream
    }

    /// Returns staged bytes that were read but not yet framed.
    pub(crate) fn buffered_len(&self) -> usize {
        self.buffer.len()
    }

    /// Reads up to `max_bytes` from the underlying stream without framing.
    pub(crate) fn read(
        &mut self,
        tcp: &mut VmTcpRuntime,
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, VmFramingError> {
        if max_bytes == 0 {
            return Err(VmFramingError::InvalidFrame);
        }
        if !self.buffer.is_empty() {
            return Ok(Some(self.take(max_bytes.min(self.buffer.len()))));
        }
        map_tcp_receive(tcp.receive(self.stream, max_bytes))
    }

    /// Writes one byte chunk to the stream peer.
    pub(crate) fn write(
        &mut self,
        tcp: &mut VmTcpRuntime,
        bytes: Vec<u8>,
    ) -> Result<usize, VmFramingError> {
        map_tcp_send(tcp.send(self.stream, bytes))
    }

    /// Closes the stream handle.
    pub(crate) fn close(&mut self, tcp: &mut VmTcpRuntime) -> Result<(), VmFramingError> {
        map_tcp_unit(tcp.close_stream(self.stream))
    }

    /// Reads an exact byte count when enough bytes are available.
    pub(crate) fn read_exact(
        &mut self,
        tcp: &mut VmTcpRuntime,
        len: usize,
    ) -> Result<Option<Vec<u8>>, VmFramingError> {
        if len > self.buffer_limit {
            return Err(VmFramingError::FramingOverflow);
        }
        self.fill_until(tcp, len)?;
        if self.buffer.len() < len {
            return self.pending_or_eof(tcp);
        }
        Ok(Some(self.take(len)))
    }

    /// Reads an exact byte count or reports timeout for a pending frame.
    pub(crate) fn read_exact_with_timeout(
        &mut self,
        tcp: &mut VmTcpRuntime,
        len: usize,
        elapsed: bool,
    ) -> Result<Option<Vec<u8>>, VmFramingError> {
        match self.read_exact(tcp, len)? {
            Some(bytes) => Ok(Some(bytes)),
            None if elapsed => Err(VmFramingError::Timeout),
            None => Ok(None),
        }
    }

    /// Reads until `delimiter`, returning bytes before the delimiter.
    pub(crate) fn read_until(
        &mut self,
        tcp: &mut VmTcpRuntime,
        delimiter: u8,
    ) -> Result<Option<Vec<u8>>, VmFramingError> {
        loop {
            if let Some(position) = self.buffer.iter().position(|byte| *byte == delimiter) {
                let frame = self.buffer.drain(..position).collect::<Vec<_>>();
                self.buffer.drain(..1);
                return Ok(Some(frame));
            }
            if self.buffer.len() >= self.buffer_limit {
                return Err(VmFramingError::FramingOverflow);
            }
            match map_tcp_receive(tcp.receive(self.stream, self.remaining_capacity()))? {
                Some(bytes) => self.push_bytes(bytes)?,
                None => return self.pending_or_eof(tcp),
            }
        }
    }

    /// Reads one big-endian u32 length-prefixed frame.
    pub(crate) fn read_length_prefixed(
        &mut self,
        tcp: &mut VmTcpRuntime,
    ) -> Result<Option<Vec<u8>>, VmFramingError> {
        self.fill_until(tcp, 4)?;
        if self.buffer.len() < 4 {
            return self.pending_or_eof(tcp);
        }
        let len = u32::from_be_bytes([
            self.buffer[0],
            self.buffer[1],
            self.buffer[2],
            self.buffer[3],
        ]) as usize;
        if len > self.buffer_limit.saturating_sub(4) {
            return Err(VmFramingError::FramingOverflow);
        }
        self.fill_until(tcp, 4 + len)?;
        if self.buffer.len() < 4 + len {
            return self.pending_or_eof(tcp);
        }
        self.buffer.drain(..4);
        Ok(Some(self.take(len)))
    }

    /// Writes one big-endian u32 length-prefixed frame.
    pub(crate) fn write_length_prefixed(
        &mut self,
        tcp: &mut VmTcpRuntime,
        payload: Vec<u8>,
    ) -> Result<usize, VmFramingError> {
        if payload.len() > self.buffer_limit.saturating_sub(4) {
            return Err(VmFramingError::FramingOverflow);
        }
        let len = u32::try_from(payload.len()).map_err(|_| VmFramingError::FramingOverflow)?;
        let mut framed = len.to_be_bytes().to_vec();
        framed.extend(payload);
        self.write(tcp, framed)
    }

    /// Reads one typed packet while retaining incomplete bytes across polls.
    pub(crate) fn read_packet(
        &mut self,
        tcp: &mut VmTcpRuntime,
        mode: VmPacketMode,
        options: VmPacketOptions,
    ) -> Result<Option<VmDecodedPacket>, VmFramingError> {
        loop {
            match decode_packet(mode, &self.buffer, options) {
                VmPacketDecodeOutcome::Complete { packet, consumed } => {
                    self.buffer.drain(..consumed);
                    return Ok(Some(packet));
                }
                VmPacketDecodeOutcome::Invalid => return Err(VmFramingError::InvalidFrame),
                VmPacketDecodeOutcome::More { total } => {
                    if total.is_some_and(|total| total > self.buffer_limit) {
                        return Err(VmFramingError::FramingOverflow);
                    }
                }
            }
            if self.buffer.len() >= self.buffer_limit {
                return Err(VmFramingError::FramingOverflow);
            }
            match map_tcp_receive(tcp.receive(self.stream, self.remaining_capacity()))? {
                Some(bytes) => self.push_bytes(bytes)?,
                None => return self.pending_or_eof(tcp),
            }
        }
    }

    fn fill_until(
        &mut self,
        tcp: &mut VmTcpRuntime,
        target_len: usize,
    ) -> Result<(), VmFramingError> {
        while self.buffer.len() < target_len {
            match map_tcp_receive(tcp.receive(self.stream, self.remaining_capacity()))? {
                Some(bytes) => self.push_bytes(bytes)?,
                None => break,
            }
        }
        Ok(())
    }

    fn pending_or_eof<T>(&self, tcp: &VmTcpRuntime) -> Result<Option<T>, VmFramingError> {
        let info = tcp.inspect_stream(self.stream).map_err(map_tcp_error)?;
        if info.closed {
            return Err(VmFramingError::Closed);
        }
        if info.cancelled {
            return Err(VmFramingError::Cancelled);
        }
        if tcp.peer_write_closed(self.stream).map_err(map_tcp_error)? {
            return Err(VmFramingError::FramingEof);
        }
        Ok(None)
    }

    fn push_bytes(&mut self, bytes: Vec<u8>) -> Result<(), VmFramingError> {
        if self.buffer.len().saturating_add(bytes.len()) > self.buffer_limit {
            return Err(VmFramingError::FramingOverflow);
        }
        self.buffer.extend(bytes);
        Ok(())
    }

    fn remaining_capacity(&self) -> usize {
        self.buffer_limit.saturating_sub(self.buffer.len()).max(1)
    }

    fn take(&mut self, len: usize) -> Vec<u8> {
        self.buffer.drain(..len).collect()
    }
}

fn map_tcp_receive(
    result: Result<Option<Vec<u8>>, String>,
) -> Result<Option<Vec<u8>>, VmFramingError> {
    result.map_err(map_tcp_error)
}

fn map_tcp_send(result: Result<usize, String>) -> Result<usize, VmFramingError> {
    result.map_err(map_tcp_error)
}

fn map_tcp_unit(result: Result<(), String>) -> Result<(), VmFramingError> {
    result.map_err(map_tcp_error)
}

fn map_tcp_error(error: String) -> VmFramingError {
    classify_tcp_error(&error)
}

/// Classifies stable VM TCP diagnostics for protocol-layer adapters.
pub(crate) fn classify_tcp_error(error: &str) -> VmFramingError {
    match error {
        "VM TCP stream is cancelled" | "VM TCP peer stream is cancelled" => {
            VmFramingError::Cancelled
        }
        "VM TCP stream is closed" | "VM TCP peer stream is closed" => VmFramingError::Closed,
        "VM TCP peer inbox is full" => VmFramingError::BackpressureExceeded,
        "VM TCP receive max_bytes must be greater than 0"
        | "VM TCP stream inbox limit must be greater than 0"
        | "VM TCP stream has no connected peer"
        | "VM TCP listener handle is unknown"
        | "VM TCP stream handle is unknown" => VmFramingError::InvalidFrame,
        _ => VmFramingError::InvalidFrame,
    }
}
