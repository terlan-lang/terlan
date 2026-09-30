//! Hyper buffer adaptation and HTTP ALPN policy over package-owned TLS I/O.

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use hyper::rt::{Read, ReadBufCursor, Write};
use terlan_net_native::tls_stream::{ShutdownWrite, TlsStream};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpProtocol {
    Http1,
    Http2,
}

pub struct TlsIo<S>(TlsStream<S>);

impl<S: io::Read + io::Write> TlsIo<S> {
    pub fn new(stream: TlsStream<S>) -> Self {
        Self(stream)
    }

    pub fn negotiated_protocol(&self) -> io::Result<HttpProtocol> {
        match self.0.alpn_protocol() {
            Some(b"h2") => Ok(HttpProtocol::Http2),
            Some(b"http/1.1") | None => Ok(HttpProtocol::Http1),
            Some(protocol) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "error[serve_tls.alpn]: unsupported negotiated protocol `{}`",
                    String::from_utf8_lossy(protocol)
                ),
            )),
        }
    }
}

impl<S: io::Read + io::Write + Unpin> Read for TlsIo<S> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        mut buffer: ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        let mut plaintext = [0; 16 * 1024];
        let limit = plaintext.len().min(buffer.remaining());
        let read = std::task::ready!(self.0.poll_read(&mut plaintext[..limit]))?;
        buffer.put_slice(&plaintext[..read]);
        Poll::Ready(Ok(()))
    }
}

impl<S: io::Read + io::Write + ShutdownWrite + Unpin> Write for TlsIo<S> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.0.poll_write(buffer)
    }

    fn poll_flush(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.0.poll_flush()
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.0.poll_shutdown()
    }
}

impl<S: io::Read + io::Write> io::Read for TlsIo<S> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.0.read(buffer)
    }
}

impl<S: io::Read + io::Write> io::Write for TlsIo<S> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        self.0.write(buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

#[cfg(test)]
#[path = "tls_test.rs"]
mod tests;
