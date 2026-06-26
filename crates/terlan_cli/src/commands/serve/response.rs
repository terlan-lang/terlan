use super::RELOAD_ENDPOINT;

/// Injects local live-reload wiring into one HTML document.
///
/// Inputs:
/// - `html`: served HTML response text.
///
/// Output:
/// - HTML response text with a local reload script inserted.
///
/// Transformation:
/// - Preserves documents that already reference the reload endpoint, inserts
///   before `</body>` when present, and appends otherwise. The packaged file on
///   disk is never modified.
pub(super) fn inject_reload_script(html: &str) -> String {
    if html.contains(RELOAD_ENDPOINT) {
        return html.to_string();
    }
    let script = format!(
        "<script>(()=>{{const es=new EventSource('{}');es.addEventListener('reload',()=>location.reload());}})();</script>",
        RELOAD_ENDPOINT
    );
    if let Some(index) = html.rfind("</body>") {
        let mut output = String::with_capacity(html.len() + script.len());
        output.push_str(&html[..index]);
        output.push_str(&script);
        output.push_str(&html[index..]);
        output
    } else {
        let mut output = String::with_capacity(html.len() + script.len());
        output.push_str(html);
        output.push_str(&script);
        output
    }
}

/// Builds a typed Rust HTTP response for the serve runtime.
///
/// Inputs:
/// - `status`: numeric response status.
/// - `content_type`: content type header value.
/// - `extra_headers`: validated handler or manifest headers.
/// - `body`: response body bytes.
/// - `head_only`: whether the emitted response body should be empty.
///
/// Output:
/// - `Ok(http::Response<Vec<u8>>)` when metadata passes Rust HTTP validation.
/// - `Err(message)` when status or headers cannot be represented.
///
/// Transformation:
/// - Uses Rust `http` request/response primitives as the shared boundary
///   between Terlan route selection and the Hyper server implementation.
pub(super) fn build_http_response(
    status: u16,
    content_type: &str,
    extra_headers: &[(String, String)],
    body: &[u8],
    head_only: bool,
) -> Result<http::Response<Vec<u8>>, String> {
    validate_http_response_metadata(status, content_type, extra_headers)?;
    let mut builder = http::Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, content_type)
        .header(http::header::CONTENT_LENGTH, body.len().to_string())
        .header(http::header::CACHE_CONTROL, "no-cache")
        .header("x-content-type-options", "nosniff")
        .header(http::header::CONNECTION, "close");
    for (name, value) in extra_headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    let emitted_body = if head_only { Vec::new() } else { body.to_vec() };
    builder
        .body(emitted_body)
        .map_err(|error| format!("HTTP response cannot be built: {error}"))
}

/// Validates HTTP response metadata before response construction.
///
/// Inputs:
/// - `status`: numeric status code selected by routing or handler code.
/// - `content_type`: response content type header value.
/// - `extra_headers`: handler or manifest response headers.
///
/// Output:
/// - `Ok(())` when metadata can be represented by Rust HTTP types.
/// - `Err(message)` when the metadata is invalid.
///
/// Transformation:
/// - Uses the maintained Rust `http` crate for status and header validation so
///   every serve response follows the same boundary consumed by Hyper.
fn validate_http_response_metadata(
    status: u16,
    content_type: &str,
    extra_headers: &[(String, String)],
) -> Result<(), String> {
    http::StatusCode::from_u16(status)
        .map_err(|error| format!("HTTP status `{status}` is invalid: {error}"))?;
    http::HeaderValue::from_str(content_type)
        .map_err(|error| format!("Content-Type value is invalid: {error}"))?;
    for (name, value) in extra_headers {
        http::HeaderName::from_bytes(name.as_bytes())
            .map_err(|error| format!("HTTP header name `{name}` is invalid: {error}"))?;
        http::HeaderValue::from_str(value)
            .map_err(|error| format!("HTTP header `{name}` value is invalid: {error}"))?;
    }
    Ok(())
}
