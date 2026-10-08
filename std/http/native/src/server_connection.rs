//! Maintained HTTP/1 connection lifecycle, including source-channel handoff.

use std::convert::Infallible;
use std::future::Future;
use std::io::{Read, Write};
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
    let service = service_fn(move |request| {
        let slot = Rc::clone(&service_slot);
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
    }
    Ok(())
}

#[cfg(test)]
#[path = "server_connection_test.rs"]
mod tests;
