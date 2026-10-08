//! Maintained Hyper HTTP/1 protocol ownership over VM socket tasks.

use std::cell::RefCell;
use std::net as std_net;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response};
use terlan_http_native::request_pipeline::{self, Application};
use terlan_http_native::websocket::upgrade::UpgradeSlot;

use super::handle_vm_stream_request;
use super::handler::VmHttpChannelTransport;
use super::server_lifecycle::{handle_suspendable_vm_stream_request, request_requires_file_body};
#[cfg(test)]
use super::{channel_transport, handle_vm_stream_http1_exchange};
use crate::runtime::vm::protocol_task_executor::{
    protocol_sleep_until, serve_protocol_tasks, VmProtocolTaskFactory, VmReadyStream,
};

mod http2;
mod tls_io;
use terlan_http_native::response_body::ResponseBody;
use terlan_http_native::websocket::hub::WebSocketHub;

thread_local! {
    /// Immutable route root copied once onto each permanent protocol owner.
    static LOCAL_WEB_ROOT: RefCell<Option<Rc<PathBuf>>> = const { RefCell::new(None) };
}

/// Runs Hyper connection futures on the protocol-agnostic VM task executor.
pub(super) fn serve(
    listener: std_net::TcpListener,
    web_root: PathBuf,
    max_body_bytes: u64,
) -> Result<(), String> {
    let maintenance =
        super::handler_cache::http_session_maintenance_for(&web_root).map_err(String::from)?;
    let web_root = Arc::new(web_root);
    let websocket_hub = Arc::new(WebSocketHub::default());
    let factory: VmProtocolTaskFactory = Arc::new(move |stream, route| {
        let web_root = owner_local_web_root(&web_root);
        let websocket_hub = Arc::clone(&websocket_hub);
        Box::pin(async move {
            serve_http1(
                HyperVmIo::new(stream),
                web_root,
                max_body_bytes,
                websocket_hub,
                |error| {
                    format!(
                        "process {} scheduler {}: Hyper HTTP/1 connection failed: {error}",
                        route.process.as_u64(),
                        route.scheduler.index()
                    )
                },
            )
            .await
        })
    });
    let incoming = terlan_net_native::tcp::TcpIncoming::new(listener)
        .map_err(|error| format!("error[serve.tcp]: {error}"))?;
    serve_protocol_tasks(Box::new(incoming), factory, maintenance)
}

/// Runs rustls and ALPN-selected Hyper protocol futures on VM protocol owners.
pub(super) fn serve_tls(
    listener: std_net::TcpListener,
    web_root: PathBuf,
    server_config: Arc<rustls::ServerConfig>,
    max_body_bytes: u64,
) -> Result<(), String> {
    let maintenance =
        super::handler_cache::http_session_maintenance_for(&web_root).map_err(String::from)?;
    let websocket_hub = Arc::new(WebSocketHub::default());
    let incoming = terlan_net_native::tcp::TcpIncoming::new(listener)
        .map_err(|error| format!("error[serve.tcp]: {error}"))?;
    serve_protocol_tasks(
        Box::new(incoming),
        tls_factory(web_root, server_config, max_body_bytes, websocket_hub),
        maintenance,
    )
}

fn tls_factory(
    web_root: PathBuf,
    server_config: Arc<rustls::ServerConfig>,
    max_body_bytes: u64,
    websocket_hub: Arc<WebSocketHub>,
) -> VmProtocolTaskFactory {
    let web_root = Arc::new(web_root);
    Arc::new(move |stream, route| {
        let web_root = owner_local_web_root(&web_root);
        let server_config = Arc::clone(&server_config);
        let websocket_hub = Arc::clone(&websocket_hub);
        Box::pin(async move {
            let io = tls_io::handshake(stream, server_config)
                .await
                .map_err(|error| {
                    format!(
                        "process {} scheduler {}: rustls handshake failed: {error}",
                        route.process.as_u64(),
                        route.scheduler.index()
                    )
                })?;
            match io
                .negotiated_protocol()
                .map_err(|error| error.to_string())?
            {
                tls_io::HttpProtocol::Http1 => {
                    serve_http1(io, web_root, max_body_bytes, websocket_hub, |error| {
                        format!("Hyper HTTP/1.1 TLS connection failed: {error}")
                    })
                    .await
                }
                tls_io::HttpProtocol::Http2 => {
                    let service = service_fn(move |request| {
                        let web_root = Rc::clone(&web_root);
                        async move {
                            Ok::<_, std::convert::Infallible>(
                                handle_request(
                                    request,
                                    web_root.as_ref().as_path(),
                                    max_body_bytes,
                                    None,
                                )
                                .await,
                            )
                        }
                    });
                    http2::serve_connection(io, service).await
                }
            }
        })
    })
}

