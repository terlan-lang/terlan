#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};

use super::process::VmProcessId;

#[cfg(test)]
#[path = "tcp_async_ports_beam_suite_parity_test.rs"]
mod tcp_async_ports_beam_suite_parity_test;
#[cfg(test)]
#[path = "tcp_busy_port_beam_suite_parity_test.rs"]
mod tcp_busy_port_beam_suite_parity_test;

#[cfg(test)]
#[path = "tcp_test.rs"]
mod tcp_test;

/// VM-owned TCP listener handle.
///
/// Inputs: opaque runtime id. Output: stable listener handle. Transformation:
/// keeps application code away from host socket descriptors.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct VmTcpListener {
    id: u64,
}

/// VM-owned TCP stream handle.
///
/// Inputs: opaque runtime id. Output: stable stream handle. Transformation:
/// represents accepted or outbound streams without exposing host file
/// descriptors.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct VmTcpStream {
    id: u64,
}

impl VmTcpStream {
    /// Returns the opaque numeric identity used by runtime diagnostics.
    pub(crate) fn as_u64(self) -> u64 {
        self.id
    }
}

#[cfg(test)]
impl VmTcpStream {
    /// Builds an opaque stream handle for adversarial runtime tests.
    pub(crate) fn test_handle(id: u64) -> Self {
        Self { id }
    }
}

/// Runtime-visible stream state snapshot.
///
/// Inputs: one stream handle. Output: inspectable state. Transformation:
/// exposes ownership and pressure data without exposing mutable stream internals.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmTcpStreamInfo {
    pub(crate) owner: Option<String>,
    pub(crate) queued_messages: usize,
    pub(crate) queued_bytes: usize,
    pub(crate) inbox_limit: usize,
    pub(crate) waiting_readers: usize,
    pub(crate) waiting_writers: usize,
    pub(crate) write_closed: bool,
    pub(crate) closed: bool,
    pub(crate) cancelled: bool,
}

/// Runtime-visible listener state snapshot.
///
/// Inputs: one listener handle. Output: inspectable listener state.
/// Transformation: exposes accept-side backpressure and lifecycle state
/// without exposing mutable listener internals or host socket descriptors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmTcpListenerInfo {
    pub(crate) address: String,
    pub(crate) backlog_limit: usize,
    pub(crate) queued_accepts: usize,
    pub(crate) waiting_acceptors: usize,
    pub(crate) closed: bool,
}

/// Runtime-visible TCP readiness wake intent.
///
/// Inputs: a VM process id and the resource that became ready. Output: stable
/// wake intent for the VM scheduler. Transformation: keeps TCP readiness
/// producer logic independent from scheduler queue internals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmTcpWake {
    Accept {
        process: VmProcessId,
        listener: VmTcpListener,
    },
    Read {
        process: VmProcessId,
        stream: VmTcpStream,
    },
    Write {
        process: VmProcessId,
        stream: VmTcpStream,
    },
}

/// VM-owned TCP stream registry.
///
/// Inputs:
/// - Listener, connection, byte, close, cancel, and owner-cleanup operations.
///
/// Output:
/// - Deterministic VM stream handles and stable diagnostics.
///
/// Transformation:
/// - Models listener/accept/send/receive ownership in the VM without relying on
///   host async state or direct OS socket handles.
#[derive(Debug, Default)]
pub(crate) struct VmTcpRuntime {
    next_listener: u64,
    next_stream: u64,
    listeners: HashMap<u64, ListenerState>,
    streams: HashMap<u64, StreamState>,
}

/// Aggregate listener and stream ownership retained by the VM TCP runtime.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmTcpRuntimeMetrics {
    pub(crate) listeners: usize,
    pub(crate) open_listeners: usize,
    pub(crate) streams: usize,
    pub(crate) open_streams: usize,
    pub(crate) queued_accepts: usize,
    pub(crate) queued_messages: usize,
    pub(crate) queued_bytes: usize,
    pub(crate) waiting_readers: usize,
    pub(crate) waiting_writers: usize,
}

#[derive(Debug)]
struct ListenerState {
    address: String,
    backlog_limit: usize,
    backlog: VecDeque<VmTcpStream>,
    accept_waiters: VecDeque<VmProcessId>,
    closed: bool,
}

