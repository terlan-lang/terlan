use super::response_rendering::*;
use super::server_lifecycle::*;
use super::*;
use std::io::Write as _;

/// Exercises the production package pipeline with an in-memory Hyper body.
/// Reload subscriber tests retain their development-only hub.
#[cfg(test)]
pub(super) async fn handle_hyper_request<B>(
    request: Request<B>,
    web_root: PathBuf,
    reload_hub: ReloadHub,
) -> Response<ServeBody>
where
    B: hyper::body::Body<Data = Bytes> + Send + Unpin + 'static,
    B::Error: std::fmt::Display,
{
    if request.uri().path() == RELOAD_ENDPOINT
        && matches!(*request.method(), http::Method::GET | http::Method::HEAD)
    {
        return reload_sse_response(reload_hub, request.method() == http::Method::HEAD);
    }
    super::hyper_server::handle_request(request, &web_root, 1024 * 1024, None)
        .await
        .map(BodyExt::boxed_unsync)
}

/// Records a statically lowered handler when its manifest route serves a request.
fn record_manifest_callable_coverage(module: &str, function: &str, arity: usize) {
    if module.is_empty() || function.is_empty() {
        return;
    }
    let Some(path) = std::env::var_os("TERLAN_CALLABLE_COVERAGE_FILE")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
    else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let callable_id =
            crate::runtime::native_image::debug::tvm_coverage_callable_id(module, function, arity);
        let _ = writeln!(file, "{callable_id}");
    }
}

/// Handles one parsed HTTP request through the serve route graph.
///
/// Inputs:
/// - `request`: VM HTTP parser request with text body.
/// - `web_root`: generated browser package root.
///
/// Output:
/// - Validated Rust HTTP response with a text body for the VM HTTP writer.
///
/// Transformation:
/// - Reuses the same manifest, static response, and dynamic VM handler route
///   selection as the Hyper adapter while keeping protocol ownership inside
///   VM TCP/HTTP primitives.
pub(super) fn handle_vm_stream_request(
    request: ::http::Request<String>,
    web_root: &Path,
    channel: &mut Option<VmHttpChannelTransport>,
    live_sse_transport_available: bool,
) -> Result<::http::Response<Bytes>, String> {
    let (request, body) = request.into_parts();
    let method = request.method.as_str();
    let request_path = request.uri.path();

    let route = match manifest_route_for_request(web_root, method, request_path) {
        Some(MatchedWebPackageRoute::WebSocket(websocket)) => {
            use terlan_http_native::websocket::handshake::{opening_handshake, OpeningHandshake};
            let upgrade =
                match opening_handshake(&request.method, request.version, &request.headers) {
                    OpeningHandshake::Reject(response) => return Ok(response),
                    OpeningHandshake::Upgrade(response) => response,
                };
            let native_request = terlan_http_native::Request::from_http_parts(
                &request,
                body,
                &[],
                terlan_http_native::RequestFieldProjection::Complete,
            );
            match execute_websocket_vm_router(web_root, &websocket, &native_request) {
                Ok(Some(VmWebSocketRouterAdmission::Respond(response))) => {
                    return serve_vm_stream_handler_response(response, false);
                }
                Ok(Some(VmWebSocketRouterAdmission::Upgrade(session))) => {
                    debug_assert!(session.is_open());
                    debug_assert!(session.inspect().max_pending_frames > 0);
                    let _ = session.plan();
                    *channel = Some(VmHttpChannelTransport::WebSocket(*session));
                }
                Ok(None) => {}
                Err(message) => {
                    return serve_vm_stream_response(
                        502,
                        "Bad Gateway",
                        "text/plain; charset=utf-8",
                        &[],
                        message.as_bytes(),
                        false,
                    );
                }
            }
            return Ok(upgrade);
        }
        route => route,
    };

    if let Some(response) = reserved_stream_response(web_root, method, request_path)? {
        return Ok(response);
    }

    if let Some(route) = route {
        match route {
            MatchedWebPackageRoute::WebSocket(_) => {
                unreachable!("WebSocket routes are handled before reserved endpoints")
            }
            MatchedWebPackageRoute::Handler(handler) => {
                let source = handler.handler.source.as_ref();
                let traceparent = request
                    .headers
                    .get("traceparent")
                    .and_then(|value| value.to_str().ok());
                let context = crate::service_foundation::next_request_context(
                    crate::service_foundation::RequestContextDescriptor {
                        service: "terlc-serve",
                        route: &handler.handler.route,
                        module: &handler.handler.module,
                        function: &handler.handler.function,
                        release_id: &manifest_build_id(web_root),
                        source_file: source.map_or("", |source| source.path.as_str()),
                        source_line: source.map_or(0, |source| source.line),
                    },
                    traceparent,
                );
                let _request_context_scope =
                    crate::service_foundation::RequestContextScope::enter(context);
                let response = match with_cached_vm_handler_runtime_for_request(
                    web_root,
                    &handler.handler,
                    |runtime| {
                        let projection = runtime.request_projection(
                            &handler.handler.module,
                            &handler.handler.function,
                            handler.handler.arity,
                        );
                        let native_request = terlan_http_native::Request::from_http_parts(
                            &request,
                            body,
                            &handler.params,
                            projection,
                        );
                        execute_dynamic_vm_handler_with_runtime(
                            runtime,
                            web_root,
                            &handler,
                            native_request,
                            projection,
                        )
                    },
                ) {
                    Ok(response) => response,
                    Err(message) => {
                        return serve_vm_stream_response(
                            502,
                            "Bad Gateway",
                            "text/plain; charset=utf-8",
                            &[],
                            message.as_bytes(),
                            method == "HEAD",
                        );
                    }
                };
                return match response {
                    Ok(response) => serve_vm_stream_handler_response(response, method == "HEAD"),
                    Err(message) => serve_vm_stream_response(
                        502,
                        "Bad Gateway",
                        "text/plain; charset=utf-8",
                        &[],
                        message.as_bytes(),
                        method == "HEAD",
                    ),
                };
            }
            MatchedWebPackageRoute::StaticFile(response_path) => {
                return static_vm_stream_file_response(method, &response_path);
            }
            MatchedWebPackageRoute::StaticResponse(response) => {
                record_manifest_callable_coverage(
                    &response.module,
                    &response.function,
                    response.arity,
                );
                let native_request = terlan_http_native::Request::from_http_parts(
                    &request,
                    body,
                    &[],
                    terlan_http_native::RequestFieldProjection::Complete,
                );
                if let Some(rendered) =
                    execute_static_vm_router(web_root, &response, &native_request)?
                {
                    return serve_vm_stream_handler_response(rendered, method == "HEAD");
                }
                let headers = static_response_header_tuples(&response.headers)?;
                return serve_vm_stream_response(
                    response.status,
                    http_reason_phrase(response.status),
                    &response.content_type,
                    &headers,
                    response.body.as_bytes(),
                    method == "HEAD",
                );
            }
            MatchedWebPackageRoute::FileResponse(response, response_path) => {
                record_manifest_callable_coverage(
                    &response.module,
                    &response.function,
                    response.arity,
                );
                return manifest_vm_stream_file_response(method, &response_path, &response);
            }
            MatchedWebPackageRoute::Sse(endpoint) => {
                let native_request = terlan_http_native::Request::from_http_parts(
                    &request,
                    body,
                    &[],
                    terlan_http_native::RequestFieldProjection::Complete,
                );
                return match execute_sse_vm_router(
                    web_root,
                    &endpoint,
                    &native_request,
                    live_sse_transport_available,
                ) {
                    Ok(VmSseRouterAdmission::Respond(response)) => {
                        serve_vm_stream_handler_response(response, method == "HEAD")
                    }
                    Ok(VmSseRouterAdmission::Stream(session)) => {
                        debug_assert!(session.is_open());
                        debug_assert_eq!(
                            session.inspect().max_pending_events,
                            session.plan().max_pending_events()
                        );
                        let response = terlan_http_native::channel_admission::sse_stream_response(
                            session.plan(),
                        );
                        *channel = Some(VmHttpChannelTransport::Sse(*session));
                        serve_vm_stream_handler_response(response, method == "HEAD")
                    }
                    Err(message) => serve_vm_stream_response(
                        502,
                        "Bad Gateway",
                        "text/plain; charset=utf-8",
                        &[],
                        message.as_bytes(),
                        method == "HEAD",
                    ),
                };
            }
        }
    }

    if method != "GET" && method != "HEAD" {
        return serve_vm_stream_response(
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            &[("Allow".to_string(), "GET, HEAD".to_string())],
            b"method not allowed",
            method == "HEAD",
        );
    }

    if std::env::var("TERLAN_SERVE_MANIFEST_ONLY").as_deref() == Ok("1") {
        return serve_vm_stream_response(
            404,
            "Not Found",
            "text/plain; charset=utf-8",
            &[],
            b"not found",
            method == "HEAD",
        );
    }

    let Some(response_path) = request_file_path(web_root, request_path) else {
        return serve_vm_stream_response(
            400,
            "Bad Request",
            "text/plain; charset=utf-8",
            &[],
            b"bad request",
            method == "HEAD",
        );
    };
    static_vm_stream_file_response(method, &response_path)
}

