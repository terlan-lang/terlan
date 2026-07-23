//! Deterministic EPMD registry state and command handling.

use std::{
    collections::{BTreeMap, VecDeque},
    format,
    vec::Vec,
};

use super::protocol::{
    encode_alive2_response, encode_port2_response, validate_extra, validate_name, Alive2Request,
    Alive2Response, Port2Found, Port2Response, RegistrationResult, Request,
};

/// Maximum number of unregistered node records retained for creation reuse.
pub const MAX_UNREGISTERED: usize = 1000;

/// A unique identifier for the socket that owns an ALIVE2 registration.
#[derive(Copy, Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConnectionId(u64);

impl ConnectionId {
    /// Create a connection identifier from a numeric socket lifecycle token.
    pub fn new(value: u64) -> Self {
        Self(value)
    }

    /// Return the raw numeric connection identifier.
    pub fn get(self) -> u64 {
        self.0
    }
}

/// Runtime options that affect server command behavior.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerOptions {
    /// TCP port reported in names and dump responses.
    pub epmd_port: u16,
    /// Allow STOP and live-node KILL behavior used by OTP's relaxed mode.
    pub relaxed_command_check: bool,
    /// Maximum unregistered records kept for creation reuse.
    pub max_unregistered: usize,
}

impl ServerOptions {
    /// Return server options for a specific epmd port.
    pub fn new(epmd_port: u16) -> Self {
        Self {
            epmd_port,
            relaxed_command_check: false,
            max_unregistered: MAX_UNREGISTERED,
        }
    }

    /// Return options with relaxed command checks enabled.
    pub fn with_relaxed_command_check(mut self, enabled: bool) -> Self {
        self.relaxed_command_check = enabled;
        self
    }

    /// Return options with a custom unregistered record retention limit.
    pub fn with_max_unregistered(mut self, max_unregistered: usize) -> Self {
        self.max_unregistered = max_unregistered;
        self
    }
}

/// Mutable EPMD server state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerState {
    /// Runtime options for command behavior.
    pub options: ServerOptions,
    /// Currently registered nodes keyed by node name bytes.
    registered: BTreeMap<Vec<u8>, RegisteredNode>,
    /// Old node registrations retained for creation reuse.
    unregistered: VecDeque<RegisteredNode>,
    /// Next seed used when allocating a new creation counter.
    next_creation_seed: u32,
}

impl ServerState {
    /// Create empty server state.
    pub fn new(options: ServerOptions) -> Self {
        Self {
            options,
            registered: BTreeMap::new(),
            unregistered: VecDeque::new(),
            next_creation_seed: 4,
        }
    }

    /// Return the number of currently registered nodes.
    pub fn registered_len(&self) -> usize {
        self.registered.len()
    }

    /// Return the number of retained unregistered node records.
    pub fn unregistered_len(&self) -> usize {
        self.unregistered.len()
    }

    /// Register an ALIVE2 node for a connection.
    pub fn register_alive2(
        &mut self,
        connection_id: ConnectionId,
        request: &Alive2Request,
    ) -> RegistrationAttempt {
        if validate_name(&request.name).is_err() || validate_extra(&request.extra).is_err() {
            return RegistrationAttempt {
                result: RegistrationResult::Error,
                creation: 99,
                registered: false,
            };
        }

        if self.registered.contains_key(&request.name) {
            return RegistrationAttempt {
                result: RegistrationResult::Error,
                creation: 99,
                registered: false,
            };
        }

        let mut node = self
            .take_unregistered_by_name(&request.name)
            .unwrap_or_else(|| self.new_unregistered_slot());
        node.connection_id = connection_id;
        node.port = request.port;
        node.node_type = request.node_type;
        node.protocol = request.protocol;
        node.highest_version = request.highest_version;
        node.lowest_version = request.lowest_version;
        node.name = request.name.clone();
        node.extra = request.extra.clone();
        let creation = node.creation();
        self.registered.insert(node.name.clone(), node);
        RegistrationAttempt {
            result: RegistrationResult::Ok,
            creation,
            registered: true,
        }
    }

