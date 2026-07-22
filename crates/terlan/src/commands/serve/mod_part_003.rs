
/// Builds one static file response for `terlc serve`.
///
/// Inputs:
/// - `method`: parsed request method.
/// - `response_path`: resolved package file path to read.
///
/// Output:
/// - Emitted status code for request logging.
/// - Hyper response for the selected file or a stable 404.
///
/// Transformation:
/// - Reads the selected file, injects the local reload client for HTML
///   responses, selects MIME type by extension, and builds a typed HTTP
///   response for Hyper.
#[cfg(test)]
fn static_file_response(method: &str, response_path: &Path) -> (u16, Response<ServeBody>) {
    let bytes = match fs::read(&response_path) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                404,
                serve_response(
                    404,
                    "Not Found",
                    "text/plain; charset=utf-8",
                    &[],
                    b"not found",
                    method == "HEAD",
                ),
            );
        }
    };
    let content_type = content_type_for_path(&response_path);
    let body = if content_type.starts_with("text/html") {
        String::from_utf8(bytes)
            .map(|html| inject_reload_script(&html).into_bytes())
            .unwrap_or_else(|err| err.into_bytes())
    } else {
        bytes
    };
    (
        200,
        serve_response(200, "OK", &content_type, &[], &body, method == "HEAD"),
    )
}

/// Builds one manifest file-route response for `terlc serve`.
///
/// Inputs:
/// - `method`: parsed request method.
/// - `response_path`: resolved package file path to read.
/// - `response`: manifest file response metadata.
///
/// Output:
/// - Emitted status code for request logging.
/// - Hyper response for the configured file or a stable 404.
///
/// Transformation:
/// - Reads the selected file, uses explicit manifest content type when
///   supplied or infers it by path, and builds a typed HTTP response without
///   modifying the file bytes.
#[cfg(test)]
fn manifest_file_response(
    method: &str,
    response_path: &Path,
    response: &WebPackageFileResponse,
) -> (u16, Response<ServeBody>) {
    let bytes = match fs::read(response_path) {
        Ok(bytes) => bytes,
        Err(_) => {
            return (
                404,
                serve_response(
                    404,
                    "Not Found",
                    "text/plain; charset=utf-8",
                    &[],
                    b"not found",
                    method == "HEAD",
                ),
            );
        }
    };
    let inferred_content_type;
    let content_type = match response.content_type.as_deref() {
        Some(content_type) => content_type,
        None => {
            inferred_content_type = content_type_for_path(response_path);
            inferred_content_type.as_str()
        }
    };
    (
        response.status,
        serve_response(
            response.status,
            http_reason_phrase(response.status),
            content_type,
            &[],
            &bytes,
            method == "HEAD",
        ),
    )
}

/// Builds one static file response for the VM-stream HTTP adapter.
///
/// Inputs:
/// - `method`: parsed request method.
/// - `response_path`: resolved package file path to read.
///
/// Output:
/// - Text-body HTTP response accepted by the VM HTTP writer.
///
/// Transformation:
/// - Reuses the same file, MIME, and reload-injection behavior as the Hyper
///   static file path, then validates metadata through the shared response
///   builder before crossing into VM HTTP serialization.
#[cfg_attr(not(test), allow(dead_code))]
fn static_vm_stream_file_response(
    method: &str,
    response_path: &Path,
) -> Result<::http::Response<String>, String> {
    let bytes = match fs::read(response_path) {
        Ok(bytes) => bytes,
        Err(_) => {
            return serve_vm_stream_response(
                404,
                "Not Found",
                "text/plain; charset=utf-8",
                &[],
                b"not found",
                method == "HEAD",
            );
        }
    };
    let content_type = content_type_for_path(response_path);
    let body = if content_type.starts_with("text/html") {
        String::from_utf8(bytes)
            .map(|html| inject_reload_script(&html).into_bytes())
            .unwrap_or_else(|err| err.into_bytes())
    } else {
        bytes
    };
    serve_vm_stream_response(200, "OK", &content_type, &[], &body, method == "HEAD")
}