#[derive(Debug)]
struct StreamState {
    peer: Option<VmTcpStream>,
    owner: Option<String>,
    inbox: VecDeque<Vec<u8>>,
    inbox_limit: usize,
    read_waiters: VecDeque<VmProcessId>,
    write_waiters: VecDeque<(VmProcessId, VmTcpStream)>,
    write_closed: bool,
    closed: bool,
    cancelled: bool,
}

impl VmTcpRuntime {
    /// Creates an empty TCP runtime registry.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Returns deterministic aggregate ownership for leak and soak checks.
    pub(crate) fn metrics(&self) -> VmTcpRuntimeMetrics {
        let mut metrics = VmTcpRuntimeMetrics {
            listeners: self.listeners.len(),
            streams: self.streams.len(),
            ..VmTcpRuntimeMetrics::default()
        };
        for listener in self.listeners.values() {
            metrics.open_listeners += usize::from(!listener.closed);
            metrics.queued_accepts += listener.backlog.len();
            metrics.waiting_readers += listener.accept_waiters.len();
        }
        for stream in self.streams.values() {
            metrics.open_streams += usize::from(!stream.closed && !stream.cancelled);
            metrics.queued_messages += stream.inbox.len();
            metrics.queued_bytes = metrics
                .queued_bytes
                .saturating_add(stream.inbox.iter().map(Vec::len).sum::<usize>());
            metrics.waiting_readers += stream.read_waiters.len();
            metrics.waiting_writers += stream.write_waiters.len();
        }
        metrics
    }

    /// Opens a VM-owned listener at a logical address.
    pub(crate) fn listen(&mut self, address: impl Into<String>) -> Result<VmTcpListener, String> {
        self.listen_with_backlog(address, usize::MAX)
    }

    /// Opens a VM-owned listener with a bounded backlog.
    pub(crate) fn listen_with_backlog(
        &mut self,
        address: impl Into<String>,
        backlog_limit: usize,
    ) -> Result<VmTcpListener, String> {
        let address = address.into();
        if address.trim().is_empty() {
            return Err("VM TCP listener address cannot be empty".to_string());
        }
        if backlog_limit == 0 {
            return Err("VM TCP listener backlog limit must be greater than 0".to_string());
        }
        if self
            .listeners
            .values()
            .any(|listener| !listener.closed && listener.address == address)
        {
            return Err(format!("VM TCP listener `{address}` already exists"));
        }
        self.next_listener = self.next_listener.saturating_add(1);
        let listener = VmTcpListener {
            id: self.next_listener,
        };
        self.listeners.insert(
            listener.id,
            ListenerState {
                address,
                backlog_limit,
                backlog: VecDeque::new(),
                accept_waiters: VecDeque::new(),
                closed: false,
            },
        );
        Ok(listener)
    }

    /// Connects to a VM-owned listener and enqueues the accepted peer stream.
    pub(crate) fn connect(
        &mut self,
        address: &str,
        owner: impl Into<String>,
    ) -> Result<VmTcpStream, String> {
        Ok(self.connect_with_wakeups(address, owner)?.0)
    }

    /// Connects to a VM-owned listener and returns accept wake intents.
    pub(crate) fn connect_with_wakeups(
        &mut self,
        address: &str,
        owner: impl Into<String>,
    ) -> Result<(VmTcpStream, Vec<VmTcpWake>), String> {
        let owner = owner.into();
        let listener_id = self
            .listeners
            .iter()
            .find_map(|(id, listener)| {
                (!listener.closed && listener.address == address).then_some(*id)
            })
            .ok_or_else(|| format!("VM TCP listener `{address}` was not found"))?;
        let listener = self
            .listeners
            .get(&listener_id)
            .expect("listener id was selected from listeners");
        if listener.backlog.len() >= listener.backlog_limit {
            return Err(format!("VM TCP listener `{address}` backlog is full"));
        }

        let client = self.allocate_stream(Some(owner));
        let server = self.allocate_stream(None);
        self.stream_mut(client)?.peer = Some(server);
        self.stream_mut(server)?.peer = Some(client);
        let listener = self
            .listeners
            .get_mut(&listener_id)
            .expect("listener id was selected from listeners");
        listener.backlog.push_back(server);
        let waiters = listener.accept_waiters.drain(..).collect::<Vec<_>>();
        let listener = VmTcpListener { id: listener_id };
        let wakeups = waiters
            .into_iter()
            .map(|process| VmTcpWake::Accept { process, listener })
            .collect();
        Ok((client, wakeups))
    }

