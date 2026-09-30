//! Shared server response construction over maintained HTTP metadata types.

use crate::response_headers::parse_response_header;
use crate::HttpError;

/// Builds a response while preserving owned body storage and HEAD content length.
/// The protocol adapter selects whether to emit an explicit Connection: close.
/// Handler-provided extra headers must first pass `validate_response_header`;
/// this lower-level builder also accepts trusted transport metadata.
pub fn build_http_response<B>(
    status: u16,
    content_type: &str,
    extra_headers: &[(String, String)],
    body: B,
    head_only: bool,
    connection_close: bool,
) -> Result<http::Response<B>, HttpError>
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

/// Parses every metadata field before transferring the body into a response.
fn validate_http_response_metadata(
    status: u16,
    content_type: &str,
    extra_headers: &[(String, String)],
) -> Result<ValidatedHttpResponseMetadata, HttpError> {
    let status = if status == 200 {
        http::StatusCode::OK
    } else {
        http::StatusCode::from_u16(status).map_err(|error| {
            HttpError::new(
                "http.response.invalid_status",
                format!("HTTP status `{status}` is invalid: {error}"),
                500,
            )
        })?
    };
    let content_type = match content_type {
        "text/plain; charset=utf-8" => http::HeaderValue::from_static("text/plain; charset=utf-8"),
        "text/html; charset=utf-8" => http::HeaderValue::from_static("text/html; charset=utf-8"),
        "application/json; charset=utf-8" => {
            http::HeaderValue::from_static("application/json; charset=utf-8")
        }
        "application/octet-stream" => http::HeaderValue::from_static("application/octet-stream"),
        value => http::HeaderValue::from_str(value).map_err(|error| {
            HttpError::new(
                "http.response.invalid_content_type",
                format!("Content-Type value is invalid: {error}"),
                500,
            )
        })?,
    };
    let mut validated_headers = Vec::with_capacity(extra_headers.len());
    for (name, value) in extra_headers {
        let (parsed_name, parsed_value) = parse_response_header(name, value)?;
        validated_headers.push((parsed_name, parsed_value));
    }
    Ok((status, content_type, validated_headers))
}