/// Preserves development and challenge endpoint precedence across dispatch paths.
pub(super) fn reserved_stream_response(
    web_root: &Path,
    method: &str,
    path: &str,
) -> Result<Option<::http::Response<Bytes>>, String> {
    if path == RELOAD_ENDPOINT {
        if method == "GET" || method == "HEAD" {
            return reload_vm_stream_response(method == "HEAD").map(Some);
        }
        return serve_vm_stream_response(
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            &[("Allow".into(), "GET, HEAD".into())],
            b"method not allowed",
            false,
        )
        .map(Some);
    }
    acme_http01_response(web_root, method, path)
}

/// Builds the finite VM-stream live-reload handshake response.
///
/// Inputs:
/// - `head_only`: whether the caller requested `HEAD`.
///
/// Output:
/// - Validated VM-stream HTTP response carrying the SSE content contract.
///
/// Transformation:
/// - Reserves the local reload endpoint in the VM-owned route graph and emits
///   the initial SSE comment frame. Live reload event fan-out remains owned by
///   the live Hyper watcher until the production listener fully moves to the
///   VM stream runtime.
pub(super) fn reload_vm_stream_response(
    head_only: bool,
) -> Result<::http::Response<Bytes>, String> {
    serve_vm_stream_response(
        200,
        "OK",
        "text/event-stream",
        &[(
            http::header::ACCESS_CONTROL_ALLOW_ORIGIN
                .as_str()
                .to_string(),
            "*".to_string(),
        )],
        b": connected\n\n",
        head_only,
    )
}

#[cfg(test)]
#[path = "request_dispatch/request_values.rs"]
mod request_values;
#[cfg(test)]
pub(crate) use request_values::handle_vm_stream_http1_request;
#[cfg(test)]
pub(super) use request_values::{
    handle_vm_stream_http1_exchange, request_header_pairs, VmStreamHttp1Exchange,
};
