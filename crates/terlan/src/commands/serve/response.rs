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
    terlan_http_native::build_http_response(
        status,
        content_type,
        extra_headers,
        body,
        head_only,
        connection_close,
    )
    .map_err(|error| error.message().to_owned())
}
