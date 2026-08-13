//! EPMD connection handling on the fixed VM protocol executor.

use std::future::Future;
use std::io::{self, Read, Write};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};

use super::protocol::{
    encode_alive2_validation_error_response, is_alive2_validation_error, parse_payload,
    parse_alive2_payload_unvalidated, ALIVE2_REQ,
};
use super::state::{ConnectionId, ServerOptions, ServerReply, ServerState};
use super::super::protocol_task_executor::{
    VmProtocolTaskFactory, VmProtocolTaskRoute, VmReadyTcpStream,
};

/// Shared deterministic EPMD registry used by fixed protocol owners.
pub(crate) type VmSharedEpmdState = Arc<Mutex<ServerState>>;

/// Creates an empty shared EPMD registry.
#[cfg(test)]
pub(crate) fn shared_state(options: ServerOptions) -> VmSharedEpmdState {
    Arc::new(Mutex::new(ServerState::new(options)))
}

/// Builds a protocol-task factory for EPMD discovery connections.
#[cfg(test)]
pub(crate) fn protocol_factory(state: VmSharedEpmdState) -> VmProtocolTaskFactory {
    Arc::new(move |stream, route| {
        Box::pin(VmEpmdConnection::new(
            stream,
            route,
            Arc::clone(&state),
        ))
    })
}


/// Incremental state of one length-prefixed EPMD connection.
#[derive(Debug)]
enum VmEpmdConnectionPhase {
    /// Reading the two-byte request payload length.
    ReadingLength { bytes: [u8; 2], offset: usize },
    /// Reading exactly one request payload.
    ReadingPayload { bytes: Vec<u8>, offset: usize },
    /// Writing the complete command response.
    Writing {
        reply: ServerReply,
        offset: usize,
    },
    /// Retaining an ALIVE2 registration until socket closure.
    HoldingRegistration,
    /// The connection and any owned registration are complete.
    Complete,
}

/// One EPMD socket future owned by a fixed protocol scheduler.
struct VmEpmdConnection {
    stream: VmReadyTcpStream,
    route: VmProtocolTaskRoute,
    state: VmSharedEpmdState,
    connection: ConnectionId,
    phase: VmEpmdConnectionPhase,
    owns_registration: bool,
}

impl VmEpmdConnection {
    /// Creates a connection future bound to its exact protocol-task route.
    fn new(
        stream: VmReadyTcpStream,
        route: VmProtocolTaskRoute,
        state: VmSharedEpmdState,
    ) -> Self {
        Self {
            stream,
            route,
            state,
            connection: ConnectionId::new(route.process().as_u64()),
            phase: VmEpmdConnectionPhase::ReadingLength {
                bytes: [0; 2],
                offset: 0,
            },
            owns_registration: false,
        }
    }

    /// Advances one connection until host readiness is required or it completes.
    fn poll_connection(&mut self) -> Poll<Result<(), String>> {
        loop {
            match &mut self.phase {
                VmEpmdConnectionPhase::ReadingLength { bytes, offset } => {
                    match read_available(&mut self.stream, bytes, offset)? {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(()) => {
                            let payload_len = usize::from(u16::from_be_bytes(*bytes));
                            self.phase = VmEpmdConnectionPhase::ReadingPayload {
                                bytes: vec![0; payload_len],
                                offset: 0,
                            };
                        }
                    }
                }
                VmEpmdConnectionPhase::ReadingPayload { bytes, offset } => {
                    match read_available(&mut self.stream, bytes, offset)? {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(()) => {
                            let reply = handle_payload(&self.state, self.connection, bytes)?;
                            self.owns_registration = reply.keep_connection;
                            self.phase = VmEpmdConnectionPhase::Writing { reply, offset: 0 };
                        }
                    }
                }
                VmEpmdConnectionPhase::Writing { reply, offset } => {
                    match write_available(&mut self.stream, &reply.bytes, offset)? {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(()) if reply.keep_connection => {
                            self.phase = VmEpmdConnectionPhase::HoldingRegistration;
                        }
                        Poll::Ready(()) => {
                            self.phase = VmEpmdConnectionPhase::Complete;
                        }
                    }
                }
                VmEpmdConnectionPhase::HoldingRegistration => {
                    let mut discard = [0; 64];
                    match self.stream.read(&mut discard) {
                        Ok(0) => {
                            self.unregister();
                            self.phase = VmEpmdConnectionPhase::Complete;
                        }
                        Ok(_) => {}
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            return Poll::Pending;
                        }
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                        Err(error) => return Poll::Ready(Err(render_io("hold registration", error))),
                    }
                }
                VmEpmdConnectionPhase::Complete => return Poll::Ready(Ok(())),
            }
        }
    }

    /// Releases this connection's ALIVE2 registration exactly once.
    fn unregister(&mut self) {
        if !self.owns_registration {
            return;
        }
        if let Ok(mut state) = self.state.lock() {
            state.unregister_connection(self.connection);
        }
        self.owns_registration = false;
    }
}