    /// Accepts the oldest queued stream for a listener.
    pub(crate) fn accept(
        &mut self,
        listener: VmTcpListener,
        owner: impl Into<String>,
    ) -> Result<Option<VmTcpStream>, String> {
        let accepted = {
            let listener = self.listener_mut(listener)?;
            if listener.closed {
                return Err("VM TCP listener is closed".to_string());
            }
            listener.backlog.pop_front()
        };
        if let Some(stream) = accepted {
            self.stream_mut(stream)?.owner = Some(owner.into());
            Ok(Some(stream))
        } else {
            Ok(None)
        }
    }

    /// Parks a process until a listener has an accepted stream.
    pub(crate) fn park_accept(
        &mut self,
        listener: VmTcpListener,
        process: VmProcessId,
    ) -> Result<bool, String> {
        let listener = self.listener_mut(listener)?;
        if listener.closed {
            return Err("VM TCP listener is closed".to_string());
        }
        if !listener.backlog.is_empty() {
            return Ok(false);
        }
        if !listener.accept_waiters.contains(&process) {
            listener.accept_waiters.push_back(process);
        }
        Ok(true)
    }

    /// Sends one byte chunk to a stream's peer inbox.
    pub(crate) fn send(&mut self, stream: VmTcpStream, bytes: Vec<u8>) -> Result<usize, String> {
        Ok(self.send_with_wakeups(stream, bytes)?.0)
    }

    /// Sends one byte chunk and returns read wake intents for the peer stream.
    pub(crate) fn send_with_wakeups(
        &mut self,
        stream: VmTcpStream,
        bytes: Vec<u8>,
    ) -> Result<(usize, Vec<VmTcpWake>), String> {
        let peer = {
            let state = self.stream(stream)?;
            if state.cancelled {
                return Err("VM TCP stream is cancelled".to_string());
            }
            if state.closed {
                return Err("VM TCP stream is closed".to_string());
            }
            if state.write_closed {
                return Err("VM TCP stream write side is closed".to_string());
            }
            state
                .peer
                .ok_or_else(|| "VM TCP stream has no connected peer".to_string())?
        };
        let peer_state = self.stream_mut(peer)?;
        if peer_state.cancelled {
            return Err("VM TCP peer stream is cancelled".to_string());
        }
        if peer_state.closed {
            return Err("VM TCP peer stream is closed".to_string());
        }
        let len = bytes.len();
        let queued_bytes = peer_state.inbox.iter().map(Vec::len).sum::<usize>();
        if queued_bytes.saturating_add(len) > peer_state.inbox_limit {
            return Err("VM TCP peer inbox is full".to_string());
        }
        peer_state.inbox.push_back(bytes);
        let waiters = peer_state.read_waiters.drain(..).collect::<Vec<_>>();
        let wakeups = waiters
            .into_iter()
            .map(|process| VmTcpWake::Read {
                process,
                stream: peer,
            })
            .collect();
        Ok((len, wakeups))
    }

    /// Sets the maximum queued unread bytes for a stream.
    pub(crate) fn set_stream_inbox_limit(
        &mut self,
        stream: VmTcpStream,
        limit: usize,
    ) -> Result<(), String> {
        if limit == 0 {
            return Err("VM TCP stream inbox limit must be greater than 0".to_string());
        }
        self.stream_mut(stream)?.inbox_limit = limit;
        Ok(())
    }

    /// Receives one byte chunk, splitting it when a timeout-sized read limit is
    /// smaller than the queued payload.
    pub(crate) fn receive(
        &mut self,
        stream: VmTcpStream,
        max_bytes: usize,
    ) -> Result<Option<Vec<u8>>, String> {
        Ok(self.receive_with_wakeups(stream, max_bytes)?.0)
    }

