#[cfg(test)]
use super::server_lifecycle::ReloadSseBody;
use super::*;

/// Legacy transport tests use the same package file behavior as production.
#[cfg(test)]
pub(super) fn static_file_response(
    method: &str,
    response_path: &Path,
) -> (u16, Response<ServeBody>) {
    boxed_file_response(static_vm_stream_file_response(method, response_path))
}

#[cfg(test)]
pub(super) fn manifest_file_response(
    method: &str,
    response_path: &Path,
    response: &WebPackageFileResponse,
) -> (u16, Response<ServeBody>) {
    boxed_file_response(manifest_vm_stream_file_response(
        method,
        response_path,
        response,
    ))
}

#[cfg(test)]
fn boxed_file_response(
    response: Result<::http::Response<Bytes>, String>,
) -> (u16, Response<ServeBody>) {
    let response = match response {
        Ok(mut response) => {
            response.headers_mut().insert(
                http::header::CONNECTION,
                http::HeaderValue::from_static("close"),
            );
            response.map(|body| Full::new(body).boxed())
        }
        Err(message) => internal_error_response(message),
    };
    (response.status().as_u16(), response)
}

/// File policy belongs to std.http; only local reload injection belongs to CLI.
pub(super) fn static_vm_stream_file_response(
    method: &str,
    response_path: &Path,
) -> Result<::http::Response<Bytes>, String> {
    terlan_http_native::static_file::response(
        response_path,
        200,
        None,
        method == "HEAD",
        |content_type, bytes| {
            if content_type.starts_with("text/html") {
                String::from_utf8(bytes)
                    .map(|html| inject_reload_script(&html).into_bytes())
                    .unwrap_or_else(|error| error.into_bytes())
            } else {
                bytes
            }
        },
    )
    .map_err(|error| error.message().to_owned())
}

pub(super) fn manifest_vm_stream_file_response(
    method: &str,
    response_path: &Path,
    response: &WebPackageFileResponse,
) -> Result<::http::Response<Bytes>, String> {
    terlan_http_native::static_file::response(
        response_path,
        response.status,
        response.content_type.as_deref(),
        method == "HEAD",
        |_, bytes| bytes,
    )
    .map_err(|error| error.message().to_owned())
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
pub(super) fn serve_vm_stream_response(
    status: u16,
    reason: &str,
    content_type: &str,
    extra_headers: &[(String, String)],
    body: &[u8],
    head_only: bool,
) -> Result<::http::Response<Bytes>, String> {
    let response =
        build_http_response_for_stream(status, content_type, extra_headers, body, head_only)
            .map_err(|message| format!("response build failed for {status} {reason}: {message}"))?;
    let (parts, body) = response.into_parts();
    Ok(::http::Response::from_parts(parts, Bytes::from(body)))
}

/// Consumes a VM handler response without recopying its managed body bytes.
pub(super) fn serve_vm_stream_handler_response(
    response: handler::HandlerResponse,
    head_only: bool,
) -> Result<::http::Response<Bytes>, String> {
    response
        .into_http(head_only)
        .map_err(|error| error.message().to_owned())
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
pub(super) fn reload_sse_response(reload_hub: ReloadHub, head_only: bool) -> Response<ServeBody> {
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
pub(super) fn serve_response(
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
pub(super) fn boxed_body(body: Vec<u8>) -> ServeBody {
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
pub(super) fn internal_error_response(message: String) -> Response<ServeBody> {
    http::Response::builder()
        .status(500)
        .header(http::header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(boxed_body(message.into_bytes()))
        .unwrap_or_else(|_| Response::new(boxed_body(b"internal server error".to_vec())))
}

pub(super) use terlan_http_native::file_response::package_relative_path;
pub(super) use terlan_http_native::static_file::request_path as request_file_path;
