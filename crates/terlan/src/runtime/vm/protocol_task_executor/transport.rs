//! Nonblocking transport facade owned by one VM protocol task.

use std::io::{self, Read, Write};
use std::mem::MaybeUninit;
use std::net::Shutdown;
use std::sync::Arc;

use mio::net::TcpStream;
use mio::{Interest, Token};
use socket2::SockRef;

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

/// Nonblocking transport handed to a protocol adapter after VM registration.
pub(crate) struct VmReadyTcpStream {
    stream: TcpStream,
    owner: Arc<VmProtocolOwnerWake>,
    token: Token,
    writable_interest_armed: bool,
}

impl VmReadyTcpStream {
    pub(super) fn new(stream: TcpStream, owner: Arc<VmProtocolOwnerWake>, token: Token) -> Self {
        Self {
            stream,
            owner,
            token,
            writable_interest_armed: false,
        }
    }

    /// Adds write readiness only after a direct nonblocking write stalls.
    fn arm_writable_interest(&mut self) -> io::Result<()> {
        if self.writable_interest_armed {
            return Ok(());
        }
        self.owner.registry.reregister(
            &mut self.stream,
            self.token,
            Interest::READABLE.add(Interest::WRITABLE),
        )?;
        self.writable_interest_armed = true;
        Ok(())
    }

    pub(crate) fn shutdown_write(&self) -> io::Result<()> {
        self.stream.shutdown(Shutdown::Write)
    }

    /// Reads directly into protocol-owned uninitialized receive storage.
    ///
    /// `socket2` exposes the operating-system receive contract without first
    /// constructing an initialized `u8` slice, so maintained protocol
    /// adapters can avoid a zero-fill and an intermediate copy.
    pub(crate) fn read_uninit(&self, buffer: &mut [MaybeUninit<u8>]) -> io::Result<usize> {
        SockRef::from(&self.stream).recv(buffer)
    }
}

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

impl Read for VmReadyTcpStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buffer)
    }
}

impl Write for VmReadyTcpStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self.stream.write(buffer) {
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                self.arm_writable_interest()?;
                Err(error)
            }
            outcome => outcome,
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn write_vectored(&mut self, buffers: &[io::IoSlice<'_>]) -> io::Result<usize> {
        match self.stream.write_vectored(buffers) {
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                self.arm_writable_interest()?;
                Err(error)
            }
            outcome => outcome,
        }
    }
}
