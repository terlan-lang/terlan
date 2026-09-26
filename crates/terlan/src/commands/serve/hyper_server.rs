//! Maintained Hyper HTTP/1 protocol ownership over VM socket tasks.

use std::cell::RefCell;
use std::convert::Infallible;
use std::io::{self, IoSlice, Write as _};
use std::net as std_net;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::body::Incoming;
use hyper::rt::{Read, ReadBufCursor, Write};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response};
use tungstenite::protocol::{Message, Role, WebSocket};
use tungstenite::Error as WebSocketError;

use crate::runtime::vm::protocol_task_executor::{
    protocol_sleep_until, serve_protocol_tasks, VmProtocolTaskFactory, VmReadyTcpStream,
};
use crate::runtime::vm::websocket::VmWebSocketFrame;
#[cfg(test)]
use crate::runtime::vm::websocket::VmWebSocketPairingPlan;
use crate::runtime::vm::ReplValue;

use super::handle_vm_stream_request;
use super::handler::VmHttpChannelTransport;
use super::server_lifecycle::{
    handle_suspendable_vm_stream_request, request_requires_file_body, RequestBodyFilePath,
};
#[cfg(test)]
use super::{channel_transport, handle_vm_stream_http1_exchange};

mod http2;
mod tls_io;
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
    let web_root = Arc::new(web_root);
    let websocket_hub = Arc::new(WebSocketHub::default());
    let factory: VmProtocolTaskFactory = Arc::new(move |stream, route| {
        let web_root = owner_local_web_root(&web_root);
        let websocket_hub = Arc::clone(&websocket_hub);
        let pending_upgrade = Rc::new(RefCell::new(None));
        let service_pending_upgrade = Rc::clone(&pending_upgrade);
        let service = service_fn(move |request| {
            let web_root = Rc::clone(&web_root);
            let pending_upgrade = Rc::clone(&service_pending_upgrade);
            async move {
                Ok::<_, Infallible>(
                    handle_request(
                        request,
                        web_root.as_ref().as_path(),
                        max_body_bytes,
                        Some(&pending_upgrade),
                    )
                    .await,
                )
            }
        });
        let connection = http1::Builder::new()
            .serve_connection(HyperVmIo::new(stream), service)
            .with_upgrades();
        Box::pin(async move {
            connection.await.map_err(|error| {
                format!(
                    "process {} scheduler {}: Hyper HTTP/1 connection failed: {error}",
                    route.process.as_u64(),
                    route.scheduler.index()
                )
            })?;
            let pending = pending_upgrade.borrow_mut().take();
            if let Some(pending) = pending {
                pump_hyper_websocket(pending, websocket_hub).await?;
            }
            Ok(())
        })
    });
    serve_protocol_tasks(listener, factory)
}

/// Runs rustls and ALPN-selected Hyper protocol futures on VM protocol owners.
pub(super) fn serve_tls(
    listener: std_net::TcpListener,
    web_root: PathBuf,
    server_config: Arc<rustls::ServerConfig>,
    max_body_bytes: u64,
) -> Result<(), String> {
    let websocket_hub = Arc::new(WebSocketHub::default());
    serve_protocol_tasks(
        listener,
        tls_io::factory(web_root, server_config, max_body_bytes, websocket_hub),
    )
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

#[derive(Debug, Eq, PartialEq)]
enum BodyReadError {
    TooLarge,
    Invalid(String),
    Unavailable(String),
}

static NEXT_UPLOAD_FILE: AtomicU64 = AtomicU64::new(1);

struct TemporaryBodyFile {
    path: PathBuf,
}

impl Drop for TemporaryBodyFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn declared_body_exceeds_limit(headers: &http::HeaderMap, max_body_bytes: u64) -> bool {
    headers
        .get_all(http::header::CONTENT_LENGTH)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .filter_map(|value| value.trim().parse::<u64>().ok())
        .any(|length| length > max_body_bytes)
}

async fn collect_bounded_body<B>(mut body: B, max_body_bytes: u64) -> Result<Vec<u8>, BodyReadError>
where
    B: http_body::Body<Data = Bytes> + Unpin,
    B::Error: std::fmt::Display,
{
    let mut bytes = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|error| BodyReadError::Invalid(error.to_string()))?;
        let Ok(data) = frame.into_data() else {
            continue;
        };
        let next_length = (bytes.len() as u64)
            .checked_add(data.len() as u64)
            .ok_or(BodyReadError::TooLarge)?;
        if next_length > max_body_bytes {
            return Err(BodyReadError::TooLarge);
        }
        bytes.extend_from_slice(&data);
    }
    Ok(bytes)
}