/// Builds one manifest file-route response for the VM-stream HTTP adapter.
///
/// Inputs:
/// - `method`: parsed request method.
/// - `response_path`: resolved package file path to read.
/// - `response`: manifest file response metadata.
///
/// Output:
/// - Text-body HTTP response accepted by the VM HTTP writer.
///
/// Transformation:
/// - Mirrors manifest file response resolution outside Hyper so production
///   serve can move protocol ownership into VM TCP without changing route
///   semantics.
#[cfg_attr(not(test), allow(dead_code))]
fn manifest_vm_stream_file_response(
    method: &str,
    response_path: &Path,
    response: &WebPackageFileResponse,
) -> Result<::http::Response<String>, String> {
    let bytes = match fs::read(response_path) {
        Ok(bytes) => bytes,
        Err(_) => {
            return serve_vm_stream_response(
                404,
                "Not Found",
                "text/plain; charset=utf-8",
                &[],
                b"not found",
                method == "HEAD",
            );
        }
    };
    let inferred_content_type;
    let content_type = match response.content_type.as_deref() {
        Some(content_type) => content_type,
        None => {
            inferred_content_type = content_type_for_path(response_path);
            inferred_content_type.as_str()
        }
    };
    serve_vm_stream_response(
        response.status,
        http_reason_phrase(response.status),
        content_type,
        &[],
        &bytes,
        method == "HEAD",
    )
}

/// Builds a validated string-body response for the VM HTTP writer.
///
/// Inputs:
/// - Standard serve response metadata and raw response bytes.
///
/// Output:
/// - `http::Response<String>` accepted by `VmHttpTcpServer`.
///
/// Transformation:
/// - Runs response metadata through the same Rust HTTP validation as Hyper,
///   removes transport headers that the VM HTTP writer owns, and converts the
///   body to text at the current VM HTTP boundary.
#[cfg_attr(not(test), allow(dead_code))]
fn serve_vm_stream_response(
    status: u16,
    reason: &str,
    content_type: &str,
    extra_headers: &[(String, String)],
    body: &[u8],
    head_only: bool,
) -> Result<::http::Response<String>, String> {
    let response = build_http_response(status, content_type, extra_headers, body, head_only)
        .map_err(|message| format!("response build failed for {status} {reason}: {message}"))?;
    let (parts, body) = response.into_parts();
    let mut builder = ::http::Response::builder().status(parts.status);
    for (name, value) in &parts.headers {
        if name == http::header::CONNECTION {
            continue;
        }
        builder = builder.header(name, value);
    }
    builder
        .body(String::from_utf8_lossy(&body).into_owned())
        .map_err(|error| format!("VM stream response cannot be built: {error}"))
}

/// Builds a VM-stream WebSocket opening-handshake response.
fn serve_vm_stream_websocket_upgrade_response(
    request: &::http::Request<String>,
) -> Result<::http::Response<String>, String> {
    let key = request
        .headers()
        .get("sec-websocket-key")
        .and_then(|value| value.to_str().ok())
        .ok_or_else(|| "VM stream WebSocket upgrade is missing Sec-WebSocket-Key".to_string())?;
    let upgrade = crate::runtime::vm::websocket::build_websocket_upgrade_response(key)?;
    let status = ::http::StatusCode::from_u16(upgrade.status)
        .map_err(|error| format!("VM stream WebSocket status is invalid: {error}"))?;
    let mut builder = ::http::Response::builder().status(status);
    for (name, value) in upgrade.headers {
        builder = builder.header(name, value);
    }
    builder
        .body(String::new())
        .map_err(|error| format!("VM stream WebSocket response cannot be built: {error}"))
}

/// Reads all currently available response bytes from one VM TCP client stream.
#[cfg_attr(not(test), allow(dead_code))]
fn read_vm_stream_response_bytes(
    tcp: &mut VmTcpRuntime,
    client: VmTcpStream,
) -> Result<Vec<u8>, String> {
    let mut response = Vec::new();
    while let Some(bytes) = tcp.receive(client, 64 * 1024)? {
        response.extend(bytes);
    }
    if response.is_empty() {
        return Err("VM stream request produced no response bytes".to_string());
    }
    Ok(response)
}

