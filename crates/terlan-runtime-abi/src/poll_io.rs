//! Package-supplied nonblocking I/O on a host-owned readiness registry.
//! Implementations supply resources, never a reactor or executor.

#[cfg(test)]
#[path = "poll_io_test.rs"]
mod tests;

use std::io::{self, IoSlice, Read, Write};

use mio::event::Source;

/// A registered-capable, unbuffered byte stream. All I/O must be nonblocking.
/// Dropping the value releases its resource. EOF and WouldBlock retain their
/// std meanings. Only initialized buffers cross this safe extension boundary.
pub trait ReadinessStream: Source + Read + Write + Send {
    fn shutdown_write(&self) -> io::Result<()>;
}

impl<T: ReadinessStream + ?Sized> ReadinessStream for Box<T> {
    fn shutdown_write(&self) -> io::Result<()> {
        (**self).shutdown_write()
    }
}

/// A nonblocking source of streams. The host owns acceptance budgets,
/// registration tokens, capacity reservations, and placement of each stream.
pub trait IncomingStreams: Source + Send {
    fn accept(&mut self) -> io::Result<Box<dyn ReadinessStream>>;
}

/// The host registers write readiness without transferring scheduler ownership.
/// Successful registration must preserve existing read interest.
pub trait WriteInterest<S> {
    fn arm_writable(&mut self, stream: &mut S) -> io::Result<()>;
}

/// One task's stream with lazy host-directed write-interest registration.
pub struct ReadyStream<S, I> {
    stream: S,
    interest: I,
    writable_interest_armed: bool,
}

impl<S, I> ReadyStream<S, I> {
    pub fn new(stream: S, interest: I) -> Self {
        Self {
            stream,
            interest,
            writable_interest_armed: false,
        }
    }
}

impl<S, I: WriteInterest<S>> ReadyStream<S, I> {
    fn write_result(&mut self, result: io::Result<usize>) -> io::Result<usize> {
        if result
            .as_ref()
            .is_err_and(|error| error.kind() == io::ErrorKind::WouldBlock)
            && !self.writable_interest_armed
        {
            self.interest.arm_writable(&mut self.stream)?;
            self.writable_interest_armed = true;
        }
        result
    }
}

impl<S: ReadinessStream, I> ReadyStream<S, I> {
    pub fn shutdown_write(&self) -> io::Result<()> {
        self.stream.shutdown_write()
    }
}

impl<S: Read, I> Read for ReadyStream<S, I> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.stream.read(buffer)
    }
}

impl<S: Write, I: WriteInterest<S>> Write for ReadyStream<S, I> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let result = self.stream.write(buffer);
        self.write_result(result)
    }

    fn write_vectored(&mut self, buffers: &[IoSlice<'_>]) -> io::Result<usize> {
        let result = self.stream.write_vectored(buffers);
        self.write_result(result)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