async fn spool_bounded_body<B>(
    body: B,
    max_body_bytes: u64,
) -> Result<TemporaryBodyFile, BodyReadError>
where
    B: http_body::Body<Data = Bytes> + Unpin,
    B::Error: std::fmt::Display,
{
    let configured_root = std::env::var("TERLAN_SERVE_UPLOAD_ROOT").map_err(|_| {
        BodyReadError::Unavailable(
            "TERLAN_SERVE_UPLOAD_ROOT is required for file-backed request bodies".into(),
        )
    })?;
    spool_bounded_body_to_root(body, max_body_bytes, Path::new(&configured_root)).await
}

async fn spool_bounded_body_to_root<B>(
    mut body: B,
    max_body_bytes: u64,
    root: &Path,
) -> Result<TemporaryBodyFile, BodyReadError>
where
    B: http_body::Body<Data = Bytes> + Unpin,
    B::Error: std::fmt::Display,
{
    let root = root.to_path_buf();
    if !root.is_absolute() {
        return Err(BodyReadError::Unavailable(
            "TERLAN_SERVE_UPLOAD_ROOT must be absolute".into(),
        ));
    }
    std::fs::create_dir_all(&root).map_err(|error| {
        BodyReadError::Unavailable(format!("cannot create upload root: {error}"))
    })?;
    let root = std::fs::canonicalize(&root).map_err(|error| {
        BodyReadError::Unavailable(format!("cannot resolve upload root: {error}"))
    })?;
    let (temporary, mut file) = (0..16)
        .find_map(|_| {
            let sequence = NEXT_UPLOAD_FILE.fetch_add(1, Ordering::Relaxed);
            let path = root.join(format!(
                "terlan-request-body-{}-{sequence}.upload",
                std::process::id()
            ));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => Some(Ok((TemporaryBodyFile { path }, file))),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => None,
                Err(error) => Some(Err(BodyReadError::Unavailable(format!(
                    "cannot create request body file: {error}"
                )))),
            }
        })
        .unwrap_or_else(|| {
            Err(BodyReadError::Unavailable(
                "cannot allocate a unique request body file".into(),
            ))
        })?;
    let mut written = 0_u64;
    while let Some(frame) = body.frame().await {
        let frame = frame.map_err(|error| BodyReadError::Invalid(error.to_string()))?;
        let Ok(data) = frame.into_data() else {
            continue;
        };
        written = written
            .checked_add(data.len() as u64)
            .ok_or(BodyReadError::TooLarge)?;
        if written > max_body_bytes {
            return Err(BodyReadError::TooLarge);
        }
        file.write_all(&data)
            .map_err(|error| BodyReadError::Unavailable(format!("cannot spool body: {error}")))?;
    }
    file.flush()
        .map_err(|error| BodyReadError::Unavailable(format!("cannot flush body: {error}")))?;
    drop(file);
    Ok(temporary)
}

/// Hyper I/O facade; readiness and polling remain owned by the VM executor.
struct HyperVmIo {
    stream: VmReadyTcpStream,
}

impl HyperVmIo {
    fn new(stream: VmReadyTcpStream) -> Self {
        Self { stream }
    }