    /// Receives one byte chunk and returns write wake intents after capacity
    /// becomes available.
    pub(crate) fn receive_with_wakeups(
        &mut self,
        stream: VmTcpStream,
        max_bytes: usize,
    ) -> Result<(Option<Vec<u8>>, Vec<VmTcpWake>), String> {
        if max_bytes == 0 {
            return Err("VM TCP receive max_bytes must be greater than 0".to_string());
        }
        let state = self.stream_mut(stream)?;
        if state.cancelled {
            return Err("VM TCP stream is cancelled".to_string());
        }
        let Some(mut bytes) = state.inbox.pop_front() else {
            return Ok((None, Vec::new()));
        };
        if bytes.len() > max_bytes {
            let rest = bytes.split_off(max_bytes);
            state.inbox.push_front(rest);
        }
        let waiters = if state.inbox.iter().map(Vec::len).sum::<usize>() < state.inbox_limit {
            state.write_waiters.drain(..).collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let wakeups = waiters
            .into_iter()
            .map(|(process, stream)| VmTcpWake::Write { process, stream })
            .collect();
        Ok((Some(bytes), wakeups))
    }

    /// Parks a process until a stream has readable bytes.
    pub(crate) fn park_receive(
        &mut self,
        stream: VmTcpStream,
        process: VmProcessId,
    ) -> Result<bool, String> {
        let state = self.stream_mut(stream)?;
        if state.cancelled {
            return Err("VM TCP stream is cancelled".to_string());
        }
        if state.closed {
            return Err("VM TCP stream is closed".to_string());
        }
        if !state.inbox.is_empty() {
            return Ok(false);
        }
        if !state.read_waiters.contains(&process) {
            state.read_waiters.push_back(process);
        }
        Ok(true)
    }

    /// Parks a process until this stream can send to its peer.
    pub(crate) fn park_send(
        &mut self,
        stream: VmTcpStream,
        process: VmProcessId,
    ) -> Result<bool, String> {
        let peer = {
            let state = self.stream(stream)?;
            if state.cancelled {
                return Err("VM TCP stream is cancelled".to_string());
            }
            if state.closed {
                return Err("VM TCP stream is closed".to_string());
            }
            if state.write_closed {
                return Err("VM TCP stream write side is closed".to_string());
            }
            state
                .peer
                .ok_or_else(|| "VM TCP stream has no connected peer".to_string())?
        };
        let peer_state = self.stream_mut(peer)?;
        if peer_state.cancelled {
            return Err("VM TCP peer stream is cancelled".to_string());
        }
        if peer_state.closed {
            return Err("VM TCP peer stream is closed".to_string());
        }
        if peer_state.inbox.iter().map(Vec::len).sum::<usize>() < peer_state.inbox_limit {
            return Ok(false);
        }
        let waiter = (process, stream);
        if !peer_state.write_waiters.contains(&waiter) {
            peer_state.write_waiters.push_back(waiter);
        }
        Ok(true)
    }

    /// Closes a stream handle.
    pub(crate) fn close_stream(&mut self, stream: VmTcpStream) -> Result<(), String> {
        self.terminate_stream(stream, false)
    }

    /// Closes only the write side of a stream handle.
    pub(crate) fn close_write(&mut self, stream: VmTcpStream) -> Result<(), String> {
        let state = self.stream_mut(stream)?;
        if state.cancelled {
            return Err("VM TCP stream is cancelled".to_string());
        }
        if state.closed {
            return Err("VM TCP stream is closed".to_string());
        }
        state.write_closed = true;
        Ok(())
    }

    /// Returns whether the connected peer has closed its write side.
    pub(crate) fn peer_write_closed(&self, stream: VmTcpStream) -> Result<bool, String> {
        let peer = self
            .stream(stream)?
            .peer
            .ok_or_else(|| "VM TCP stream has no connected peer".to_string())?;
        Ok(self.stream(peer)?.write_closed)
    }

    /// Closes a listener handle.
    pub(crate) fn close_listener(&mut self, listener: VmTcpListener) -> Result<(), String> {
        let pending = {
            let state = self.listener_mut(listener)?;
            state.closed = true;
            state.accept_waiters.clear();
            state.backlog.drain(..).collect::<Vec<_>>()
        };
        for stream in pending {
            self.terminate_stream(stream, false)?;
        }
        Ok(())
    }

    /// Cancels a stream and drops queued unread bytes.
    pub(crate) fn cancel_stream(&mut self, stream: VmTcpStream) -> Result<(), String> {
        self.terminate_stream(stream, true)
    }

    /// Closes every stream owned by one VM actor or runtime component.
    pub(crate) fn close_owner_streams(&mut self, owner: &str) -> usize {
        let streams = self
            .streams
            .iter()
            .filter_map(|(id, state)| {
                (state.owner.as_deref() == Some(owner) && !state.closed && !state.cancelled)
                    .then_some(VmTcpStream { id: *id })
            })
            .collect::<Vec<_>>();
        for stream in &streams {
            self.terminate_stream(*stream, false)
                .expect("owner stream id was collected from the stream table");
        }
        streams.len()
    }

    /// Returns an inspectable stream state snapshot.
    pub(crate) fn inspect_stream(&self, stream: VmTcpStream) -> Result<VmTcpStreamInfo, String> {
        let state = self.stream(stream)?;
        Ok(VmTcpStreamInfo {
            owner: state.owner.clone(),
            queued_messages: state.inbox.len(),
            queued_bytes: state.inbox.iter().map(Vec::len).sum(),
            inbox_limit: state.inbox_limit,
            waiting_readers: state.read_waiters.len(),
            waiting_writers: state.write_waiters.len(),
            write_closed: state.write_closed,
            closed: state.closed,
            cancelled: state.cancelled,
        })
    }

    /// Returns an inspectable listener state snapshot.
    pub(crate) fn inspect_listener(
        &self,
        listener: VmTcpListener,
    ) -> Result<VmTcpListenerInfo, String> {
        let state = self.listener(listener)?;
        Ok(VmTcpListenerInfo {
            address: state.address.clone(),
            backlog_limit: state.backlog_limit,
            queued_accepts: state.backlog.len(),
            waiting_acceptors: state.accept_waiters.len(),
            closed: state.closed,
        })
    }

    fn allocate_stream(&mut self, owner: Option<String>) -> VmTcpStream {
        self.next_stream = self.next_stream.saturating_add(1);
        let stream = VmTcpStream {
            id: self.next_stream,
        };
        self.streams.insert(
            stream.id,
            StreamState {
                peer: None,
                owner,
                inbox: VecDeque::new(),
                inbox_limit: usize::MAX,
                read_waiters: VecDeque::new(),
                write_waiters: VecDeque::new(),
                write_closed: false,
                closed: false,
                cancelled: false,
            },
        );
        stream
    }

    /// Terminates one stream and releases all readiness and buffered state.
    fn terminate_stream(&mut self, stream: VmTcpStream, cancelled: bool) -> Result<(), String> {
        let state = self.stream_mut(stream)?;
        state.closed = !cancelled;
        state.cancelled = cancelled;
        state.inbox.clear();
        state.read_waiters.clear();
        state.write_waiters.clear();
        for candidate in self.streams.values_mut() {
            candidate
                .write_waiters
                .retain(|(_, waiting_stream)| *waiting_stream != stream);
        }
        Ok(())
    }

    fn listener_mut(&mut self, listener: VmTcpListener) -> Result<&mut ListenerState, String> {
        self.listeners
            .get_mut(&listener.id)
            .ok_or_else(|| "VM TCP listener handle is unknown".to_string())
    }

    fn listener(&self, listener: VmTcpListener) -> Result<&ListenerState, String> {
        self.listeners
            .get(&listener.id)
            .ok_or_else(|| "VM TCP listener handle is unknown".to_string())
    }

    fn stream(&self, stream: VmTcpStream) -> Result<&StreamState, String> {
        self.streams
            .get(&stream.id)
            .ok_or_else(|| "VM TCP stream handle is unknown".to_string())
    }

    fn stream_mut(&mut self, stream: VmTcpStream) -> Result<&mut StreamState, String> {
        self.streams
            .get_mut(&stream.id)
            .ok_or_else(|| "VM TCP stream handle is unknown".to_string())
    }
}