    /// Look up a registered node by name.
    pub fn lookup(&self, name: &[u8]) -> Option<Port2Found> {
        self.registered
            .get(name)
            .map(RegisteredNode::to_port2_found)
    }

    /// Unregister a node by connection owner.
    pub fn unregister_connection(&mut self, connection_id: ConnectionId) -> Option<RegisteredNode> {
        let name = self
            .registered
            .iter()
            .find_map(|(name, node)| (node.connection_id == connection_id).then(|| name.clone()))?;
        self.unregister_name(&name)
    }

    /// Unregister a node by name.
    pub fn unregister_name(&mut self, name: &[u8]) -> Option<RegisteredNode> {
        let node = self.registered.remove(name)?;
        self.push_unregistered(node.clone());
        Some(node)
    }

    /// Return a names response body.
    pub fn names_response(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&u32::from(self.options.epmd_port).to_be_bytes());
        for node in self.registered.values() {
            out.extend_from_slice(b"name ");
            out.extend_from_slice(&node.name);
            out.extend_from_slice(format!(" at port {}\n", node.port).as_bytes());
        }
        out
    }

    /// Return a dump response body.
    pub fn dump_response(&self) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(&u32::from(self.options.epmd_port).to_be_bytes());
        for node in self.registered.values() {
            out.extend_from_slice(
                format!(
                    "active name     <{}> at port {}, fd = {}\n",
                    core::str::from_utf8(&node.name).unwrap_or("<invalid>"),
                    node.port,
                    node.connection_id.get()
                )
                .as_bytes(),
            );
        }
        for node in &self.unregistered {
            out.extend_from_slice(
                format!(
                    "old/unused name <{}>, port = {}, fd = {} \n",
                    core::str::from_utf8(&node.name).unwrap_or("<invalid>"),
                    node.port,
                    node.connection_id.get()
                )
                .as_bytes(),
            );
        }
        out
    }

    /// Handle one decoded request and mutate server state as needed.
    pub fn handle_request(
        &mut self,
        connection_id: ConnectionId,
        local_peer: bool,
        request: Request,
    ) -> ServerReply {
        match request {
            Request::Alive2(alive) => self.handle_alive2(connection_id, local_peer, &alive),
            Request::Port2(port) => {
                let response = self
                    .lookup(&port.name)
                    .map(Port2Response::Found)
                    .unwrap_or(Port2Response::NotFound);
                let bytes = encode_port2_response(&response)
                    .expect("server state only stores protocol-valid node records");
                ServerReply::close(bytes)
            }
            Request::Names => ServerReply::close(self.names_response()),
            Request::Dump => {
                if !local_peer {
                    return ServerReply::silent_close();
                }
                ServerReply::close(self.dump_response())
            }
            Request::Kill => self.handle_kill(local_peer),
            Request::Stop(stop) => self.handle_stop(local_peer, &stop.name),
        }
    }

    /// Applies one local ALIVE2 request and encodes its registration result.
    fn handle_alive2(
        &mut self,
        connection_id: ConnectionId,
        local_peer: bool,
        request: &Alive2Request,
    ) -> ServerReply {
        if !local_peer {
            return ServerReply::silent_close();
        }
        let attempt = self.register_alive2(connection_id, request);
        let bytes = encode_alive2_response(&Alive2Response {
            result: attempt.result,
            creation: attempt.creation,
            extended_creation: request.highest_version >= 6,
        });
        ServerReply {
            bytes,
            keep_connection: attempt.registered,
            shutdown: false,
        }
    }

    /// Applies local-peer and live-registration policy to one KILL request.
    fn handle_kill(&self, local_peer: bool) -> ServerReply {
        if !local_peer {
            return ServerReply::silent_close();
        }
        if !self.options.relaxed_command_check && !self.registered.is_empty() {
            return ServerReply::close(b"NO".to_vec());
        }
        ServerReply {
            bytes: b"OK".to_vec(),
            keep_connection: false,
            shutdown: true,
        }
    }

    /// Applies relaxed-mode policy to one named STOP request.
    fn handle_stop(&mut self, local_peer: bool, name: &[u8]) -> ServerReply {
        if !local_peer || !self.options.relaxed_command_check {
            return ServerReply::silent_close();
        }
        if self.unregister_name(name).is_some() {
            ServerReply::close(b"STOPPED".to_vec())
        } else {
            ServerReply::close(b"NOEXISTSTOPPED".to_vec())
        }
    }

    /// Recovers and advances a retained creation slot for the same node name.
    fn take_unregistered_by_name(&mut self, name: &[u8]) -> Option<RegisteredNode> {
        let index = self
            .unregistered
            .iter()
            .position(|node| node.name.as_slice() == name)?;
        let mut node = self
            .unregistered
            .remove(index)
            .expect("position was returned for retained unregistered node");
        node.bump_creation();
        Some(node)
    }

    /// Allocates or recycles one bounded retained registration slot.
    fn new_unregistered_slot(&mut self) -> RegisteredNode {
        if self.unregistered.len() > self.options.max_unregistered {
            let mut node = self
                .unregistered
                .pop_front()
                .expect("retained unregistered node must exist");
            node.bump_creation();
            node
        } else {
            let seed = self.next_creation_seed;
            self.next_creation_seed = self.next_creation_seed.wrapping_add(1).max(4);
            RegisteredNode::new(ConnectionId::new(0), seed)
        }
    }

    /// Retains one closed registration for future creation reuse.
    fn push_unregistered(&mut self, node: RegisteredNode) {
        self.unregistered.push_back(node);
    }
}

