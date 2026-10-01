//! Nonblocking transport facade owned by one VM protocol task.

use std::io;
use std::sync::Arc;

use mio::{Interest, Token};
use terlan_runtime_abi::poll_io::{ReadinessStream, ReadyStream, WriteInterest};

use super::{current_protocol_scheduler, VmProtocolOwnerWake};
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::scheduler_topology::VmSchedulerId;

pub(super) fn render_io(operation: &'static str) -> impl FnOnce(io::Error) -> String {
    move |error| format!("error[vm.protocol_io]: {operation}: {error}")
}

/// Immutable VM ownership retained for one socket task's whole lifetime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmProtocolTaskRoute {
    pub(crate) process: VmProcessId,
    pub(crate) scheduler: VmSchedulerId,
}

/// Typed host readiness; protocol adapters never schedule themselves.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmSocketReadinessWake {
    pub(crate) route: VmProtocolTaskRoute,
    pub(crate) readable: bool,
    pub(crate) writable: bool,
    pub(crate) closed: bool,
}

#[derive(Clone, Copy)]
pub(super) struct VmReadyEvent {
    token: Token,
    pub(super) readable: bool,
    pub(super) writable: bool,
    pub(super) closed: bool,
}

impl VmReadyEvent {
    pub(super) fn token(self) -> Token {
        self.token
    }
}

impl From<&mio::event::Event> for VmReadyEvent {
    fn from(event: &mio::event::Event) -> Self {
        Self {
            token: event.token(),
            readable: event.is_readable(),
            writable: event.is_writable(),
            closed: event.is_read_closed() || event.is_write_closed() || event.is_error(),
        }
    }
}

/// VM ownership adapter for the package-owned socket transport.
pub(crate) type VmReadyStream = ReadyStream<Box<dyn ReadinessStream>, VmSocketInterest>;

pub(crate) struct VmSocketInterest {
    owner: Arc<VmProtocolOwnerWake>,
    token: Token,
}

pub(super) fn ready_stream(
    stream: Box<dyn ReadinessStream>,
    owner: Arc<VmProtocolOwnerWake>,
    token: Token,
) -> VmReadyStream {
    ReadyStream::new(stream, VmSocketInterest { owner, token })
}

impl<S: mio::event::Source> WriteInterest<S> for VmSocketInterest {
    fn arm_writable(&mut self, stream: &mut S) -> io::Result<()> {
        self.owner.registry.reregister(
            stream,
            self.token,
            Interest::READABLE.add(Interest::WRITABLE),
        )
    }
}

#[cfg(test)]
#[path = "transport_test.rs"]
mod tests;

impl VmProtocolTaskRoute {
    /// Returns the protocol process that owns this connection task.
    pub(crate) const fn process(self) -> VmProcessId {
        self.process
    }

    /// Returns the fixed protocol scheduler that owns this connection task.
    pub(crate) const fn scheduler(self) -> VmSchedulerId {
        self.scheduler
    }

    /// Verifies that a completion is being published by its protocol owner.
    pub(crate) fn validate_completion_origin(self) -> Result<(), String> {
        match current_protocol_scheduler() {
            Some(scheduler) if scheduler == self.scheduler() => Ok(()),
            Some(scheduler) => Err(format!(
                "error[vm.protocol_completion_owner]: process {} belongs to scheduler {}, not scheduler {}",
                self.process().as_u64(),
                self.scheduler().index(),
                scheduler.index()
            )),
            None => Err(format!(
                "error[vm.protocol_completion_owner]: process {} completion was published outside a protocol scheduler",
                self.process().as_u64()
            )),
        }
    }
}
