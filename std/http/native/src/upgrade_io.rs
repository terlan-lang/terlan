//! Preserves Hyper read-ahead when a connection becomes a package protocol stream.
//!
//! The host selects the permitted plain and TLS transport types. This adapter
//! does not authenticate a request, schedule work, or alter readiness handling.

use std::io::{self, Read, Write};

use bytes::{Buf, Bytes};

pub struct UpgradeIo<P, T> {
    prefix: Bytes,
    stream: Transport<P, T>,
}

enum Transport<P, T> {
    Plain(P),
    Tls(Box<T>),
}

impl<P, T> UpgradeIo<P, T>
where
    P: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
    T: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
{
    /// Rejects foreign transports instead of losing bytes or changing I/O owners.
    pub fn from_upgraded(upgraded: hyper::upgrade::Upgraded) -> Result<Self, crate::ServiceError> {
        let (prefix, stream) = match upgraded.downcast::<P>() {
            Ok(parts) => (parts.read_buf, Transport::Plain(parts.io)),
            Err(upgraded) => {
                let parts = upgraded.downcast::<T>().map_err(|_| {
                    "error[serve.websocket.upgrade]: Hyper returned an unexpected transport type"
                        .to_string()
                })?;
                (parts.read_buf, Transport::Tls(Box::new(parts.io)))
            }
        };
        Ok(Self { prefix, stream })
    }
}

impl<P: Read, T: Read> Read for UpgradeIo<P, T> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if !self.prefix.is_empty() {
            let count = self.prefix.len().min(buffer.len());
            self.prefix.copy_to_slice(&mut buffer[..count]);
            return Ok(count);
        }
        match &mut self.stream {
            Transport::Plain(stream) => stream.read(buffer),
            Transport::Tls(stream) => stream.read(buffer),
        }
    }
}

impl<P: Write, T: Write> Write for UpgradeIo<P, T> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match &mut self.stream {
            Transport::Plain(stream) => stream.write(buffer),
            Transport::Tls(stream) => stream.write(buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.stream {
            Transport::Plain(stream) => stream.flush(),
            Transport::Tls(stream) => stream.flush(),
        }
    }
}

#[cfg(test)]
#[path = "upgrade_io_test.rs"]
mod tests;
