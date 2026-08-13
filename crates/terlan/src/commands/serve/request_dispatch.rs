use super::response_rendering::*;
use super::server_lifecycle::*;
use super::*;

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
        if method != "GET" {
            return serve_response(
                405,
                "Method Not Allowed",
                "text/plain; charset=utf-8",
                &[
                    ("allow".to_string(), "GET".to_string()),
                    ("upgrade".to_string(), "websocket".to_string()),
                ],
                b"websocket upgrades require GET",
                method == "HEAD",
            );
        }
        let _ = websocket_hub;
        match websocket_upgrade_state(request.headers()) {
            WebSocketUpgradeState::Missing => {
                return serve_response(
                    426,
                    "Upgrade Required",
                    "text/plain; charset=utf-8",
                    &[("upgrade".to_string(), "websocket".to_string())],
                    b"websocket upgrade required",
                    false,
                );
            }
            WebSocketUpgradeState::Malformed => {
                return serve_response(
                    400,
                    "Bad Request",
                    "text/plain; charset=utf-8",
                    &[],
                    b"malformed websocket upgrade request",
                    false,
                );
            }
            WebSocketUpgradeState::Upgrade => {}
        }
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
        let native_request =
            crate::terlan_native::http::Request::from_parts_with_raw_query_metadata(
                method,
                request_path,
                body_text,
                crate::terlan_native::http::RequestMetadata {
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
                let output = serve_response(
                    response.status,
                    http_reason_phrase(response.status),
                    &response.content_type,
                    &response.headers,
                    response.body.as_bytes(),
                    method == "HEAD",
                );
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

    let Some(file_path) = request_file_path(&web_root, &request_path) else {
        return serve_response(
            400,
            "Bad Request",
            "text/plain; charset=utf-8",
            &[],
            b"bad request",
            method == "HEAD",
        );
    };
    let response_path = if file_path.is_dir() {
        file_path.join("index.html")
    } else if file_path.exists() {
        file_path
    } else if file_path.extension().is_none() {
        file_path.join("index.html")
    } else {
        file_path
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
) -> Result<::http::Response<Bytes>, String> {
    let (request, body) = request.into_parts();
    let method = request.method.as_str();
    let request_path = request.uri.path();
    let request_query = request.uri.query().unwrap_or("");

    let route = match manifest_route_for_request(web_root, method, request_path) {
        Some(MatchedWebPackageRoute::WebSocket(websocket)) => {
            if method != "GET" {
                return serve_vm_stream_response(
                    405,
                    "Method Not Allowed",
                    "text/plain; charset=utf-8",
                    &[
                        ("allow".to_string(), "GET".to_string()),
                        ("upgrade".to_string(), "websocket".to_string()),
                    ],
                    b"websocket upgrades require GET",
                    method == "HEAD",
                );
            }
            match websocket_upgrade_state(&request.headers) {
                WebSocketUpgradeState::Missing => {
                    return serve_vm_stream_response(
                        426,
                        "Upgrade Required",
                        "text/plain; charset=utf-8",
                        &[("upgrade".to_string(), "websocket".to_string())],
                        b"websocket upgrade required",
                        method == "HEAD",
                    );
                }
                WebSocketUpgradeState::Malformed => {
                    return serve_vm_stream_response(
                        400,
                        "Bad Request",
                        "text/plain; charset=utf-8",
                        &[],
                        b"malformed websocket upgrade request",
                        method == "HEAD",
                    );
                }
                WebSocketUpgradeState::Upgrade => {}
            }
            let native_request =
                crate::terlan_native::http::Request::from_parts_with_raw_query_metadata(
                    method.to_owned(),
                    request_path.to_owned(),
                    body.clone(),
                    crate::terlan_native::http::RequestMetadata {
                        params: Vec::new(),
                        query_string: request_query.to_owned(),
                        query: query_pairs(request_query),
                        headers: request_header_pairs(&request.headers),
                        cookies: request_cookie_pairs(&request.headers),
                    },
                );
            match execute_websocket_vm_router(web_root, &websocket, &native_request) {
                Ok(Some(VmWebSocketRouterAdmission::Respond(response))) => {
                    return serve_vm_stream_response(
                        response.status,
                        http_reason_phrase(response.status),
                        &response.content_type,
                        &response.headers,
                        response.body.as_bytes(),
                        false,
                    );
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
            return serve_vm_stream_websocket_upgrade_response(&request.headers);
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
                let response = match with_cached_vm_handler_runtime_for_request(
                    web_root,
                    &handler.handler,
                    |runtime| {
                        let projection = runtime.request_projection(
                            &handler.handler.module,
                            &handler.handler.function,
                            handler.handler.arity,
                        );
                        let projected_headers = if projection
                            .requires(crate::runtime::native::http::RequestFieldProjection::HEADERS)
                        {
                            request_header_pairs(&request.headers)
                        } else {
                            Default::default()
                        };
                        let projected_cookies = if projection
                            .requires(crate::runtime::native::http::RequestFieldProjection::COOKIES)
                            || projection.requires(
                                crate::runtime::native::http::RequestFieldProjection::COOKIE_JAR,
                            ) {
                            request_cookie_pairs(&request.headers)
                        } else {
                            Default::default()
                        };
                        let native_request =
                            crate::terlan_native::http::Request::from_parts_with_raw_query_metadata(
                                if projection
                                    .requires(
                                        crate::runtime::native::http::RequestFieldProjection::METHOD,
                                    ) { method.to_owned() } else { Default::default() },
                                if projection
                                    .requires(
                                        crate::runtime::native::http::RequestFieldProjection::PATH,
                                    ) { request_path.to_owned() } else { Default::default() },
                                body,
                                crate::terlan_native::http::RequestMetadata {
                                    params: if projection
                                        .requires(
                                            crate::runtime::native::http::RequestFieldProjection::PARAMS,
                                        ) { handler.params.clone() } else { Default::default() },
                                    query_string: if projection
                                        .requires(
                                            crate::runtime::native::http::RequestFieldProjection::QUERY_STRING,
                                        ) { request_query.to_owned() } else { Default::default() },
                                    query: if projection
                                        .requires(
                                            crate::runtime::native::http::RequestFieldProjection::QUERY,
                                        ) { query_pairs(request_query) } else { Default::default() },
                                    headers: projected_headers,
                                    cookies: projected_cookies,
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
                let header_pairs = request_header_pairs(&request.headers);
                let cookie_pairs = request_cookie_pairs(&request.headers);
                let native_request =
                    crate::terlan_native::http::Request::from_parts_with_raw_query_metadata(
                        method.to_owned(),
                        request_path.to_owned(),
                        body,
                        crate::terlan_native::http::RequestMetadata {
                            params: Vec::new(),
                            query_string: request_query.to_owned(),
                            query: query_pairs(request_query),
                            headers: header_pairs,
                            cookies: cookie_pairs,
                        },
                    );
                if let Some(rendered) =
                    execute_static_vm_router(web_root, &response, &native_request)?
                {
                    return serve_vm_stream_response(
                        rendered.status,
                        http_reason_phrase(rendered.status),
                        &rendered.content_type,
                        &rendered.headers,
                        rendered.body.as_bytes(),
                        method == "HEAD",
                    );
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
                return manifest_vm_stream_file_response(method, &response_path, &response);
            }
            MatchedWebPackageRoute::Sse(endpoint) => {
                let header_pairs = request_header_pairs(&request.headers);
                let cookie_pairs = request_cookie_pairs(&request.headers);
                let native_request =
                    crate::terlan_native::http::Request::from_parts_with_raw_query_metadata(
                        method.to_owned(),
                        request_path.to_owned(),
                        body,
                        crate::terlan_native::http::RequestMetadata {
                            params: Vec::new(),
                            query_string: request_query.to_owned(),
                            query: query_pairs(request_query),
                            headers: header_pairs,
                            cookies: cookie_pairs,
                        },
                    );
                return match execute_sse_vm_router(web_root, &endpoint, &native_request) {
                    Ok(VmSseRouterAdmission::Respond(response)) => serve_vm_stream_response(
                        response.status,
                        http_reason_phrase(response.status),
                        &response.content_type,
                        &response.headers,
                        response.body.as_bytes(),
                        method == "HEAD",
                    ),
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

    let Some(file_path) = request_file_path(web_root, request_path) else {
        return serve_vm_stream_response(
            400,
            "Bad Request",
            "text/plain; charset=utf-8",
            &[],
            b"bad request",
            method == "HEAD",
        );
    };
    let response_path = if file_path.is_dir() {
        file_path.join("index.html")
    } else if file_path.exists() {
        file_path
    } else if file_path.extension().is_none() {
        file_path.join("index.html")
    } else {
        file_path
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

/// Handles one raw HTTP/1 request over VM TCP streams.
///
/// Inputs:
/// - `web_root`: generated browser package root.
/// - `raw_request`: HTTP/1 request bytes to inject into a VM TCP stream.
///
/// Output:
/// - Raw HTTP/1 response bytes produced by the VM HTTP writer.
///
/// Transformation:
/// - Connects an in-memory VM TCP client to a VM HTTP listener, dispatches the
///   request through the serve route graph, and reads the response from the
///   VM-managed stream without binding host sockets or entering Hyper.
#[cfg(test)]
pub(super) fn handle_vm_stream_http1_request(
    web_root: &Path,
    raw_request: &[u8],
) -> Result<Vec<u8>, String> {
    handle_vm_stream_http1_exchange(web_root, raw_request).map(|exchange| exchange.response)
}

/// Raw response and optional admitted channel retained for socket handoff.
#[derive(Debug)]
#[cfg(test)]
pub(super) struct VmStreamHttp1Exchange {
    pub(super) response: Vec<u8>,
    pub(super) channel: Option<VmHttpChannelTransport>,
}

/// Handles one raw request while preserving any admitted long-lived channel.
#[cfg(test)]
pub(super) fn handle_vm_stream_http1_exchange(
    web_root: &Path,
    raw_request: &[u8],
) -> Result<VmStreamHttp1Exchange, String> {
    let mut reader = std::io::Cursor::new(raw_request);
    let mut response = Vec::new();
    let mut channel = None;
    if let Err(error) =
        handle_http1_in_memory_exchange(&mut reader, &mut response, true, |request| {
            handle_vm_stream_request(request, web_root, &mut channel)
        })
    {
        return vm_stream_bad_request_response(&error).map(|response| VmStreamHttp1Exchange {
            response,
            channel: None,
        });
    }
    Ok(VmStreamHttp1Exchange { response, channel })
}

/// Converts a malformed VM-stream HTTP request into a stable wire response.
///
/// Inputs:
/// - `error`: parser/runtime diagnostic reported by the strict VM HTTP layer.
///
/// Output:
/// - Raw HTTP/1 `400 Bad Request` response bytes.
///
/// Transformation:
/// - Keeps protocol diagnostics strict inside `runtime::vm::http` while giving
///   `terlc serve` the same user-facing bad-request response shape as the
///   legacy adapter for malformed input.
#[cfg(test)]
pub(super) fn vm_stream_bad_request_response(error: &str) -> Result<Vec<u8>, String> {
    let response = serve_vm_stream_response(
        400,
        "Bad Request",
        "text/plain; charset=utf-8",
        &[],
        format!("bad request: {error}").as_bytes(),
        false,
    )?;
    let mut wire = Vec::new();
    write_http1_response(&mut wire, &response, true)?;
    Ok(wire)
}

/// Extracts source-visible request header pairs from Hyper metadata.
///
/// Inputs:
/// - `headers`: Hyper/http request header map.
///
/// Output:
/// - Header name/value pairs with lowercase header names and UTF-8-lossy
///   values.
///
/// Transformation:
/// - Converts the protocol-owned header map into the handler request-map shape
///   without exposing Hyper types to generated handler code.
pub(super) fn request_header_pairs(headers: &http::HeaderMap) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(name, value)| {
            (
                // `http::HeaderName` canonicalizes names to lowercase when
                // parsing, so copying is sufficient here; rescanning every
                // name for ASCII case conversion only burns request CPU.
                name.as_str().to_owned(),
                String::from_utf8_lossy(value.as_bytes()).into_owned(),
            )
        })
        .collect()
}

/// Parses cookies only for Request projections that can observe cookie state.
pub(super) fn request_cookie_pairs(headers: &http::HeaderMap) -> Vec<(String, String)> {
    headers
        .get(http::header::COOKIE)
        .and_then(|value| value.to_str().ok())
        .map(crate::terlan_native::http::parse_request_cookie_header)
        .unwrap_or_default()
}

/// Extracts source-visible query pairs from a raw URI query string.
///
/// Inputs:
/// - `query`: URI query text without the leading `?`.
///
/// Output:
/// - Percent-decoded query name/value pairs in request order.
///
/// Transformation:
/// - Delegates form-url-encoded parsing to the maintained `url` crate instead
///   of hand-splitting query text.
pub(super) fn query_pairs(query: &str) -> Vec<(String, String)> {
    url::form_urlencoded::parse(query.as_bytes())
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect()
}

/// Reads a Hyper request body into source-visible UTF-8 text.
///
/// Inputs:
/// - `request`: Hyper request whose metadata has already been captured.
///
/// Output:
/// - UTF-8-lossy body text for Terlan request handlers.
/// - Stable runtime diagnostic when Hyper body collection fails.
///
/// Transformation:
/// - Uses Hyper/http-body-util collection and only decodes bytes at the VM
///   request boundary, keeping protocol mechanics out of Terlan source.
#[cfg(test)]
pub(super) async fn request_body_text<B>(request: Request<B>) -> Result<String, String>
where
    B: hyper::body::Body<Data = Bytes> + Send + 'static,
    B::Error: std::fmt::Display,
{
    let body = request
        .into_body()
        .collect()
        .await
        .map_err(|err| format!("error[serve_request]: failed to read request body: {err}"))?
        .to_bytes();
    Ok(String::from_utf8_lossy(&body).into_owned())
}
