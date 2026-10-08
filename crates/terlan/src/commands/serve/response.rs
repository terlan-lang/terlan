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
    terlan_http_native::build_server_response(
        status,
        content_type,
        extra_headers,
        body.to_vec(),
        head_only,
        true,
    )
    .map_err(|error| error.message().to_owned())
}

/// Builds a VM-stream response whose protocol adapter owns connection policy.
pub(super) fn build_http_response_for_stream(
    status: u16,
    content_type: &str,
    extra_headers: &[(String, String)],
    body: &[u8],
    head_only: bool,
) -> Result<http::Response<Vec<u8>>, String> {
    terlan_http_native::build_server_response(
        status,
        content_type,
        extra_headers,
        body.to_vec(),
        head_only,
        false,
    )
    .map_err(|error| error.message().to_owned())
}
