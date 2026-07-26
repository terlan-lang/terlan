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
use super::{handle_suspendable_vm_stream_request, handle_vm_stream_request};

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
    #[allow(unsafe_code)]
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
mod hyper_server_test;