/// Parses and applies one complete request without holding a lock during I/O.
pub(super) fn handle_payload(
    state: &VmSharedEpmdState,
    connection: ConnectionId,
    payload: &[u8],
) -> Result<ServerReply, String> {
    let request = match parse_payload(payload) {
        Ok(request) => request,
        Err(error)
            if payload.first() == Some(&ALIVE2_REQ) && is_alive2_validation_error(&error) =>
        {
            let request = parse_alive2_payload_unvalidated(payload)
                .map_err(|error| format!("error[vm.epmd.protocol]: {error:?}"))?;
            return Ok(ServerReply::close(
                encode_alive2_validation_error_response(request.highest_version >= 6),
            ));
        }
        Err(error) => return Err(format!("error[vm.epmd.protocol]: {error:?}")),
    };
    state
        .lock()
        .map_err(|_| "error[vm.epmd.state]: registry lock poisoned".to_string())
        .map(|mut state| state.handle_request(connection, true, request))
}

impl Future for VmEpmdConnection {
    /// Terminal connection result after response or registration release.
    type Output = Result<(), String>;

    /// Polls only under the exact fixed protocol owner selected at admission.
    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
        if let Err(error) = self.route.validate_completion_origin() {
            return Poll::Ready(Err(error));
        }
        self.poll_connection()
    }
}

impl Drop for VmEpmdConnection {
    /// Releases a registration when transport failure drops the connection future.
    fn drop(&mut self) {
        self.unregister();
    }
}

/// Reads until one fixed buffer is complete or host readiness is exhausted.
fn read_available(
    stream: &mut impl Read,
    bytes: &mut [u8],
    offset: &mut usize,
) -> Result<Poll<()>, String> {
    while *offset < bytes.len() {
        match stream.read(&mut bytes[*offset..]) {
            Ok(0) => return Err("error[vm.epmd.io]: unexpected end of request".to_string()),
            Ok(read) => *offset += read,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(Poll::Pending),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(render_io("read request", error)),
        }
    }
    Ok(Poll::Ready(()))
}

/// Writes until one fixed response is complete or host readiness is exhausted.
fn write_available(
    stream: &mut impl Write,
    bytes: &[u8],
    offset: &mut usize,
) -> Result<Poll<()>, String> {
    while *offset < bytes.len() {
        match stream.write(&bytes[*offset..]) {
            Ok(0) => return Err("error[vm.epmd.io]: response write made no progress".to_string()),
            Ok(written) => *offset += written,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Ok(Poll::Pending),
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(render_io("write response", error)),
        }
    }
    Ok(Poll::Ready(()))
}

/// Renders one stable EPMD transport diagnostic.
fn render_io(operation: &str, error: io::Error) -> String {
    format!("error[vm.epmd.io]: {operation}: {error}")
}
