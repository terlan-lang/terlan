//! Maintained Hyper HTTP/1 protocol ownership over VM socket tasks.

use std::cell::RefCell;
use std::convert::Infallible;
use std::net as std_net;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use bytes::Bytes;
use http_body_util::Full;
use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response};
use terlan_http_native::request_body::{
    collect_bounded_body, declared_body_exceeds_limit, spool_bounded_body_to_root, BodyReadError,
    TemporaryBodyFile,
};
use terlan_http_native::websocket::{output, ErrorKind, Message, Server as WebSocket};

use super::handle_vm_stream_request;
use super::handler::VmHttpChannelTransport;
use super::server_lifecycle::{
    handle_suspendable_vm_stream_request, request_requires_file_body, RequestBodyFilePath,
};
#[cfg(test)]
use super::{channel_transport, handle_vm_stream_http1_exchange};
use crate::runtime::vm::protocol_task_executor::{
    protocol_sleep_until, serve_protocol_tasks, VmProtocolTaskFactory, VmReadyStream,
};
use crate::runtime::vm::ReplValue;

mod http2;
mod tls_io;
use terlan_http_native::response_body::ResponseBody;
mod websocket_hub;
use websocket_hub::WebSocketHub;

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
    I: hyper::rt::Read + hyper::rt::Write + Unpin + Send + 'static,
{
    let pending_upgrade = Rc::new(RefCell::new(None));
    let service_pending_upgrade = Rc::clone(&pending_upgrade);
    let service = service_fn(move |request| {
        let web_root = Rc::clone(&web_root);
        let pending_upgrade = Rc::clone(&service_pending_upgrade);
        async move {
            Ok::<_, Infallible>(
                handle_request(
                    request,
                    web_root.as_path(),
                    max_body_bytes,
                    Some(&pending_upgrade),
                )
                .await,
            )
        }
    });
    terlan_http_native::http1::serve_connection(io, service)
        .await
        .map_err(failure_context)?;
    let pending = pending_upgrade.borrow_mut().take();
    if let Some(pending) = pending {
        pump_hyper_websocket(pending, websocket_hub).await?;
    }
    Ok(())
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

async fn spool_bounded_body<B>(
    body: B,
    max_body_bytes: u64,
) -> Result<TemporaryBodyFile, BodyReadError>
where
    B: hyper::body::Body<Data = Bytes> + Unpin,
    B::Error: std::fmt::Display,
{
    let configured_root = std::env::var("TERLAN_SERVE_UPLOAD_ROOT").map_err(|_| {
        BodyReadError::Unavailable(
            "TERLAN_SERVE_UPLOAD_ROOT is required for file-backed request bodies".into(),
        )
    })?;
    spool_bounded_body_to_root(body, max_body_bytes, Path::new(&configured_root)).await
}

/// Package-owned Hyper adaptation; the VM supplies only registered streams.
type HyperVmIo = terlan_http_native::plain_io::PlainIo<VmReadyStream>;

async fn handle_request(
    mut request: Request<Incoming>,
    web_root: &Path,
    max_body_bytes: u64,
    upgrade_slot: Option<&PendingHyperUpgradeSlot>,
) -> Response<ResponseBody> {
    if declared_body_exceeds_limit(request.headers(), max_body_bytes) {
        return error_response(413, format!("request body exceeds {max_body_bytes} bytes"));
    }
    let file_backed =
        match request_requires_file_body(web_root, request.method().as_str(), request.uri().path())
        {
            Ok(file_backed) => file_backed,
            Err(error) => return error_response(500, error),
        };
    let on_upgrade = upgrade_slot.map(|_| hyper::upgrade::on(&mut request));
    let (parts, body) = request.into_parts();
    let (body, temporary) = if file_backed {
        match spool_bounded_body(body, max_body_bytes).await {
            Ok(temporary) => (String::new(), Some(temporary)),
            Err(BodyReadError::TooLarge) => {
                return error_response(413, format!("request body exceeds {max_body_bytes} bytes"))
            }
            Err(BodyReadError::Invalid(error)) => {
                return error_response(400, format!("invalid request body: {error}"))
            }
            Err(BodyReadError::Unavailable(error)) => return error_response(503, error),
        }
    } else {
        match collect_bounded_body(body, max_body_bytes).await {
            Ok(body) => match String::from_utf8(body) {
                Ok(body) => (body, None),
                Err(error) => return error_response(400, format!("invalid UTF-8 body: {error}")),
            },
            Err(BodyReadError::TooLarge) => {
                return error_response(413, format!("request body exceeds {max_body_bytes} bytes"))
            }
            Err(BodyReadError::Invalid(error)) => {
                return error_response(400, format!("invalid request body: {error}"))
            }
            Err(BodyReadError::Unavailable(error)) => return error_response(503, error),
        }
    };
    let mut request = Request::from_parts(parts, body);
    if let Some(temporary) = &temporary {
        let Some(path) = temporary.path().to_str() else {
            return error_response(503, "temporary request path is not UTF-8".into());
        };
        request
            .extensions_mut()
            .insert(RequestBodyFilePath(path.to_string()));
    }
    match handle_suspendable_vm_stream_request(&request, web_root).await {
        Ok(Some(response)) => {
            return ResponseBody::from_response(response);
        }
        Ok(None) => {}
        Err(error) => return error_response(500, error),
    }
    let channel_route = request.uri().path().to_string();
    let channel_request_target = request
        .uri()
        .path_and_query()
        .map(|target| target.as_str().to_string())
        .unwrap_or_else(|| channel_route.clone());
    let mut channel = None;
    let response = match handle_vm_stream_request(request, web_root, &mut channel, false) {
        Ok(response) => response,
        Err(error) => return error_response(500, error),
    };
    if let Some(channel) = channel {
        let channel = match channel {
            VmHttpChannelTransport::WebSocket(session) => {
                let Some(slot) = upgrade_slot else {
                    drop(session);
                    return error_response(
                        501,
                        "error[serve_http.upgrade_adapter_missing]: maintained async Hyper adapter is required for WebSocket"
                            .to_string(),
                    );
                };
                let Some(on_upgrade) = on_upgrade else {
                    drop(session);
                    return error_response(
                        500,
                        "error[serve_http.upgrade_state]: Hyper upgrade future was not retained"
                            .to_string(),
                    );
                };
                let mut slot = slot.borrow_mut();
                if slot.is_some() {
                    drop(session);
                    return error_response(
                        500,
                        "error[serve_http.upgrade_state]: connection already owns an upgrade"
                            .to_string(),
                    );
                }
                *slot = Some(PendingHyperUpgrade {
                    on_upgrade,
                    session,
                    route: channel_route,
                    request_target: channel_request_target,
                });
                return ResponseBody::from_response(response);
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
    ResponseBody::from_response(response)
}

type PendingHyperUpgradeSlot = Rc<RefCell<Option<PendingHyperUpgrade>>>;

struct PendingHyperUpgrade {
    on_upgrade: hyper::upgrade::OnUpgrade,
    session: super::handler::AotWebSocketCallbackSession,
    route: String,
    request_target: String,
}

type HyperWebSocketIo = terlan_http_native::upgrade_io::UpgradeIo<HyperVmIo, tls_io::HttpTlsIo>;

async fn pump_hyper_websocket(
    mut pending: PendingHyperUpgrade,
    hub: Arc<WebSocketHub>,
) -> Result<(), String> {
    let upgraded = pending.on_upgrade.await.map_err(|error| {
        format!("error[serve.websocket.upgrade]: Hyper upgrade failed: {error}")
    })?;
    let io = HyperWebSocketIo::from_upgraded(upgraded)?;
    let mut socket = WebSocket::new(io, pending.session.plan().max_frame_bytes());
    let mut pairing = pending.session.plan().pairing().cloned();
    let mut identity = None;
    if let Some(pairing) = &mut pairing {
        if pairing.restoration.is_some() {
            identity = pending
                .session
                .dispatch_pair_identity_output(pending.request_target.clone())
                .await?;
            pairing.waiting = pending.session.dispatch_pair_waiting_output()?;
            pairing.peer_left = pending.session.dispatch_pair_peer_left_output()?;
        }
    }
    let mut lease = pairing
        .as_ref()
        .map(|pairing| {
            hub.join(
                pending.route.clone(),
                pending.request_target.clone(),
                pending.session.inspect().max_pending_frames,
                pairing,
                identity,
            )
        })
        .transpose()?;
    if let Some(lease) = &mut lease {
        lease.dispatch_admission(&mut pending.session)?;
    }
    notify_websocket_writable(&mut pending.session)?;

    loop {
        if let Some(lease) = &lease {
            if !output::drain(&mut socket, &lease.outbound, websocket_transport_wait).await? {
                return pending.session.close().map(|_| ());
            }
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                let result = pending.session.enqueue_inbound(text);
                match result {
                    Ok(()) => {
                        if let Some(lease) = &lease {
                            if pairing.as_ref().is_some_and(|pairing| pairing.stateful) {
                                websocket_hub::drain_stateful_inbound(lease, &mut pending.session)?;
                            } else {
                                for payload in drain_websocket_inbound(&mut pending.session)? {
                                    lease.broadcast(payload)?;
                                }
                            }
                        } else {
                            let _ = drain_websocket_inbound(&mut pending.session)?;
                        }
                    }
                    Err(error) => {
                        let _ = pending.session.cancel(error.clone());
                        return Err(error);
                    }
                }
            }
            Ok(Message::Ping(_)) => {
                if !output::flush(&mut socket, websocket_transport_wait).await? {
                    return pending.session.close().map(|_| ());
                }
                notify_websocket_writable(&mut pending.session)?;
            }
            Ok(Message::Pong(_)) => {}
            Ok(Message::Close(_)) => {
                let callback = pending.session.close().map(|_| ());
                let flush = output::flush(&mut socket, websocket_transport_wait)
                    .await
                    .map(|_| ());
                return callback.and(flush);
            }
            Ok(Message::Binary(_)) => {
                let error =
                    "error[serve.websocket.binary]: endpoint rejects binary payloads".to_string();
                let _ = pending.session.cancel(error.clone());
                return Err(error);
            }
            Ok(Message::Frame(_)) => {
                let error = "error[serve.websocket.frame]: raw frame escaped maintained decoding"
                    .to_string();
                let _ = pending.session.cancel(error.clone());
                return Err(error);
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                websocket_transport_wait().await;
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => {
                websocket_transport_wait().await;
            }
            Err(error) if error.kind() == ErrorKind::Disconnected => {
                return pending.session.close().map(|_| ());
            }
            Err(error) if error.kind() == ErrorKind::Closed => {
                return pending.session.close().map(|_| ());
            }
            Err(error) => {
                let reason = format!("error[serve.websocket.transport]: {error}");
                pending.session.cancel(reason.clone()).map(|_| ())?;
                return Err(reason);
            }
        }
    }
}

fn drain_websocket_inbound(
    session: &mut super::handler::AotWebSocketCallbackSession,
) -> Result<Vec<String>, String> {
    let mut payloads = Vec::new();
    loop {
        let (dispatched, output) = session.dispatch_next_inbound_output()?;
        if !dispatched {
            return Ok(payloads);
        }
        match output {
            Some(ReplValue::String(payload)) => payloads.push(payload),
            Some(ReplValue::Unit) | None => {}
            Some(value) => {
                return Err(format!(
                    "error[serve.websocket.callback_result]: inbound callback returned {value:?}, expected String or Unit"
                ))
            }
        }
    }
}

fn notify_websocket_writable(
    session: &mut super::handler::AotWebSocketCallbackSession,
) -> Result<(), String> {
    if !session.is_waiting() {
        session.writable()?;
    }
    Ok(())
}

async fn websocket_transport_wait() {
    protocol_sleep_until(Instant::now() + Duration::from_millis(2)).await;
}

fn error_response(status: u16, message: String) -> Response<ResponseBody> {
    Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(ResponseBody::Buffered(Full::new(Bytes::from(message))))
        .unwrap_or_else(|_| {
            Response::new(ResponseBody::Buffered(Full::new(Bytes::from_static(
                b"HTTP service error",
            ))))
        })
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
