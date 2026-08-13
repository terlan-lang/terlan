//! Maintained Hyper HTTP/1 protocol ownership over VM socket tasks.

use std::cell::RefCell;
use std::convert::Infallible;
use std::io::{self, IoSlice, Write as _};
use std::net as std_net;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;
use std::task::{Context, Poll};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::rt::{Read, ReadBufCursor, Write};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};

use crate::runtime::vm::protocol_task_executor::{
    serve_protocol_tasks, VmProtocolTaskFactory, VmReadyTcpStream,
};

use super::handler::VmHttpChannelTransport;
#[cfg(test)]
use super::{channel_transport, handle_vm_stream_http1_exchange};
use super::{handle_suspendable_vm_stream_request, handle_vm_stream_request};

mod http2;
mod tls_io;

thread_local! {
    /// Immutable route root copied once onto each permanent protocol owner.
    static LOCAL_WEB_ROOT: RefCell<Option<Rc<PathBuf>>> = const { RefCell::new(None) };
}

/// Runs Hyper connection futures on the protocol-agnostic VM task executor.
pub(super) fn serve(listener: std_net::TcpListener, web_root: PathBuf) -> Result<(), String> {
    let web_root = Arc::new(web_root);
    let factory: VmProtocolTaskFactory = Arc::new(move |stream, route| {
        let web_root = owner_local_web_root(&web_root);
        let service = service_fn(move |request| {
            let web_root = Rc::clone(&web_root);
            async move {
                Ok::<_, Infallible>(handle_request(request, web_root.as_ref().as_path()).await)
            }
        });
        let connection = http1::Builder::new().serve_connection(HyperVmIo::new(stream), service);
        Box::pin(async move {
            connection.await.map_err(|error| {
                format!(
                    "process {} scheduler {}: Hyper HTTP/1 connection failed: {error}",
                    route.process.as_u64(),
                    route.scheduler.index()
                )
            })
        })
    });
    serve_protocol_tasks(listener, factory)
}

/// Runs rustls and ALPN-selected Hyper protocol futures on VM protocol owners.
pub(super) fn serve_tls(
    listener: std_net::TcpListener,
    web_root: PathBuf,
    server_config: Arc<rustls::ServerConfig>,
) -> Result<(), String> {
    serve_protocol_tasks(listener, tls_factory(web_root, server_config))
}

fn tls_factory(
    web_root: PathBuf,
    server_config: Arc<rustls::ServerConfig>,
) -> VmProtocolTaskFactory {
    let web_root = Arc::new(web_root);
    Arc::new(move |stream, route| {
        let web_root = owner_local_web_root(&web_root);
        let server_config = Arc::clone(&server_config);
        Box::pin(async move {
            let io = tls_io::VmTlsHyperIo::handshake(stream, server_config)
                .await
                .map_err(|error| {
                    format!(
                        "process {} scheduler {}: rustls handshake failed: {error}",
                        route.process.as_u64(),
                        route.scheduler.index()
                    )
                })?;
            let protocol = io.negotiated_protocol()?;
            let service = service_fn(move |request| {
                let web_root = Rc::clone(&web_root);
                async move {
                    Ok::<_, Infallible>(handle_request(request, web_root.as_ref().as_path()).await)
                }
            });
            match protocol {
                tls_io::VmTlsHttpProtocol::Http1 => http1::Builder::new()
                    .serve_connection(io, service)
                    .await
                    .map_err(|error| format!("Hyper HTTP/1.1 TLS connection failed: {error}")),
                tls_io::VmTlsHttpProtocol::Http2 => http2::serve_connection(io, service).await,
            }
        })
    })
}

fn owner_local_web_root(shared: &Arc<PathBuf>) -> Rc<PathBuf> {
    LOCAL_WEB_ROOT.with(|local| {
        let mut local = local.borrow_mut();
        if local
            .as_ref()
            .is_none_or(|root| root.as_path() != shared.as_path())
        {
            *local = Some(Rc::new(shared.as_ref().clone()));
        }
        Rc::clone(
            local
                .as_ref()
                .expect("owner-local web root is initialized before cloning"),
        )
    })
}

