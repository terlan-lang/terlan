//! Maintained TCP resources supplied to the host's protocol task executor.

use std::io::{self, IoSlice, Read, Write};
use std::net::{self, Shutdown};

use mio::{event::Source, Interest, Registry, Token};
use terlan_runtime_abi::poll_io::{IncomingStreams, ReadinessStream};

pub struct TcpIncoming(mio::net::TcpListener);

impl TcpIncoming {
    pub fn new(listener: net::TcpListener) -> io::Result<Self> {
        listener.set_nonblocking(true)?;
        Ok(Self(mio::net::TcpListener::from_std(listener)))
    }
}

impl IncomingStreams for TcpIncoming {
    fn accept(&mut self) -> io::Result<Box<dyn ReadinessStream>> {
        self.0
            .accept()
            .map(|(stream, _)| Box::new(TcpConnection(stream)) as Box<dyn ReadinessStream>)
    }
}

pub struct TcpConnection(mio::net::TcpStream);

impl TcpConnection {
    pub fn new(stream: net::TcpStream) -> io::Result<Self> {
        stream.set_nonblocking(true)?;
        Ok(Self(mio::net::TcpStream::from_std(stream)))
    }
}

impl Read for TcpConnection {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.read(buffer)
    }
}

impl Write for TcpConnection {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.write(buffer)
    }

    fn write_vectored(&mut self, buffers: &[IoSlice<'_>]) -> io::Result<usize> {
        self.0.write_vectored(buffers)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

impl ReadinessStream for TcpConnection {
    fn shutdown_write(&self) -> io::Result<()> {
        self.0.shutdown(Shutdown::Write)
    }
}

macro_rules! source {
    ($ty:ty) => {
        impl Source for $ty {
            fn register(
                &mut self,
                registry: &Registry,
                token: Token,
                interests: Interest,
            ) -> io::Result<()> {
                self.0.register(registry, token, interests)
            }
            fn reregister(
                &mut self,
                registry: &Registry,
                token: Token,
                interests: Interest,
            ) -> io::Result<()> {
                self.0.reregister(registry, token, interests)
            }
            fn deregister(&mut self, registry: &Registry) -> io::Result<()> {
                self.0.deregister(registry)
            }
        }
    };
}

source!(TcpIncoming);
source!(TcpConnection);

#[cfg(test)]
#[path = "ready_test.rs"]
mod tests;
