//! HTTP admission and dispatch policy over host-supplied application execution.

use std::future::Future;
use std::path::Path;

use bytes::Bytes;
use http::{Request, Response, StatusCode};
use hyper::body::Body;

use crate::request_ingress::{prepare_request, validate_declared_length, BodyStorage};
use crate::response_body::ResponseBody;
use crate::sse_callbacks::SseCancellation;
use crate::websocket::connection::Callbacks;
use crate::websocket::upgrade::{self, UpgradeSlot};

/// An admitted source session whose transport must accept or cancel ownership.
#[derive(Debug)]
pub enum Channel<W, S> {
    WebSocket(W),
    Sse(S),
}

/// The host resolves and executes compiled application code. HTTP admission,
/// body lifetime, protocol handoff, and failure responses stay in this package.
pub trait Application {
    type WebSocket: Callbacks;
    type Sse: SseCancellation;

    fn requires_file_body(&self, method: &str, path: &str) -> Result<bool, String>;
    fn handle_suspendable(
        &self,
        request: &Request<String>,
    ) -> impl Future<Output = Result<Option<Response<Bytes>>, String>>;
    fn handle(
        &self,
        request: Request<String>,
        channel: &mut Option<Channel<Self::WebSocket, Self::Sse>>,
    ) -> Result<Response<Bytes>, String>;
}

pub async fn handle<A, B>(
    application: &A,
    mut request: Request<B>,
    max_body_bytes: u64,
    upload_root: Option<&Path>,
    upgrade_slot: Option<&UpgradeSlot<A::WebSocket>>,
) -> Response<ResponseBody>
where
    A: Application,
    B: Body<Data = Bytes> + Unpin,
    B::Error: std::fmt::Display,
{
    if let Err(error) = validate_declared_length(request.headers(), max_body_bytes) {
        return error_response(error.status, error.message);
    }
    let file_backed =
        match application.requires_file_body(request.method().as_str(), request.uri().path()) {
            Ok(file_backed) => file_backed,
            Err(error) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, error),
        };
    let storage = if file_backed {
        match upload_root {
            Some(root) => BodyStorage::File(root),
            None => {
                return error_response(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "TERLAN_SERVE_UPLOAD_ROOT is required for file-backed request bodies".into(),
                )
            }
        }
    } else {
        BodyStorage::Text
    };
    let on_upgrade = upgrade_slot.map(|_| hyper::upgrade::on(&mut request));
    let request = match prepare_request(request, max_body_bytes, storage).await {
        Ok(request) => request,
        Err(error) => return error_response(error.status, error.message),
    };
    match application.handle_suspendable(&request).await {
        Ok(Some(response)) => return ResponseBody::from_response(response),
        Ok(None) => {}
        Err(error) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, error),
    }
    let route = request.uri().path().to_string();
    let target = request
        .uri()
        .path_and_query()
        .map(|target| target.as_str().to_string())
        .unwrap_or_else(|| route.clone());
    let mut channel = None;
    let response = match application.handle(request, &mut channel) {
        Ok(response) => response,
        Err(error) => {
            let error = match channel {
                Some(mut channel) => cancel(&mut channel, error),
                None => error,
            };
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, error);
        }
    };
    match channel {
        Some(Channel::WebSocket(session)) => {
            if let Err(error) = upgrade::admit(upgrade_slot, on_upgrade, session, route, target) {
                let status = StatusCode::from_u16(error.status() as u16)
                    .expect("package upgrade errors carry valid HTTP status codes");
                return error_response(
                    status,
                    format!("error[{}]: {}", error.code(), error.message()),
                );
            }
        }
        Some(mut channel @ Channel::Sse(_)) => {
            let message = cancel(&mut channel,
                "error[serve_http.upgrade_adapter_missing]: maintained async Hyper adapter is required for SSE".into());
            return error_response(StatusCode::NOT_IMPLEMENTED, message);
        }
        None => {}
    }
    ResponseBody::from_response(response)
}

fn cancel<W: Callbacks, S: SseCancellation>(channel: &mut Channel<W, S>, reason: String) -> String {
    let result = match channel {
        Channel::WebSocket(session) => session.cancel(reason.clone()),
        Channel::Sse(session) => session.cancel(reason.clone()),
    };
    match result {
        Ok(()) => reason,
        Err(cleanup) => format!("{reason}; cancellation failed: {cleanup}"),
    }
}

pub(crate) fn error_response(status: StatusCode, message: String) -> Response<ResponseBody> {
    let mut response = Response::new(ResponseBody::Buffered(http_body_util::Full::new(
        Bytes::from(message),
    )));
    *response.status_mut() = status;
    response.headers_mut().insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

#[cfg(test)]
#[path = "request_pipeline_test.rs"]
mod tests;