/// Builds one local live-reload SSE response.
///
/// Inputs:
/// - `reload_hub`: shared reload subscriber registry.
/// - `head_only`: whether the request should return headers without opening
///   the event stream.
///
/// Output:
/// - Streaming Hyper response for local reload events, or an empty response
///   with the same headers for HEAD requests.
///
/// Transformation:
/// - Registers GET connections as reload subscribers, emits the initial SSE
///   comment, and forwards reload version values as streamed frames. HEAD
///   requests preserve the route contract without allocating a subscriber.
#[cfg(test)]
fn reload_sse_response(reload_hub: ReloadHub, head_only: bool) -> Response<ServeBody> {
    if head_only {
        return http::Response::builder()
            .status(200)
            .header(http::header::CONTENT_TYPE, "text/event-stream")
            .header(http::header::CACHE_CONTROL, "no-cache")
            .header("x-content-type-options", "nosniff")
            .header(http::header::CONNECTION, "keep-alive")
            .header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
            .body(Full::new(Bytes::new()).boxed())
            .unwrap_or_else(|err| {
                internal_error_response(format!("reload response failed: {err}"))
            });
    }

    let (tx, rx) = std_mpsc::channel();
    if let Ok(mut subscribers) = reload_hub.lock() {
        subscribers.push(tx);
    }

    let body = ReloadSseBody {
        initial_frame_pending: true,
        receiver: Mutex::new(rx),
    }
    .boxed();

    http::Response::builder()
        .status(200)
        .header(http::header::CONTENT_TYPE, "text/event-stream")
        .header(http::header::CACHE_CONTROL, "no-cache")
        .header("x-content-type-options", "nosniff")
        .header(http::header::CONNECTION, "keep-alive")
        .header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
        .body(body)
        .unwrap_or_else(|err| internal_error_response(format!("reload response failed: {err}")))
}

/// Builds one Hyper response from validated response metadata.
///
/// Inputs:
/// - `status`: numeric response status.
/// - `reason`: response reason phrase retained for fallback diagnostics.
/// - `content_type`: response content type.
/// - `extra_headers`: validated handler or manifest headers.
/// - `body`: response body bytes.
/// - `head_only`: whether to omit emitted body bytes.
///
/// Output:
/// - Hyper response with a boxed body.
///
/// Transformation:
/// - Builds a Rust `http::Response<Vec<u8>>` through the shared response
///   helper, then converts its body into Hyper's boxed body type.
#[cfg(test)]
fn serve_response(
    status: u16,
    reason: &str,
    content_type: &str,
    extra_headers: &[(String, String)],
    body: &[u8],
    head_only: bool,
) -> Response<ServeBody> {
    match build_http_response(status, content_type, extra_headers, body, head_only) {
        Ok(response) => response.map(boxed_body),
        Err(message) => internal_error_response(format!(
            "response build failed for {status} {reason}: {message}"
        )),
    }
}

/// Wraps bytes in the Hyper body type used by `terlc serve`.
///
/// Inputs:
/// - `body`: response bytes selected by route handling.
///
/// Output:
/// - Boxed Hyper body.
///
/// Transformation:
/// - Converts concrete bytes into a single-frame body accepted by Hyper.
#[cfg(test)]
fn boxed_body(body: Vec<u8>) -> ServeBody {
    Full::new(Bytes::from(body)).boxed()
}

/// Builds a generic internal error response for protocol-boundary failures.
///
/// Inputs:
/// - `message`: diagnostic text for local development response body.
///
/// Output:
/// - Hyper response with status 500.
///
/// Transformation:
/// - Avoids panics in the Hyper service by turning unexpected response build
///   failures into ordinary local development responses.
#[cfg(test)]
fn internal_error_response(message: String) -> Response<ServeBody> {
    http::Response::builder()
        .status(500)
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(boxed_body(message.into_bytes()))
        .unwrap_or_else(|_| Response::new(boxed_body(b"internal server error".to_vec())))
}

/// Converts a URL request path into a package file path.
///
/// Inputs:
/// - `web_root`: package root.
/// - `request_path`: URL path component.
///
/// Output:
/// - Safe filesystem path under `web_root`, or `None` for unsafe paths.
///
/// Transformation:
/// - Maps `/` to `index.html`, strips a leading slash, and rejects traversal,
///   Windows separators, and NUL bytes.
fn request_file_path(web_root: &Path, request_path: &str) -> Option<PathBuf> {
    let trimmed = request_path.trim_start_matches('/');
    if trimmed.is_empty() {
        return Some(web_root.join("index.html"));
    }
    package_relative_path(web_root, trimmed)
}

/// Converts a manifest-relative path into a safe package file path.
///
/// Inputs:
/// - `web_root`: package root.
/// - `relative`: manifest-relative path text.
///
/// Output:
/// - Safe filesystem path under `web_root`, or `None` for unsafe paths.
///
/// Transformation:
/// - Rejects absolute paths, parent components, prefixes, Windows separators,
///   and NUL bytes before joining accepted normal components.
pub(super) fn package_relative_path(web_root: &Path, relative: &str) -> Option<PathBuf> {
    if relative.contains('\\') || relative.contains('\0') {
        return None;
    }
    let relative_path = Path::new(relative);
    if relative_path.is_absolute() {
        return None;
    }

    let mut output = web_root.to_path_buf();
    for component in relative_path.components() {
        match component {
            Component::Normal(segment) => output.push(segment),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(output)
}
