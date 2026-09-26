//! Nonblocking rustls transport driven by VM socket readiness.

use std::future::Future;
use std::io::{self, Read as _, Write as _};
use std::path::PathBuf;
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll};

use hyper::rt::{Read, ReadBufCursor, Write};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use rustls::{ServerConfig, ServerConnection};

use crate::runtime::vm::protocol_task_executor::{VmProtocolTaskFactory, VmReadyTcpStream};

use super::websocket_hub::WebSocketHub;

const PLAINTEXT_READ_CHUNK: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum VmTlsHttpProtocol {
    Http1,
    Http2,
}

pub(super) fn factory(
    web_root: PathBuf,
    server_config: Arc<ServerConfig>,
    max_body_bytes: u64,
    websocket_hub: Arc<WebSocketHub>,
) -> VmProtocolTaskFactory {
    let web_root = Arc::new(web_root);
    Arc::new(move |stream, route| {
        let web_root = super::owner_local_web_root(&web_root);
        let server_config = Arc::clone(&server_config);
        let websocket_hub = Arc::clone(&websocket_hub);
        Box::pin(async move {
            let io = VmTlsHyperIo::handshake(stream, server_config)
                .await
                .map_err(|error| {
                    format!(
                        "process {} scheduler {}: rustls handshake failed: {error}",
                        route.process.as_u64(),
                        route.scheduler.index()
                    )
                })?;
            match io.negotiated_protocol()? {
                VmTlsHttpProtocol::Http1 => {
                    let pending_upgrade = Rc::new(std::cell::RefCell::new(None));
                    let service_pending_upgrade = Rc::clone(&pending_upgrade);
                    let service = service_fn(move |request| {
                        let web_root = Rc::clone(&web_root);
                        let pending_upgrade = Rc::clone(&service_pending_upgrade);
                        async move {
                            Ok::<_, std::convert::Infallible>(
                                super::handle_request(
                                    request,
                                    web_root.as_ref().as_path(),
                                    max_body_bytes,
                                    Some(&pending_upgrade),
                                )
                                .await,
                            )
                        }
                    });
                    http1::Builder::new()
                        .serve_connection(io, service)
                        .with_upgrades()
                        .await
                        .map_err(|error| {
                            format!("Hyper HTTP/1.1 TLS connection failed: {error}")
                        })?;
                    let pending = pending_upgrade.borrow_mut().take();
                    if let Some(pending) = pending {
                        super::pump_hyper_websocket(pending, websocket_hub).await?;
                    }
                    Ok(())
                }
                VmTlsHttpProtocol::Http2 => {
                    let service = service_fn(move |request| {
                        let web_root = Rc::clone(&web_root);
                        async move {
                            Ok::<_, std::convert::Infallible>(
                                super::handle_request(
                                    request,
                                    web_root.as_ref().as_path(),
                                    max_body_bytes,
                                    None,
                                )
                                .await,
                            )
                        }
                    });
                    super::http2::serve_connection(io, service).await
                }
            }
        })
    })
}

/// One rustls connection whose socket remains registered on its VM owner.
pub(super) struct VmTlsHyperIo {
    stream: VmReadyTcpStream,
    connection: ServerConnection,
}

impl VmTlsHyperIo {
    pub(super) async fn handshake(
        stream: VmReadyTcpStream,
        config: Arc<ServerConfig>,
    ) -> io::Result<Self> {
        let connection = ServerConnection::new(config)
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        VmTlsHandshake {
            io: Some(Self { stream, connection }),
        }
        .await
    }

    pub(super) fn negotiated_protocol(&self) -> super::super::ServeResult<VmTlsHttpProtocol> {
        match self.connection.alpn_protocol() {
            Some(b"h2") => Ok(VmTlsHttpProtocol::Http2),
            Some(b"http/1.1") | None => Ok(VmTlsHttpProtocol::Http1),
            Some(protocol) => Err(format!(
                "error[serve_tls.alpn]: unsupported negotiated protocol `{}`",
                String::from_utf8_lossy(protocol)
            )
            .into()),
        }
    }

    fn flush_tls(&mut self) -> Poll<io::Result<()>> {
        while self.connection.wants_write() {
            match self.connection.write_tls(&mut self.stream) {
                Ok(0) => break,
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
}

impl Future for VmTlsHandshake {
    type Output = io::Result<VmTlsHyperIo>;

    fn poll(mut self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<Self::Output> {
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
                    // SAFETY: `read_plaintext` initialized `read` bytes and the
                    // destination cursor has at least `limit` bytes remaining.
                    unsafe {
                        std::ptr::copy_nonoverlapping(
                            plaintext.as_ptr(),
                            buffer.as_mut().as_mut_ptr().cast::<u8>(),
                            read,
                        );
                        buffer.advance(read);
                    }
                    return Poll::Ready(Ok(()));
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Poll::Ready(Err(error)),
            }
            match self.receive_tls() {
                Poll::Ready(Ok(0)) => return Poll::Ready(Ok(())),
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
        let written = match self.connection.writer().write(buffer) {
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

impl std::io::Read for VmTlsHyperIo {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        loop {
            match self.read_plaintext(buffer) {
                Ok(read) => return Ok(read),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
                Err(error) => return Err(error),
            }
            match self.receive_tls() {
                Poll::Ready(Ok(0)) => return Ok(0),
                Poll::Ready(Ok(_)) => {}
                Poll::Ready(Err(error)) => return Err(error),
                Poll::Pending => return Err(io::ErrorKind::WouldBlock.into()),
            }
        }
    }
}

impl std::io::Write for VmTlsHyperIo {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let written = self.connection.writer().write(buffer)?;
        match self.flush_tls() {
            Poll::Ready(Ok(())) | Poll::Pending => Ok(written),
            Poll::Ready(Err(error)) => Err(error),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.flush_tls() {
            Poll::Ready(result) => result,
            Poll::Pending => Err(io::ErrorKind::WouldBlock.into()),
        }
    }
}
