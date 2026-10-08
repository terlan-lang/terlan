//! Preserves Hyper read-ahead when a connection becomes a package protocol stream.
//!
//! The connection selects its exact admitted transport type. This adapter
//! does not authenticate a request, schedule work, or alter readiness handling.

use std::io::{self, Read, Write};

use bytes::{Buf, Bytes};

pub struct UpgradeIo<I> {
    prefix: Bytes,
    stream: I,
}

impl<I> UpgradeIo<I>
where
    I: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
{
    /// Rejects foreign transports instead of losing bytes or changing I/O owners.
    pub fn from_upgraded(upgraded: hyper::upgrade::Upgraded) -> Result<Self, crate::ServiceError> {
        let parts = upgraded.downcast::<I>().map_err(|_| {
            "error[serve.websocket.upgrade]: Hyper returned an unexpected transport type"
                .to_string()
        })?;
        Ok(Self {
            prefix: parts.read_buf,
            stream: parts.io,
        })
    }
}

impl<I: Read> Read for UpgradeIo<I> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if !self.prefix.is_empty() {
            let count = self.prefix.len().min(buffer.len());
            self.prefix.copy_to_slice(&mut buffer[..count]);
            return Ok(count);
        }
        self.stream.read(buffer)
    }
}

impl<I: Write> Write for UpgradeIo<I> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.stream.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stream.flush()
    }
}

#[cfg(test)]
#[path = "upgrade_io_test.rs"]
mod tests;
