//! Production nonblocking rustls transport driven by VM socket readiness.

use std::future::Future;
use std::io::{self, Read as _, Write as _};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use hyper::rt::{Read, ReadBufCursor, Write};
use rustls::{ServerConfig, ServerConnection};

use crate::runtime::vm::protocol_task_executor::{
    protocol_sleep_until, VmProtocolSleep, VmReadyTcpStream,
};

const PLAINTEXT_READ_CHUNK: usize = 16 * 1024;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(30);

/// Maintained HTTP protocol selected by the authenticated TLS handshake.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmTlsHttpProtocol {
    /// HTTP/1.1, including peers that do not offer ALPN.
    Http1,
    /// HTTP/2 selected by the h2 ALPN identifier.
    Http2,
}

/// One rustls connection whose socket remains registered on its VM owner.
pub(crate) struct VmTlsHyperIo {
    stream: VmReadyTcpStream,
    connection: ServerConnection,
}

impl VmTlsHyperIo {
    /// Completes rustls authentication on the fixed VM protocol owner.
    pub(crate) async fn handshake(
        stream: VmReadyTcpStream,
        config: Arc<ServerConfig>,
    ) -> io::Result<Self> {
        Self::handshake_until(stream, config, Instant::now() + HANDSHAKE_TIMEOUT).await
    }

    /// Bounds authentication using the VM owner's timer, never a blocking timeout.
    pub(crate) async fn handshake_until(
        stream: VmReadyTcpStream,
        config: Arc<ServerConfig>,
        deadline: Instant,
    ) -> io::Result<Self> {
        let connection = ServerConnection::new(config)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        VmTlsHandshake {
            io: Some(Self { stream, connection }),
            timeout: protocol_sleep_until(deadline),
        }
        .await
    }

    /// Rejects unknown protocols rather than passing them to an HTTP parser.
    pub(crate) fn negotiated_protocol(&self) -> io::Result<VmTlsHttpProtocol> {
        match self.connection.alpn_protocol() {
            Some(b"h2") => Ok(VmTlsHttpProtocol::Http2),
            Some(b"http/1.1") | None => Ok(VmTlsHttpProtocol::Http1),
            Some(protocol) => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "error[serve_tls.alpn]: unsupported negotiated protocol `{}`",
                    String::from_utf8_lossy(protocol)
                ),
            )),
        }
    }

    fn flush_tls(&mut self) -> Poll<io::Result<()>> {
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
                    self.connection.process_new_packets().map_err(|error| {
                        io::Error::new(io::ErrorKind::InvalidData, error.to_string())
                    })?;
                    return Poll::Ready(Ok(read));
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => return Poll::Pending,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                Err(error) => return Poll::Ready(Err(error)),
            }
        }
    }

    fn poll_handshake(&mut self) -> Poll<io::Result<()>> {
        loop {
            if self.connection.wants_write() {
                match self.flush_tls() {
                    Poll::Ready(Ok(())) => {}
                    Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                    Poll::Pending => return Poll::Pending,
                }
            }
            if !self.connection.is_handshaking() {
                return Poll::Ready(Ok(()));
            }
            match self.receive_tls() {
                Poll::Ready(Ok(0)) => {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "TLS peer closed during handshake",
                    )))
                }
                Poll::Ready(Ok(_)) => {}
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Pending => return Poll::Pending,
            }
        }
    }

    fn read_plaintext(&mut self, destination: &mut [u8]) -> io::Result<usize> {
        self.connection.reader().read(destination)
    }
}

struct VmTlsHandshake {
    io: Option<VmTlsHyperIo>,
    timeout: VmProtocolSleep,
}

impl Future for VmTlsHandshake {
    type Output = io::Result<VmTlsHyperIo>;

    fn poll(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        if Pin::new(&mut self.timeout).poll(context).is_ready() {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "TLS handshake deadline expired",
            )));
        }
        let io = self
            .io
            .as_mut()
            .expect("TLS handshake polled after completion");
        match io.poll_handshake() {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(self.io.take().unwrap())),
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl Read for VmTlsHyperIo {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        mut buffer: ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        if buffer.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        let mut plaintext = [0_u8; PLAINTEXT_READ_CHUNK];
        loop {
            let limit = plaintext.len().min(buffer.remaining());
            match self.read_plaintext(&mut plaintext[..limit]) {
                Ok(read) => {
                    buffer.put_slice(&plaintext[..read]);
                    return Poll::Ready(Ok(()));
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Poll::Ready(Err(error)),
            }
            match self.receive_tls() {
                // rustls, not raw TCP EOF, decides whether close_notify was received.
                // Its reader drains buffered plaintext before reporting truncation.
                Poll::Ready(Ok(_)) => {}
                Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl Write for VmTlsHyperIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        if buffer.is_empty() {
            return Poll::Ready(Ok(0));
        }
        // Drain buffered records before accepting more plaintext. A full rustls
        // buffer otherwise looks like a successful zero-byte write to Hyper.
        match self.flush_tls() {
            Poll::Ready(Ok(())) => {}
            Poll::Ready(Err(error)) => return Poll::Ready(Err(error)),
            Poll::Pending => return Poll::Pending,
        }
        let written = match self.connection.writer().write(buffer) {
            Ok(0) => return Poll::Ready(Err(io::ErrorKind::WriteZero.into())),
            Ok(written) => written,
            Err(error) => return Poll::Ready(Err(error)),
        };
        match self.flush_tls() {
            Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
            Poll::Ready(Ok(())) | Poll::Pending => Poll::Ready(Ok(written)),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.flush_tls()
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.connection.send_close_notify();
        match self.flush_tls() {
            Poll::Ready(Ok(())) => Poll::Ready(self.stream.shutdown_write()),
            outcome => outcome,
        }
    }

    fn is_write_vectored(&self) -> bool {
        false
    }
}
