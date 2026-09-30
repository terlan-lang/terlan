//! Nonblocking rustls I/O over an owner-provided transport. No socket reactor,
//! timers, HTTP policy, or executor is created here.

use std::future::{poll_fn, Future};
use std::io::{self, Read, Write};
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;

use rustls::{ServerConfig, ServerConnection};

/// A transport whose owner supports closing its write half after TLS closure.
pub trait ShutdownWrite {
    fn shutdown_write(&mut self) -> io::Result<()>;
}

/// TLS record and plaintext state over a nonblocking transport. The transport
/// must arrange an owner wakeup when a read or write returns `WouldBlock`.
pub struct TlsStream<S> {
    stream: S,
    connection: ServerConnection,
}

impl<S: Read + Write> TlsStream<S> {
    pub fn new(stream: S, config: Arc<ServerConfig>) -> io::Result<Self> {
        let connection = ServerConnection::new(config)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        Ok(Self { stream, connection })
    }

    /// Authentication is bounded by a caller-owned deadline future. Dropping
    /// this future drops the transport; the package owns no background work.
    pub async fn handshake(
        stream: S,
        config: Arc<ServerConfig>,
        mut timeout: impl Future<Output = ()> + Unpin,
    ) -> io::Result<Self> {
        let mut tls = Self::new(stream, config)?;
        poll_fn(|context| {
            if Pin::new(&mut timeout).poll(context).is_ready() {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "TLS handshake deadline expired",
                )));
            }
            tls.poll_handshake()
        })
        .await?;
        Ok(tls)
    }

    pub fn alpn_protocol(&self) -> Option<&[u8]> {
        self.connection.alpn_protocol()
    }

    pub fn poll_flush(&mut self) -> Poll<io::Result<()>> {
        while self.connection.wants_write() {
            match self.connection.write_tls(&mut self.stream) {
                Ok(0) => return Poll::Ready(Err(io::ErrorKind::WriteZero.into())),
                Ok(_) => {}
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Poll::Pending,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Poll::Ready(Err(error)),
            }
        }
        Poll::Ready(Ok(()))
    }

    fn receive_tls(&mut self) -> Poll<io::Result<usize>> {
        loop {
            match self.connection.read_tls(&mut self.stream) {
                Ok(read) => {
                    self.connection
                        .process_new_packets()
                        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
                    return Poll::Ready(Ok(read));
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Poll::Pending,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Poll::Ready(Err(error)),
            }
        }
    }

    pub fn poll_handshake(&mut self) -> Poll<io::Result<()>> {
        loop {
            std::task::ready!(self.poll_flush())?;
            if !self.connection.is_handshaking() {
                return Poll::Ready(Ok(()));
            }
            if std::task::ready!(self.receive_tls())? == 0 {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "TLS peer closed during handshake",
                )));
            }
        }
    }

    pub fn poll_read(&mut self, buffer: &mut [u8]) -> Poll<io::Result<usize>> {
        if buffer.is_empty() {
            return Poll::Ready(Ok(0));
        }
        loop {
            match self.connection.reader().read(buffer) {
                Ok(read) => return Poll::Ready(Ok(read)),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Poll::Ready(Err(error)),
            }
            // Only rustls decides whether EOF was authenticated. Drain any
            // buffered plaintext before reporting a missing close_notify.
            std::task::ready!(self.receive_tls())?;
        }
    }

    pub fn poll_write(&mut self, buffer: &[u8]) -> Poll<io::Result<usize>> {
        if buffer.is_empty() {
            return Poll::Ready(Ok(0));
        }
        // Apply transport backpressure before accepting more plaintext.
        std::task::ready!(self.poll_flush())?;
        let written = match self.connection.writer().write(buffer)? {
            0 => return Poll::Ready(Err(io::ErrorKind::WriteZero.into())),
            written => written,
        };
        match self.poll_flush() {
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) | Poll::Pending => Poll::Ready(Ok(written)),
        }
    }

    pub fn poll_shutdown(&mut self) -> Poll<io::Result<()>>
    where
        S: ShutdownWrite,
    {
        self.connection.send_close_notify();
        std::task::ready!(self.poll_flush())?;
        Poll::Ready(self.stream.shutdown_write())
    }
}

fn nonblocking<T>(result: Poll<io::Result<T>>) -> io::Result<T> {
    match result {
        Poll::Ready(result) => result,
        Poll::Pending => Err(io::ErrorKind::WouldBlock.into()),
    }
}

impl<S: Read + Write> Read for TlsStream<S> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        nonblocking(self.poll_read(buffer))
    }
}

impl<S: Read + Write> Write for TlsStream<S> {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        nonblocking(self.poll_write(buffer))
    }

    fn flush(&mut self) -> io::Result<()> {
        nonblocking(self.poll_flush())
    }
}

#[cfg(test)]
#[path = "tls_stream_test.rs"]
mod tests;
