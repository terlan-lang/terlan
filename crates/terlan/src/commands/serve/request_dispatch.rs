use super::response_rendering::*;
use super::server_lifecycle::*;
use super::*;
use std::io::Write as _;

/// Handles one Hyper request for the browser package server.
///
/// Inputs:
/// - `request`: Hyper request accepted by the local HTTP service.
/// - `web_root`: validated package root owned by the connection task.
/// - `reload_hub`: shared reload subscriber registry.
/// - `websocket_hub`: shared WebSocket room state.
///
/// Output:
/// - Hyper response carrying the selected route body.
///
/// Transformation:
/// - Reads method, URI, headers, and body through Hyper/http types, then
///   preserves the existing Terlan route-manifest and VM handler routing
///   behavior above the protocol layer.
#[cfg(test)]
pub(super) async fn handle_hyper_request<B>(
    request: Request<B>,
    web_root: PathBuf,
    reload_hub: ReloadHub,
    websocket_hub: WebSocketHub,
) -> Response<ServeBody>
where
    B: hyper::body::Body<Data = Bytes> + Send + 'static,
    B::Error: std::fmt::Display,
{
    let request_id = next_request_id();
    let build_id = manifest_build_id(&web_root);
    // Keep HTTP-owned method and URI storage borrowed across route selection.
    // Projected dynamic handlers allocate only the fields observable by their
    // generated image instead of cloning method, path, and query eagerly.
    let request_method = request.method().clone();
    let request_uri = request.uri().clone();
    let method = request_method.as_str();
    let request_path = request_uri.path();
    let request_query = request_uri.query().unwrap_or("");
    let header_pairs = request_header_pairs(request.headers());
    let cookie_pairs = request_cookie_pairs(request.headers());
    if manifest_websocket_for_path(&web_root, &request_path).is_some() {
        let _ = websocket_hub;
        return websocket_upgrade_response(&request);
    }

    if request_path == RELOAD_ENDPOINT {
        if method != "GET" && method != "HEAD" {
            return serve_response(
                405,
                "Method Not Allowed",
                "text/plain; charset=utf-8",
                &[("Allow".to_string(), "GET, HEAD".to_string())],
                b"method not allowed",
                false,
            );
        }
        return reload_sse_response(reload_hub, method == "HEAD");
    }

    if method == "GET" || method == "HEAD" {
        match acme_http01_challenge(&web_root, &request_path) {
            Ok(AcmeHttp01Challenge::Found(body)) => {
                return serve_response(
                    200,
                    "OK",
                    "text/plain; charset=utf-8",
                    &[],
                    body.as_bytes(),
                    method == "HEAD",
                );
            }
            Ok(AcmeHttp01Challenge::Missing) => {
                return serve_response(
                    404,
                    "Not Found",
                    "text/plain; charset=utf-8",
                    &[],
                    b"not found",
                    method == "HEAD",
                );
            }
            Ok(AcmeHttp01Challenge::Invalid(message)) => {
                return serve_response(
                    400,
                    "Bad Request",
                    "text/plain; charset=utf-8",
                    &[],
                    message.as_bytes(),
                    method == "HEAD",
                );
            }
            Ok(AcmeHttp01Challenge::NotMatched) => {}
            Err(message) => {
                return serve_response(
                    500,
                    "Internal Server Error",
                    "text/plain; charset=utf-8",
                    &[],
                    message.as_bytes(),
                    method == "HEAD",
                );
            }
        }
    }

    if method == "GET" || method == "HEAD" {
        if let Some(response_path) = manifest_static_file_for_request(&web_root, &request_path) {
            let started = Instant::now();
            let (status, output) = static_file_response(&method, &response_path);
            log_static_result(
                request_id,
                &build_id,
                &method,
                &request_path,
                &response_path,
                status,
                started.elapsed().as_millis(),
            );
            return output;
        }
    }

    if let Some(handler) = manifest_handler_for_request(&web_root, &method, &request_path) {
        let identity = handler_log_identity(&handler);
        let started = Instant::now();
        let body_text = match request_body_text(request).await {
            Ok(body) => body,
            Err(message) => {
                let body = render_dev_error_page(
                    request_id,
                    &build_id,
                    &method,
                    &request_path,
                    &identity,
                    &message,
                )
                .into_bytes();
                let output = serve_response(
                    400,
                    "Bad Request",
                    "text/html; charset=utf-8",
                    &[],
                    &body,
                    method == "HEAD",
                );
                log_handler_result(
                    request_id,
                    &build_id,
                    &method,
                    &request_path,
                    &identity,
                    400,
                    started.elapsed().as_millis(),
                );
                return output;
            }
        };
        let native_request = terlan_http_native::Request::from_parts_with_raw_query_metadata(
            method,
            request_path,
            body_text,
            terlan_http_native::RequestMetadata {
                params: handler.params.clone(),
                query_string: request_query.to_owned(),
                query: query_pairs(&request_query),
                headers: header_pairs,
                cookies: cookie_pairs,
            },
        );
        let result = execute_dynamic_vm_handler(&web_root, &handler, native_request);
        match result {
            Ok(response) => {
                let status = response.status;
                let output = response
                    .into_http(method == "HEAD")
                    .map(terlan_http_native::response_body::ResponseBody::from_response)
                    .map(|response| response.map(BodyExt::boxed))
                    .unwrap_or_else(|error| internal_error_response(error.message().to_owned()));
                log_handler_result(
                    request_id,
                    &build_id,
                    &method,
                    &request_path,
                    &identity,
                    status,
                    started.elapsed().as_millis(),
                );
                return output;
            }
            Err(message) => {
                let body = render_dev_error_page(
                    request_id,
                    &build_id,
                    &method,
                    &request_path,
                    &identity,
                    &message,
                )
                .into_bytes();
                let output = serve_response(
                    502,
                    "Bad Gateway",
                    "text/html; charset=utf-8",
                    &[],
                    &body,
                    method == "HEAD",
                );
                log_handler_result(
                    request_id,
                    &build_id,
                    &method,
                    &request_path,
                    &identity,
                    502,
                    started.elapsed().as_millis(),
                );
                return output;
            }
        }
    }

    if let Some(response) = manifest_static_response_for_request(&web_root, &method, &request_path)
    {
        let started = Instant::now();
        record_manifest_callable_coverage(&response.module, &response.function, response.arity);
        let status = response.status;
        let headers = static_response_header_tuples(&response.headers).unwrap_or_else(|message| {
            eprintln!("{message}");
            Vec::new()
        });
        let output = serve_response(
            response.status,
            http_reason_phrase(response.status),
            &response.content_type,
            &headers,
            response.body.as_bytes(),
            method == "HEAD",
        );
        log_static_route_result(RouteLogEvent {
            request_id,
            build_id: &build_id,
            request_method: &method,
            request_path: &request_path,
            route_method: &response.method,
            route: &response.route,
            response_path: None,
            source: response.source.as_ref(),
            status,
            duration_ms: started.elapsed().as_millis(),
        });
        return output;
    }

    if let Some((response, response_path)) =
        manifest_file_response_for_request(&web_root, &method, &request_path)
    {
        let started = Instant::now();
        record_manifest_callable_coverage(&response.module, &response.function, response.arity);
        let (status, output) = manifest_file_response(&method, &response_path, &response);
        log_file_route_result(RouteLogEvent {
            request_id,
            build_id: &build_id,
            request_method: &method,
            request_path: &request_path,
            route_method: &response.method,
            route: &response.route,
            response_path: Some(&response_path),
            source: response.source.as_ref(),
            status,
            duration_ms: started.elapsed().as_millis(),
        });
        return output;
    }

    if method != "GET" && method != "HEAD" {
        return serve_response(
            405,
            "Method Not Allowed",
            "text/plain; charset=utf-8",
            &[("Allow".to_string(), "GET, HEAD".to_string())],
            b"method not allowed",
            method == "HEAD",
        );
    }

    if std::env::var("TERLAN_SERVE_MANIFEST_ONLY").as_deref() == Ok("1") {
        return serve_response(
            404,
            "Not Found",
            "text/plain; charset=utf-8",
            &[],
            b"not found",
            method == "HEAD",
        );
    }

    let Some(response_path) = request_file_path(&web_root, &request_path) else {
        return serve_response(
            400,
            "Bad Request",
            "text/plain; charset=utf-8",
            &[],
            b"bad request",
            method == "HEAD",
        );
    };

    let started = Instant::now();
    let (status, output) = static_file_response(&method, &response_path);
    log_static_result(
        request_id,
        &build_id,
        &method,
        &request_path,
        &response_path,
        status,
        started.elapsed().as_millis(),
    );
    output
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
    let body_file_path = request
        .extensions
        .get::<terlan_http_native::request_ingress::RequestBodyFile>()
        .map(|file| file.path().to_owned())
        .unwrap_or_default();
    let method = request.method.as_str();
    let request_path = request.uri.path();
    let request_query = request.uri.query().unwrap_or("");

    let route = match manifest_route_for_request(web_root, method, request_path) {
        Some(MatchedWebPackageRoute::WebSocket(websocket)) => {
            use terlan_http_native::websocket::handshake::{opening_handshake, OpeningHandshake};
            let upgrade =
                match opening_handshake(&request.method, request.version, &request.headers) {
                    OpeningHandshake::Reject(response) => return Ok(response),
                    OpeningHandshake::Upgrade(response) => response,
                };
            let native_request = terlan_http_native::Request::from_parts_with_raw_query_metadata(
                method.to_owned(),
                request_path.to_owned(),
                body.clone(),
                terlan_http_native::RequestMetadata::from_http(
                    terlan_http_native::RequestFieldProjection::Complete,
                    &[],
                    request_query,
                    &request.headers,
                ),
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

    if request_path == RELOAD_ENDPOINT {
        if method != "GET" && method != "HEAD" {
            return serve_vm_stream_response(
                405,
                "Method Not Allowed",
                "text/plain; charset=utf-8",
                &[("Allow".to_string(), "GET, HEAD".to_string())],
                b"method not allowed",
                false,
            );
        }
        return reload_vm_stream_response(method == "HEAD");
    }

    if method == "GET" || method == "HEAD" {
        match acme_http01_challenge(web_root, request_path) {
            Ok(AcmeHttp01Challenge::Found(body)) => {
                return serve_vm_stream_response(
                    200,
                    "OK",
                    "text/plain; charset=utf-8",
                    &[],
                    body.as_bytes(),
                    method == "HEAD",
                );
            }
            Ok(AcmeHttp01Challenge::Missing) => {
                return serve_vm_stream_response(
                    404,
                    "Not Found",
                    "text/plain; charset=utf-8",
                    &[],
                    b"not found",
                    method == "HEAD",
                );
            }
            Ok(AcmeHttp01Challenge::Invalid(message)) => {
                return serve_vm_stream_response(
                    400,
                    "Bad Request",
                    "text/plain; charset=utf-8",
                    &[],
                    message.as_bytes(),
                    method == "HEAD",
                );
            }
            Ok(AcmeHttp01Challenge::NotMatched) => {}
            Err(message) => {
                return serve_vm_stream_response(
                    500,
                    "Internal Server Error",
                    "text/plain; charset=utf-8",
                    &[],
                    message.as_bytes(),
                    method == "HEAD",
                );
            }
        }
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
                        let native_request =
                            terlan_http_native::Request::from_parts_with_raw_query_metadata(
                                if projection
                                    .requires(terlan_http_native::RequestFieldProjection::METHOD)
                                {
                                    method.to_owned()
                                } else {
                                    Default::default()
                                },
                                if projection
                                    .requires(terlan_http_native::RequestFieldProjection::PATH)
                                {
                                    request_path.to_owned()
                                } else {
                                    Default::default()
                                },
                                body,
                                terlan_http_native::RequestMetadata::from_http(
                                    projection,
                                    &handler.params,
                                    request_query,
                                    &request.headers,
                                ),
                            )
                            .with_body_file_path(
                                if projection.requires(
                                    terlan_http_native::RequestFieldProjection::BODY_FILE_PATH,
                                ) {
                                    body_file_path.clone()
                                } else {
                                    String::new()
                                },
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
                let native_request =
                    terlan_http_native::Request::from_parts_with_raw_query_metadata(
                        method.to_owned(),
                        request_path.to_owned(),
                        body,
                        terlan_http_native::RequestMetadata::from_http(
                            terlan_http_native::RequestFieldProjection::Complete,
                            &[],
                            request_query,
                            &request.headers,
                        ),
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
                let native_request =
                    terlan_http_native::Request::from_parts_with_raw_query_metadata(
                        method.to_owned(),
                        request_path.to_owned(),
                        body,
                        terlan_http_native::RequestMetadata::from_http(
                            terlan_http_native::RequestFieldProjection::Complete,
                            &[],
                            request_query,
                            &request.headers,
                        ),
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
                        *channel = Some(VmHttpChannelTransport::Sse(*session));
                        serve_vm_stream_response(
                            200,
                            "OK",
                            "text/event-stream",
                            &[
                                ("cache-control".to_string(), "no-cache".to_string()),
                                ("x-content-type-options".to_string(), "nosniff".to_string()),
                                ("connection".to_string(), "keep-alive".to_string()),
                            ],
                            b": connected\n\n",
                            method == "HEAD",
                        )
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
pub(super) use request_values::*;
