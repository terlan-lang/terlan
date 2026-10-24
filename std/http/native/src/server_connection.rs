//! Maintained HTTP connection dispatch and source-channel handoff.

use std::cell::RefCell;
use std::convert::Infallible;
use std::future::{poll_fn, Future};
use std::io::{Read, Write};
use std::pin::Pin;
use std::rc::Rc;
use std::sync::Arc;

use hyper::body::Incoming;
use hyper::service::service_fn;
use hyper::{Request, Response};

use crate::response_body::ResponseBody;
use crate::websocket::connection::Callbacks;
use crate::websocket::hub::WebSocketHub;
use crate::websocket::upgrade::UpgradeSlot;
use crate::ServiceError;

/// Selects the maintained protocol from the authenticated TLS negotiation.
/// HTTP/1 owns upgrade leases; HTTP/2 receives no HTTP/1 upgrade slot. The host
/// drives all submitted stream tasks under the same bounded connection owner.
pub async fn serve_tls<S, C, H, R, W, F, E>(
    io: crate::tls::TlsIo<S>,
    handle: H,
    hub: &Arc<WebSocketHub>,
    wait: W,
    execute: E,
) -> Result<(), ServiceError>
where
    S: Read + Write + terlan_net_native::tls_stream::ShutdownWrite + Unpin + Send + 'static,
    C: Callbacks + 'static,
    H: Fn(Request<Incoming>, Option<Rc<UpgradeSlot<C>>>) -> R + 'static,
    R: Future<Output = Response<ResponseBody>> + 'static,
    W: FnMut() -> F,
    F: Future<Output = ()>,
    E: Fn(crate::http2::StreamTask) + Clone + 'static,
{
    match io
        .negotiated_protocol()
        .map_err(|error| error.to_string())?
    {
        crate::tls::HttpProtocol::Http1 => {
            serve_http1(
                io,
                move |request, slot| handle(request, Some(slot)),
                hub,
                wait,
                |error| format!("Hyper HTTP/1.1 TLS connection failed: {error}"),
            )
            .await
        }
        crate::tls::HttpProtocol::Http2 => {
            let service = service_fn(move |request| {
                let response = handle(request, None);
                async move { Ok::<_, Infallible>(response.await) }
            });
            crate::http2::serve_connection(io, service, execute)
                .await
                .map_err(|error| format!("Hyper HTTP/2 TLS connection failed: {error}").into())
        }
    }
}

/// The host supplies request execution and readiness waiting, but never takes
/// ownership of pending upgrades. Dropping or failing the connection therefore
/// cancels admitted callbacks through the same package-owned lease.
pub async fn serve_http1<I, C, H, R, W, F>(
    io: I,
    handle: H,
    hub: &Arc<WebSocketHub>,
    wait: W,
    failure_context: impl FnOnce(hyper::Error) -> String,
) -> Result<(), ServiceError>
where
    I: hyper::rt::Read + hyper::rt::Write + Read + Write + Unpin + Send + 'static,
    C: Callbacks + 'static,
    H: Fn(Request<Incoming>, Rc<UpgradeSlot<C>>) -> R + 'static,
    R: Future<Output = Response<ResponseBody>> + 'static,
    W: FnMut() -> F,
    F: Future<Output = ()>,
{
    let pending_upgrade = Rc::new(UpgradeSlot::default());
    let service_slot = Rc::clone(&pending_upgrade);
    let rejected_upgrade = Rc::new(RefCell::new(None));
    let service_rejected = Rc::clone(&rejected_upgrade);
    let service = service_fn(move |request: Request<Incoming>| {
        let slot = Rc::clone(&service_slot);
        let rejected = Rc::clone(&service_rejected);
        let upgrade = request
            .extensions()
            .get::<hyper::upgrade::OnUpgrade>()
            .cloned();
        let response = handle(request, Rc::clone(&slot));
        async move {
            let response = response.await;
            let response = match slot.validate_response(response.status().as_u16()) {
                Ok(()) => response,
                Err(error) => {
                    let mut response = crate::request_pipeline::error_response(
                        http::StatusCode::INTERNAL_SERVER_ERROR,
                        format!("error[{}]: {}", error.code(), error.message()),
                    );
                    response.headers_mut().insert(
                        http::header::CONNECTION,
                        http::HeaderValue::from_static("close"),
                    );
                    response
                }
            };
            if response.status() != http::StatusCode::SWITCHING_PROTOCOLS && upgrade.is_some() {
                *rejected.borrow_mut() = upgrade;
            }
            Ok::<_, Infallible>(response)
        }
    });
    crate::http1::serve_connection(io, service)
        .await
        .map_err(failure_context)?;
    let pending = pending_upgrade.take();
    if let Some(pending) = pending {
        // Hyper must return the exact transport that this connection admitted.
        pending
            .serve::<I, _, _>(hub, wait)
            .await
            .map_err(String::from)?;
    } else {
        // Hyper can hand off an upgrade-request transport even after rejection.
        // Keep its receiver alive until the response flushes so TLS can close cleanly.
        let rejected = rejected_upgrade.borrow_mut().take();
        if let Some(upgrade) = rejected {
            if let Ok(mut io) = upgrade.await {
                poll_fn(|cx| hyper::rt::Write::poll_shutdown(Pin::new(&mut io), cx))
                    .await
                    .map_err(|error| format!("error[serve_http.upgrade_shutdown]: {error}"))?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "server_connection_test.rs"]
mod tests;

#[cfg(test)]
#[path = "server_tls_test.rs"]
mod tls_tests;
