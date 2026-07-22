#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};

use super::process::VmProcessId;

#[cfg(test)]
#[path = "udp_test.rs"]
mod udp_test;

/// VM-owned UDP socket handle.
///
/// Inputs: opaque runtime id. Output: stable datagram socket handle.
/// Transformation: keeps Terlan actors away from host UDP descriptors.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) struct VmUdpSocket {
    id: u64,
}

/// Runtime-visible UDP socket state snapshot.
///
/// Inputs: one UDP socket handle. Output: inspectable queue and lifecycle
/// state. Transformation: exposes packet pressure without leaking mutable
/// socket internals.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmUdpSocketInfo {
    pub(crate) address: String,
    pub(crate) owner: Option<String>,
    pub(crate) queued_packets: usize,
    pub(crate) queued_bytes: usize,
    pub(crate) inbox_limit: usize,
    pub(crate) waiting_receivers: usize,
    pub(crate) closed: bool,
}

/// Runtime-visible UDP packet readiness wake intent.
///
/// Inputs: a VM process id and the socket that became readable. Output: stable
/// wake intent for the VM scheduler. Transformation: keeps UDP readiness
/// producer logic independent from scheduler queue internals.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmUdpWake {
    Receive {
        process: VmProcessId,
        socket: VmUdpSocket,
    },
}

/// VM-owned UDP packet reactor.
///
/// Inputs:
/// - Bind, send, receive, park, close, and owner-cleanup operations.
///
/// Output:
/// - Deterministic UDP socket handles, packet delivery, wakeups, and stable
///   diagnostics.
///
/// Transformation:
/// - Models datagram readiness and backpressure inside the VM without handing
///   scheduling semantics to a host async runtime.
#[derive(Debug, Default)]
pub(crate) struct VmUdpRuntime {
    next_socket: u64,
    sockets: HashMap<u64, UdpSocketState>,
    addresses: HashMap<String, u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmUdpPacket {
    pub(crate) source: String,
    pub(crate) bytes: Vec<u8>,
}

#[derive(Debug)]
struct UdpSocketState {
    address: String,
    owner: Option<String>,
    inbox: VecDeque<VmUdpPacket>,
    inbox_limit: usize,
    receive_waiters: VecDeque<VmProcessId>,
    closed: bool,
}

impl VmUdpRuntime {
    /// Creates an empty UDP runtime registry.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Binds a VM-owned UDP socket at a logical address.
    pub(crate) fn bind(
        &mut self,
        address: impl Into<String>,
        owner: impl Into<String>,
    ) -> Result<VmUdpSocket, String> {
        self.bind_with_inbox_limit(address, owner, usize::MAX)
    }

    /// Binds a VM-owned UDP socket with bounded packet inbox capacity.
    pub(crate) fn bind_with_inbox_limit(
        &mut self,
        address: impl Into<String>,
        owner: impl Into<String>,
        inbox_limit: usize,
    ) -> Result<VmUdpSocket, String> {
        let address = address.into();
        if address.trim().is_empty() {
            return Err("VM UDP socket address cannot be empty".to_string());
        }
        if inbox_limit == 0 {
            return Err("VM UDP socket inbox limit must be greater than 0".to_string());
        }
        if self.addresses.contains_key(&address) {
            return Err(format!("VM UDP socket `{address}` already exists"));
        }

        self.next_socket = self.next_socket.saturating_add(1);
        let socket = VmUdpSocket {
            id: self.next_socket,
        };
        self.addresses.insert(address.clone(), socket.id);
        self.sockets.insert(
            socket.id,
            UdpSocketState {
                address,
                owner: Some(owner.into()),
                inbox: VecDeque::new(),
                inbox_limit,
                receive_waiters: VecDeque::new(),
                closed: false,
            },
        );
        Ok(socket)
    }

