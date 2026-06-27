use crate::terlan_native::http as native_http;

use super::types::WebPackageResponseHeader;

/// HTTP response returned by a BEAM-backed handler.
///
/// Inputs:
/// - Produced by the handler runner after parsing the stable handler ABI.
///
/// Output:
/// - Status, content type, and byte body ready for the local HTTP writer.
///
/// Transformation:
/// - Keeps BEAM process output separate from socket writing so handler
///   execution can be tested without binding a server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BeamHandlerResponse {
    pub(crate) status: u16,
    pub(crate) content_type: String,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: Vec<u8>,
}

impl BeamHandlerResponse {
    /// Converts a Rust-native HTTP adapter response into the server wire shape.
    ///
    /// Inputs:
    /// - `response`: Rust-native `std.http.Response` backing value.
    ///
    /// Output:
    /// - Internal handler response accepted by the local HTTP writer.
    /// - Stable serve-handler error when the status cannot be represented as
    ///   an HTTP status code.
    ///
    /// Transformation:
    /// - Copies native response status, content type, headers, and UTF-8 body
    ///   into the bridge response without exposing the BEAM tuple ABI to source
    ///   code.
    pub(crate) fn from_native_response(response: &native_http::Response) -> Result<Self, String> {
        let status = response.status_code();
        if !(100..=599).contains(&status) {
            return Err(format!(
                "error[serve_handler]: native HTTP response status `{status}` is outside HTTP range"
            ));
        }
        let headers = response
            .headers()
            .iter()
            .map(|(name, value)| validate_response_header(name, value))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            status: status as u16,
            content_type: response.content_type().to_string(),
            headers,
            body: response.body().as_bytes().to_vec(),
        })
    }
}

/// Converts static response headers into the HTTP writer tuple shape.
///
/// Inputs:
/// - `headers`: manifest header objects already accepted by package
///   validation.
///
/// Output:
/// - Header name/value tuples used by the local HTTP writer.
///
/// Transformation:
/// - Performs one final validation pass before socket emission so hand-authored
///   manifests still cannot bypass the response-header safety boundary.
pub(crate) fn static_response_header_tuples(
    headers: &[WebPackageResponseHeader],
) -> Result<Vec<(String, String)>, String> {
    headers
        .iter()
        .map(|header| validate_response_header(&header.name, &header.value))
        .collect()
}

/// Validates one response header accepted from a handler boundary.
///
/// Inputs:
/// - `name`: handler-provided header name.
/// - `value`: handler-provided header value.
///
/// Output:
/// - Sanitized owned name/value pair when the header can be emitted.
/// - Stable `error[serve_handler]` diagnostic otherwise.
///
/// Transformation:
/// - Rejects empty names, non-token characters, CR/LF injection, and
///   server-owned headers whose values are produced by the local HTTP writer.
pub(super) fn validate_response_header(
    name: &str,
    value: &str,
) -> Result<(String, String), String> {
    if name.is_empty() || !name.bytes().all(is_http_token_byte) {
        return Err(format!(
            "error[serve_handler]: response header name `{name}` is not a valid HTTP token"
        ));
    }
    if is_server_owned_response_header(name) {
        return Err(format!(
            "error[serve_handler]: response header `{name}` is owned by the server bridge"
        ));
    }
    if value.bytes().any(|byte| byte == b'\r' || byte == b'\n') {
        return Err(format!(
            "error[serve_handler]: response header `{name}` contains a line break"
        ));
    }
    Ok((name.to_string(), value.to_string()))
}

/// Returns whether a byte is allowed inside an HTTP token.
///
/// Inputs:
/// - `byte`: candidate header-name byte.
///
/// Output:
/// - `true` when the byte is accepted by the conservative HTTP token subset.
///
/// Transformation:
/// - Implements the RFC token character set needed for response header names.
fn is_http_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

/// Returns whether a response header is controlled by the server bridge.
///
/// Inputs:
/// - `name`: handler-provided header name.
///
/// Output:
/// - `true` when the bridge renders the header itself.
///
/// Transformation:
/// - Keeps handler metadata from conflicting with the local server's required
///   HTTP framing and local-development safety headers.
fn is_server_owned_response_header(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "content-type"
            | "content-length"
            | "connection"
            | "cache-control"
            | "x-content-type-options"
    )
}