async fn serve_http1<I>(
    io: I,
    web_root: Rc<PathBuf>,
    max_body_bytes: u64,
    websocket_hub: Arc<WebSocketHub>,
    failure_context: impl FnOnce(hyper::Error) -> String,
) -> Result<(), String>
where
    I: hyper::rt::Read + hyper::rt::Write + std::io::Read + std::io::Write + Unpin + Send + 'static,
{
    terlan_http_native::server_connection::serve_http1(
        io,
        move |request, pending_upgrade| {
            let web_root = Rc::clone(&web_root);
            async move {
                handle_request(
                    request,
                    web_root.as_path(),
                    max_body_bytes,
                    Some(&pending_upgrade),
                )
                .await
            }
        },
        &websocket_hub,
        websocket_transport_wait,
        failure_context,
    )
    .await
    .map_err(String::from)
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

/// Package-owned Hyper adaptation; the VM supplies only registered streams.
type HyperVmIo = terlan_http_native::plain_io::PlainIo<VmReadyStream>;

struct CompiledApplication<'a>(&'a Path);

impl Application for CompiledApplication<'_> {
    type WebSocket = super::handler::AotWebSocketCallbackSession;
    type Sse = super::handler::AotSseCallbackSession;

    fn requires_file_body(&self, method: &str, path: &str) -> Result<bool, String> {
        request_requires_file_body(self.0, method, path)
    }

    async fn handle_suspendable(
        &self,
        request: &Request<String>,
    ) -> Result<Option<Response<Bytes>>, String> {
        handle_suspendable_vm_stream_request(request, self.0).await
    }

    fn handle(
        &self,
        request: Request<String>,
        channel: &mut Option<VmHttpChannelTransport>,
    ) -> Result<Response<Bytes>, String> {
        handle_vm_stream_request(request, self.0, channel, false)
    }
}

async fn handle_request(
    request: Request<Incoming>,
    web_root: &Path,
    max_body_bytes: u64,
    upgrade_slot: Option<&PendingHyperUpgradeSlot>,
) -> Response<ResponseBody> {
    let upload_root = std::env::var("TERLAN_SERVE_UPLOAD_ROOT")
        .ok()
        .map(PathBuf::from);
    request_pipeline::handle(
        &CompiledApplication(web_root),
        request,
        max_body_bytes,
        upload_root.as_deref(),
        upgrade_slot,
    )
    .await
}

type PendingHyperUpgradeSlot = UpgradeSlot<super::handler::AotWebSocketCallbackSession>;

async fn websocket_transport_wait() {
    protocol_sleep_until(Instant::now() + Duration::from_millis(2)).await;
}

#[cfg(test)]
#[path = "hyper_server_test.rs"]
#[cfg(test)]
pub(super) mod hyper_server_test;

/// Serves one blocking stream through the VM HTTP/1 adapter.
/// Inputs:
/// - `stream`: readable and writable byte stream.
/// - `web_root`: generated browser package root.
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

/// Returns whether buffered bytes contain one validated HTTP/1 request.
#[cfg(test)]
pub(in crate::commands::serve) fn vm_plain_http1_request_complete(
    bytes: &[u8],
) -> Result<bool, terlan_http_native::http1::RequestReadFailure> {
    terlan_http_native::http1::try_parse_http1_request_buffer(bytes)
        .map(|request| request.is_some())
}
