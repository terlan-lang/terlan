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
#[cfg(test)]
pub(super) fn build_http_response(
    status: u16,
    content_type: &str,
    extra_headers: &[(String, String)],
    body: &[u8],
    head_only: bool,
) -> Result<http::Response<Vec<u8>>, String> {
    build_http_response_owned_with_connection(
        status,
        content_type,
        extra_headers,
        body.to_vec(),
        head_only,
        true,
    )
}

/// Builds a VM-stream response whose protocol adapter owns connection policy.
pub(super) fn build_http_response_for_stream(
    status: u16,
    content_type: &str,
    extra_headers: &[(String, String)],
    body: &[u8],
    head_only: bool,
) -> Result<http::Response<Vec<u8>>, String> {
    build_http_response_owned_with_connection(
        status,
        content_type,
        extra_headers,
        body.to_vec(),
        head_only,
        false,
    )
}

/// Consumes one handler body without inserting and removing a connection header.
pub(super) fn build_http_response_owned_for_stream(
    status: u16,
    content_type: &str,
    extra_headers: &[(String, String)],
    body: Vec<u8>,
    head_only: bool,
) -> Result<http::Response<Vec<u8>>, String> {
    build_http_response_owned_with_connection(
        status,
        content_type,
        extra_headers,
        body,
        head_only,
        false,
    )
}

/// Preserves a managed text payload as text through the Hyper body boundary.
pub(super) fn build_http_text_response_owned_for_stream(
    status: u16,
    content_type: &str,
    extra_headers: &[(String, String)],
    body: String,
    head_only: bool,
) -> Result<http::Response<String>, String> {
    build_http_response_owned_with_connection(
        status,
        content_type,
        extra_headers,
        body,
        head_only,
        false,
    )
}

/// Transfers an immutable managed payload directly to the protocol adapter.
pub(super) fn build_http_shared_response_owned_for_stream(
    status: u16,
    content_type: &str,
    extra_headers: &[(String, String)],
    body: bytes::Bytes,
    head_only: bool,
) -> Result<http::Response<bytes::Bytes>, String> {
    build_http_response_owned_with_connection(
        status,
        content_type,
        extra_headers,
        body,
        head_only,
        false,
    )
}

fn build_http_response_owned_with_connection<B>(
    status: u16,
    content_type: &str,
    extra_headers: &[(String, String)],
    body: B,
    head_only: bool,
    connection_close: bool,
) -> Result<http::Response<B>, String>
where
    B: AsRef<[u8]> + Default,
{
    let (status, content_type, extra_headers) =
        validate_http_response_metadata(status, content_type, extra_headers)?;
    let content_length = body.as_ref().len();
    let emitted_body = if head_only { B::default() } else { body };
    let mut response = http::Response::new(emitted_body);
    *response.status_mut() = status;
    let headers = response.headers_mut();
    headers.insert(http::header::CONTENT_TYPE, content_type);
    headers.insert(
        http::header::CONTENT_LENGTH,
        http::HeaderValue::from(content_length as u64),
    );
    if !extra_headers
        .iter()
        .any(|(name, _)| name == http::header::CACHE_CONTROL)
    {
        headers.insert(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("no-cache"),
        );
    }
    headers.insert(
        http::HeaderName::from_static("x-content-type-options"),
        http::HeaderValue::from_static("nosniff"),
    );
    if connection_close {
        headers.insert(
            http::header::CONNECTION,
            http::HeaderValue::from_static("close"),
        );
    }
    for (name, value) in extra_headers {
        headers.append(name, value);
    }
    Ok(response)
}

type ValidatedHttpResponseMetadata = (
    http::StatusCode,
    http::HeaderValue,
    Vec<(http::HeaderName, http::HeaderValue)>,
);

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
) -> Result<ValidatedHttpResponseMetadata, String> {
    let status = if status == 200 {
        http::StatusCode::OK
    } else {
        http::StatusCode::from_u16(status)
            .map_err(|error| format!("HTTP status `{status}` is invalid: {error}"))?
    };
    let content_type = match content_type {
        "text/plain; charset=utf-8" => http::HeaderValue::from_static("text/plain; charset=utf-8"),
        "text/html; charset=utf-8" => http::HeaderValue::from_static("text/html; charset=utf-8"),
        "application/json; charset=utf-8" => {
            http::HeaderValue::from_static("application/json; charset=utf-8")
        }
        "application/octet-stream" => http::HeaderValue::from_static("application/octet-stream"),
        value => http::HeaderValue::from_str(value)
            .map_err(|error| format!("Content-Type value is invalid: {error}"))?,
    };
    let mut validated_headers = Vec::with_capacity(extra_headers.len());
    for (name, value) in extra_headers {
        let parsed_name = http::HeaderName::from_bytes(name.as_bytes())
            .map_err(|error| format!("HTTP header name `{name}` is invalid: {error}"))?;
        let parsed_value = http::HeaderValue::from_str(value)
            .map_err(|error| format!("HTTP header `{name}` value is invalid: {error}"))?;
        validated_headers.push((parsed_name, parsed_value));
    }
    Ok((status, content_type, validated_headers))
}