    /// Sends a datagram and returns receiver wake intents.
    pub(crate) fn send_to_with_wakeups(
        &mut self,
        source: VmUdpSocket,
        target_address: &str,
        bytes: Vec<u8>,
    ) -> Result<Vec<VmUdpWake>, String> {
        if bytes.is_empty() {
            return Err("VM UDP packet cannot be empty".to_string());
        }
        let source_address = self.socket(source)?.address.clone();
        let target = self
            .addresses
            .get(target_address)
            .copied()
            .ok_or_else(|| format!("VM UDP socket `{target_address}` was not found"))?;
        let target_socket = VmUdpSocket { id: target };
        let target = self.socket_mut(target_socket)?;
        if target.inbox.len() >= target.inbox_limit {
            return Err(format!("VM UDP socket `{target_address}` inbox is full"));
        }
        target.inbox.push_back(VmUdpPacket {
            source: source_address,
            bytes,
        });
        let wakeups = target
            .receive_waiters
            .drain(..)
            .map(|process| VmUdpWake::Receive {
                process,
                socket: target_socket,
            })
            .collect();
        Ok(wakeups)
    }

    /// Receives the oldest queued datagram.
    pub(crate) fn receive_from(
        &mut self,
        socket: VmUdpSocket,
    ) -> Result<Option<VmUdpPacket>, String> {
        let socket = self.socket_mut(socket)?;
        Ok(socket.inbox.pop_front())
    }

    /// Parks a process until a socket has a datagram ready.
    pub(crate) fn park_receive(
        &mut self,
        socket: VmUdpSocket,
        process: VmProcessId,
    ) -> Result<bool, String> {
        let socket = self.socket_mut(socket)?;
        if !socket.inbox.is_empty() {
            return Ok(false);
        }
        if !socket.receive_waiters.contains(&process) {
            socket.receive_waiters.push_back(process);
        }
        Ok(true)
    }

    /// Closes one VM UDP socket and removes its logical address binding.
    pub(crate) fn close(&mut self, socket: VmUdpSocket) -> Result<(), String> {
        let address = {
            let state = self.socket_mut(socket)?;
            if state.closed {
                return Ok(());
            }
            state.closed = true;
            state.inbox.clear();
            state.receive_waiters.clear();
            state.address.clone()
        };
        self.addresses.remove(&address);
        Ok(())
    }

    /// Closes all UDP sockets owned by one actor.
    pub(crate) fn cancel_owner_sockets(&mut self, owner: &str) -> Vec<VmUdpSocket> {
        let sockets = self
            .sockets
            .iter()
            .filter_map(|(id, socket)| {
                (!socket.closed && socket.owner.as_deref() == Some(owner))
                    .then_some(VmUdpSocket { id: *id })
            })
            .collect::<Vec<_>>();
        for socket in &sockets {
            let _ = self.close(*socket);
        }
        sockets
    }

    /// Returns an inspectable socket snapshot.
    pub(crate) fn inspect_socket(&self, socket: VmUdpSocket) -> Result<VmUdpSocketInfo, String> {
        let state = self.socket(socket)?;
        Ok(VmUdpSocketInfo {
            address: state.address.clone(),
            owner: state.owner.clone(),
            queued_packets: state.inbox.len(),
            queued_bytes: state.inbox.iter().map(|packet| packet.bytes.len()).sum(),
            inbox_limit: state.inbox_limit,
            waiting_receivers: state.receive_waiters.len(),
            closed: state.closed,
        })
    }

    fn socket(&self, socket: VmUdpSocket) -> Result<&UdpSocketState, String> {
        let state = self
            .sockets
            .get(&socket.id)
            .ok_or_else(|| "VM UDP socket was not found".to_string())?;
        if state.closed {
            return Err("VM UDP socket is closed".to_string());
        }
        Ok(state)
    }

    fn socket_mut(&mut self, socket: VmUdpSocket) -> Result<&mut UdpSocketState, String> {
        let state = self
            .sockets
            .get_mut(&socket.id)
            .ok_or_else(|| "VM UDP socket was not found".to_string())?;
        if state.closed {
            return Err("VM UDP socket is closed".to_string());
        }
        Ok(state)
    }
}