/// Hyper I/O facade; readiness and polling remain owned by the VM executor.
struct HyperVmIo {
    stream: VmReadyTcpStream,
}

impl HyperVmIo {
    fn new(stream: VmReadyTcpStream) -> Self {
        Self { stream }
    }
}

impl Read for HyperVmIo {
    fn poll_read(
        self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        mut buffer: ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        if buffer.remaining() == 0 {
            return Poll::Ready(Ok(()));
        }
        loop {
            // SAFETY: `SockRef::recv` accepts `MaybeUninit<u8>` directly and
            // initializes exactly the returned byte count. Advancing by that
            // count therefore satisfies Hyper's ReadBufCursor contract.
            let outcome = unsafe { self.stream.read_uninit(buffer.as_mut()) };
            match outcome {
                Ok(read) => {
                    // SAFETY: the receive above initialized exactly `read`
                    // bytes in the cursor's currently unfilled region.
                    unsafe { buffer.advance(read) };
                    return Poll::Ready(Ok(()));
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    return Poll::Pending;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Poll::Ready(Err(error)),
            }
        }
    }
}

impl Write for HyperVmIo {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        loop {
            match self.stream.write(buffer) {
                Ok(written) => return Poll::Ready(Ok(written)),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    return Poll::Pending;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Poll::Ready(Err(error)),
            }
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(self.stream.shutdown_write())
    }

    fn is_write_vectored(&self) -> bool {
        true
    }

    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        _context: &mut Context<'_>,
        buffers: &[IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        loop {
            match self.stream.write_vectored(buffers) {
                Ok(written) => return Poll::Ready(Ok(written)),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    return Poll::Pending;
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Poll::Ready(Err(error)),
            }
        }
    }
}

async fn handle_request(request: Request<Incoming>, web_root: &Path) -> Response<Full<Bytes>> {
    let (parts, body) = request.into_parts();
    let body = match body.collect().await {
        // `Bytes` can transfer a uniquely owned Hyper receive allocation into
        // the Vec without copying. The generated Request still receives its
        // required String, but the common one-frame body no longer pays for a
        // second buffer and memcpy at the protocol/runtime boundary.
        Ok(body) => match String::from_utf8(Vec::from(body.to_bytes())) {
            Ok(body) => body,
            Err(error) => return error_response(400, format!("invalid UTF-8 body: {error}")),
        },
        Err(error) => return error_response(400, format!("invalid request body: {error}")),
    };
    let request = Request::from_parts(parts, body);
    match handle_suspendable_vm_stream_request(&request, web_root).await {
        Ok(Some(response)) => {
            let (parts, body) = response.into_parts();
            return Response::from_parts(parts, Full::new(body));
        }
        Ok(None) => {}
        Err(error) => return error_response(500, error),
    }
    let mut channel = None;
    let response = match handle_vm_stream_request(request, web_root, &mut channel) {
        Ok(response) => response,
        Err(error) => return error_response(500, error),
    };
    if let Some(channel) = channel {
        let channel = match channel {
            VmHttpChannelTransport::WebSocket(session) => {
                drop(session);
                "WebSocket"
            }
            VmHttpChannelTransport::Sse(session) => {
                drop(session);
                "SSE"
            }
        };
        return error_response(
            501,
            format!(
                "error[serve_http.upgrade_adapter_missing]: maintained async Hyper adapter is required for {channel}"
            ),
        );
    }
    let (parts, body) = response.into_parts();
    Response::from_parts(parts, Full::new(body))
}

fn error_response(status: u16, message: String) -> Response<Full<Bytes>> {
    Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Full::new(Bytes::from(message)))
        .unwrap_or_else(|_| Response::new(Full::new(Bytes::from_static(b"HTTP service error"))))
}