/// Details of one node registration attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistrationAttempt {
    /// Registration result sent to the client.
    pub result: RegistrationResult,
    /// Creation value sent to the client.
    pub creation: u32,
    /// Whether the node was inserted into the active table.
    pub registered: bool,
}

/// A registered or retained EPMD node.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegisteredNode {
    /// Connection that owns the registration.
    pub connection_id: ConnectionId,
    /// TCP port used by the registered node.
    pub port: u16,
    /// OTP node type byte.
    pub node_type: u8,
    /// OTP distribution protocol byte.
    pub protocol: u8,
    /// Highest distribution version.
    pub highest_version: u16,
    /// Lowest distribution version.
    pub lowest_version: u16,
    /// UTF-8 encoded node name bytes.
    pub name: Vec<u8>,
    /// Opaque extra data bytes.
    pub extra: Vec<u8>,
    /// Internal creation counter.
    pub creation_counter: u32,
}

impl RegisteredNode {
    /// Create an empty retained-node slot with a creation counter seed.
    pub fn new(connection_id: ConnectionId, creation_counter: u32) -> Self {
        Self {
            connection_id,
            port: 0,
            node_type: 0,
            protocol: 0,
            highest_version: 0,
            lowest_version: 0,
            name: Vec::new(),
            extra: Vec::new(),
            creation_counter,
        }
    }

    /// Return the creation value visible to clients.
    pub fn creation(&self) -> u32 {
        if self.highest_version >= 6 {
            self.creation_counter
        } else {
            self.creation_counter % 3 + 1
        }
    }

    /// Increment the creation counter according to OTP epmd rules.
    pub fn bump_creation(&mut self) {
        self.creation_counter = self.creation_counter.wrapping_add(1);
        if self.creation_counter < 4 {
            self.creation_counter = 4;
        }
    }

    /// Convert this node into a PORT2 success payload.
    pub fn to_port2_found(&self) -> Port2Found {
        Port2Found {
            port: self.port,
            node_type: self.node_type,
            protocol: self.protocol,
            highest_version: self.highest_version,
            lowest_version: self.lowest_version,
            name: self.name.clone(),
            extra: self.extra.clone(),
        }
    }
}

/// The action to take after a request is handled.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ServerReply {
    /// Bytes written to the client.
    pub bytes: Vec<u8>,
    /// Whether the connection should remain open as an ALIVE2 owner.
    pub keep_connection: bool,
    /// Whether the server should shut down after the reply.
    pub shutdown: bool,
}

impl ServerReply {
    /// Return a reply that closes the connection after writing bytes.
    pub fn close(bytes: Vec<u8>) -> Self {
        Self {
            bytes,
            keep_connection: false,
            shutdown: false,
        }
    }

    /// Return a reply that closes the connection without writing bytes.
    pub fn silent_close() -> Self {
        Self::close(Vec::new())
    }
}
