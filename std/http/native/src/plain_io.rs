//! Hyper adaptation for host-registered nonblocking streams.
//! The host routes readiness to the connection task; this adapter owns no reactor.

use std::io::{self, IoSlice};
use std::pin::Pin;
use std::task::{Context, Poll};

use hyper::rt::{Read, ReadBufCursor, Write};
use terlan_net_native::tls_stream::ShutdownWrite;

pub struct PlainIo<S>(S);

impl<S> PlainIo<S> {
    pub fn new(stream: S) -> Self {
        Self(stream)
    }
}

// Interrupted I/O must not monopolize an owner indefinitely. Only exhaustion
// self-wakes; WouldBlock relies on the host's readiness registration.
fn poll_io<T>(
    context: &mut Context<'_>,
    mut operation: impl FnMut() -> io::Result<T>,
) -> Poll<io::Result<T>> {
    for _ in 0..16 {
        match operation() {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Poll::Pending,
            result => return Poll::Ready(result),
        }
    }
    context.waker().wake_by_ref();
    Poll::Pending
}

impl<S: io::Read + Unpin> Read for PlainIo<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        mut buffer: ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        if buffer.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        // Safe Read implementations cannot be trusted with Hyper's uninitialized
        // memory. Validate the count before publishing initialized bytes.
        let mut bytes = [0; 8192];
        let limit = buffer.remaining().min(bytes.len());
        let read = std::task::ready!(poll_io(context, || self.0.read(&mut bytes[..limit])))?;
        if read > limit {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "stream returned an out-of-bounds read count",
            )));
        }
        buffer.put_slice(&bytes[..read]);
        Poll::Ready(Ok(()))
    }
}

impl<S: io::Write + ShutdownWrite + Unpin> Write for PlainIo<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[u8],
    ) -> Poll<io::Result<usize>> {
        poll_io(context, || self.0.write(bytes))
    }
    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        bytes: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        poll_io(context, || self.0.write_vectored(bytes))
    }
    fn is_write_vectored(&self) -> bool {
        true
    }
    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        poll_io(context, || self.0.flush())
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        poll_io(context, || self.0.shutdown_write())
    }
}

impl<S: io::Read> io::Read for PlainIo<S> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        self.0.read(bytes)
    }
}
impl<S: io::Write> io::Write for PlainIo<S> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.write(bytes)
    }
    fn write_vectored(&mut self, bytes: &[IoSlice<'_>]) -> io::Result<usize> {
        self.0.write_vectored(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

#[cfg(test)]
#[path = "plain_io_test.rs"]
mod tests;