    fn into_inner(self) -> VmReadyTcpStream {
        self.stream
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

async fn handle_request(
    mut request: Request<Incoming>,
    web_root: &Path,
    max_body_bytes: u64,
    upgrade_slot: Option<&PendingHyperUpgradeSlot>,
) -> Response<Full<Bytes>> {
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
        let Some(path) = temporary.path.to_str() else {
            return error_response(503, "temporary request path is not UTF-8".into());
        };
        request
            .extensions_mut()
            .insert(RequestBodyFilePath(path.to_string()));
    }
    match handle_suspendable_vm_stream_request(&request, web_root).await {
        Ok(Some(response)) => {
            let (parts, body) = response.into_parts();
            return Response::from_parts(parts, Full::new(body));
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
    let response = match handle_vm_stream_request(request, web_root, &mut channel) {
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
                let (parts, body) = response.into_parts();
                return Response::from_parts(parts, Full::new(body));
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

type PendingHyperUpgradeSlot = Rc<RefCell<Option<PendingHyperUpgrade>>>;

struct PendingHyperUpgrade {
    on_upgrade: hyper::upgrade::OnUpgrade,
    session: super::handler::AotWebSocketCallbackSession,
    route: String,
    request_target: String,
}

struct HyperWebSocketIo {
    prefix: Bytes,
    prefix_offset: usize,
    stream: HyperWebSocketStream,
}

impl HyperWebSocketIo {
    fn from_upgraded(upgraded: hyper::upgrade::Upgraded) -> Result<Self, String> {
        let (prefix, stream) = match upgraded.downcast::<HyperVmIo>() {
            Ok(parts) => (
                parts.read_buf,
                HyperWebSocketStream::Plain(parts.io.into_inner()),
            ),
            Err(upgraded) => {
                let parts = upgraded.downcast::<tls_io::VmTlsHyperIo>().map_err(|_| {
                    "error[serve.websocket.upgrade]: Hyper returned an unexpected transport type"
                        .to_string()
                })?;
                (parts.read_buf, HyperWebSocketStream::Tls(parts.io))
            }
        };
        Ok(Self {
            prefix,
            prefix_offset: 0,
            stream,
        })
    }
}

enum HyperWebSocketStream {
    Plain(VmReadyTcpStream),
    Tls(tls_io::VmTlsHyperIo),
}

impl std::io::Read for HyperWebSocketStream {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Plain(stream) => std::io::Read::read(stream, buffer),
            Self::Tls(stream) => std::io::Read::read(stream, buffer),
        }
    }
}

impl std::io::Write for HyperWebSocketStream {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        match self {
            Self::Plain(stream) => std::io::Write::write(stream, buffer),
            Self::Tls(stream) => std::io::Write::write(stream, buffer),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Plain(stream) => std::io::Write::flush(stream),
            Self::Tls(stream) => std::io::Write::flush(stream),
        }
    }
}

impl std::io::Read for HyperWebSocketIo {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.prefix_offset < self.prefix.len() {
            let remaining = &self.prefix[self.prefix_offset..];
            let copied = remaining.len().min(buffer.len());
            buffer[..copied].copy_from_slice(&remaining[..copied]);
            self.prefix_offset += copied;
            return Ok(copied);
        }
        std::io::Read::read(&mut self.stream, buffer)
    }
}

impl std::io::Write for HyperWebSocketIo {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        std::io::Write::write(&mut self.stream, buffer)
    }

    fn flush(&mut self) -> io::Result<()> {
        std::io::Write::flush(&mut self.stream)
    }