#[cfg(test)]
#[path = "hyper_server_test.rs"]
#[cfg(test)]
mod hyper_server_test;

/// Serves one blocking stream through the VM HTTP/1 adapter.
///
/// Inputs:
/// - `stream`: readable and writable byte stream.
/// - `web_root`: generated browser package root.
///
/// Output:
/// - Success after one HTTP response is written.
///
/// Transformation:
/// - Reads exactly one HTTP/1 request with `httparse` header validation, routes
///   it through the VM stream adapter, and writes the serialized response.
#[cfg(test)]
pub(in crate::commands::serve) fn serve_vm_plain_http1_connection<S>(
    stream: &mut S,
    web_root: &Path,
) -> Result<(), String>
where
    S: std::io::Read + std::io::Write,
{
    let request = read_vm_plain_http1_request(stream)?;
    let exchange = handle_vm_stream_http1_exchange(web_root, &request)?;
    channel_transport::serve_vm_stream_http1_exchange(stream, exchange)
}

/// Reads one complete HTTP/1 request from a blocking stream.
///
/// Inputs:
/// - `stream`: readable byte stream.
///
/// Output:
/// - Raw request bytes containing headers and the declared body.
///
/// Transformation:
/// - Uses `httparse` to detect header completion and content-length, keeping
///   protocol parsing in a maintained crate before VM HTTP validation runs.
#[cfg(test)]
pub(in crate::commands::serve) fn read_vm_plain_http1_request<S>(
    stream: &mut S,
) -> Result<Vec<u8>, String>
where
    S: std::io::Read,
{
    let mut request = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let read = std::io::Read::read(stream, &mut chunk)
            .map_err(|err| format!("failed to read VM plain HTTP request: {err}"))?;
        if read == 0 {
            if request.is_empty() {
                return Err("empty VM plain HTTP request".to_string());
            }
            return Ok(request);
        }
        request.extend_from_slice(&chunk[..read]);
        if request.len() > 1024 * 1024 {
            return Err("VM plain HTTP request exceeds 1 MiB".to_string());
        }
        if vm_plain_http1_request_complete(&request).map_err(|error| error.to_string())? {
            return Ok(request);
        }
    }
}

#[cfg(test)]
#[derive(Debug, Eq, PartialEq)]
pub(in crate::commands::serve) enum PlainHttp1CompletenessError {
    Parse(httparse::Error),
    InvalidContentLengthEncoding,
    InvalidContentLengthValue,
}

#[cfg(test)]
impl std::fmt::Display for PlainHttp1CompletenessError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(error) => write!(formatter, "invalid VM plain HTTP request: {error}"),
            Self::InvalidContentLengthEncoding => {
                formatter.write_str("invalid VM plain HTTP content-length header")
            }
            Self::InvalidContentLengthValue => {
                formatter.write_str("invalid VM plain HTTP content-length value")
            }
        }
    }
}

/// Returns whether buffered bytes contain one complete HTTP/1 request.
#[cfg(test)]
pub(in crate::commands::serve) fn vm_plain_http1_request_complete(
    bytes: &[u8],
) -> Result<bool, PlainHttp1CompletenessError> {
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut request = httparse::Request::new(&mut headers);
    let header_length = match request
        .parse(bytes)
        .map_err(PlainHttp1CompletenessError::Parse)?
    {
        httparse::Status::Complete(length) => length,
        httparse::Status::Partial => return Ok(false),
    };
    let content_length = request
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("content-length"))
        .map(|header| {
            std::str::from_utf8(header.value)
                .map_err(|_| PlainHttp1CompletenessError::InvalidContentLengthEncoding)?
                .trim()
                .parse::<usize>()
                .map_err(|_| PlainHttp1CompletenessError::InvalidContentLengthValue)
        })
        .transpose()?
        .unwrap_or(0);
    Ok(bytes.len() >= header_length.saturating_add(content_length))
}