    fn write_vectored(&mut self, buffers: &[IoSlice<'_>]) -> io::Result<usize> {
        std::io::Write::write_vectored(&mut self.stream, buffers)
    }
}

async fn pump_hyper_websocket(
    mut pending: PendingHyperUpgrade,
    hub: Arc<WebSocketHub>,
) -> Result<(), String> {
    let upgraded = pending.on_upgrade.await.map_err(|error| {
        format!("error[serve.websocket.upgrade]: Hyper upgrade failed: {error}")
    })?;
    let io = HyperWebSocketIo::from_upgraded(upgraded)?;
    let mut socket = WebSocket::from_raw_socket(io, Role::Server, None);
    let mut pairing = pending.session.plan().pairing().cloned();
    if let Some(pairing) = &mut pairing {
        if pairing.restoration.is_some() {
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
            )
        })
        .transpose()?;
    if let Some(lease) = &mut lease {
        lease.dispatch_admission(&mut pending.session)?;
    }
    notify_websocket_writable(&mut pending.session)?;

    loop {
        if let Some(lease) = &lease {
            if !drain_hub_outbound(&mut socket, &lease.outbound).await? {
                return pending.session.close().map(|_| ());
            }
        }
        match socket.read() {
            Ok(Message::Text(text)) => {
                let result = pending
                    .session
                    .enqueue_inbound(VmWebSocketFrame::Text(text.to_string()));
                match result {
                    Ok(()) => {
                        if let Some(lease) = &lease {
                            if pairing.as_ref().is_some_and(|pairing| pairing.stateful) {
                                lease.drain_stateful_inbound(&mut pending.session)?;
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
                flush_websocket_nonblocking(&mut socket).await?;
                notify_websocket_writable(&mut pending.session)?;
            }
            Ok(Message::Pong(_)) => {}
            Ok(Message::Close(_)) => {
                let callback = pending.session.close().map(|_| ());
                let flush = flush_websocket_nonblocking(&mut socket).await;
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
            Err(WebSocketError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => {
                protocol_sleep_until(Instant::now() + Duration::from_millis(2)).await;
            }
            Err(WebSocketError::Io(error)) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(WebSocketError::Io(error)) if is_websocket_disconnect(error.kind()) => {
                return pending.session.close().map(|_| ());
            }
            Err(WebSocketError::ConnectionClosed | WebSocketError::AlreadyClosed) => {
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

async fn flush_websocket_nonblocking(
    socket: &mut WebSocket<HyperWebSocketIo>,
) -> Result<(), String> {
    loop {
        match socket.flush() {
            Ok(()) | Err(WebSocketError::ConnectionClosed | WebSocketError::AlreadyClosed) => {
                return Ok(())
            }
            Err(WebSocketError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => {
                protocol_sleep_until(Instant::now() + Duration::from_millis(2)).await;
            }
            Err(WebSocketError::Io(error)) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(WebSocketError::Io(error)) if is_websocket_disconnect(error.kind()) => {
                return Ok(())
            }
            Err(error) => {
                return Err(format!("error[serve.websocket.transport]: {error}"));
            }
        }
    }
}

async fn drain_hub_outbound(
    socket: &mut WebSocket<HyperWebSocketIo>,
    outbound: &Receiver<String>,
) -> Result<bool, String> {
    let mut wrote = false;
    loop {
        match outbound.try_recv() {
            Ok(payload) => {
                match socket.write(Message::Text(payload.into())) {
                    Ok(()) => {}
                    Err(WebSocketError::Io(error)) if error.kind() == io::ErrorKind::WouldBlock => {
                    }
                    Err(WebSocketError::Io(error)) if is_websocket_disconnect(error.kind()) => {
                        return Ok(false)
                    }
                    Err(error) => {
                        return Err(format!("error[serve.websocket.transport]: {error}"));
                    }
                }
                wrote = true;
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                return Err(
                    "error[serve.websocket.transport]: outbound hub disconnected".to_string(),
                )
            }
        }
    }
    if wrote {
        flush_websocket_nonblocking(socket).await?;
    }
    Ok(true)
}

fn is_websocket_disconnect(kind: io::ErrorKind) -> bool {
    matches!(
        kind,
        io::ErrorKind::BrokenPipe
            | io::ErrorKind::ConnectionAborted
            | io::ErrorKind::ConnectionReset
            | io::ErrorKind::NotConnected
            | io::ErrorKind::UnexpectedEof
    )
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
