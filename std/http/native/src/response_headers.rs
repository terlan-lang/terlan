//! Maintained header parsing with the package's handler-admission policy.

use crate::HttpError;

/// Validates handler metadata without cloning the caller's owned strings.
/// Content type and framing fields are supplied separately by the response adapter.
pub fn validate_response_header(name: &str, value: &str) -> Result<(), HttpError> {
    let parsed_name = http::HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
        invalid_header(format!(
            "response header name `{name}` is not a valid HTTP token"
        ))
    })?;
    if matches!(
        parsed_name.as_str(),
        "content-type" | "content-length" | "connection" | "transfer-encoding" | "trailer"
    ) {
        return Err(invalid_header(format!(
            "response header `{name}` is owned by the server bridge"
        )));
    }
    http::HeaderValue::from_str(value).map_err(|error| {
        if value.contains(['\r', '\n']) {
            invalid_header(format!("response header `{name}` contains a line break"))
        } else {
            invalid_header(format!(
                "response header `{name}` value is invalid: {error}"
            ))
        }
    })?;
    Ok(())
}

pub(crate) fn parse_response_header(
    name: &str,
    value: &str,
) -> Result<(http::HeaderName, http::HeaderValue), HttpError> {
    let name_value = http::HeaderName::from_bytes(name.as_bytes()).map_err(|error| {
        invalid_header(format!("HTTP header name `{name}` is invalid: {error}"))
    })?;
    let value = http::HeaderValue::from_str(value).map_err(|error| {
        invalid_header(format!("HTTP header `{name}` value is invalid: {error}"))
    })?;
    Ok((name_value, value))
}

fn invalid_header(message: String) -> HttpError {
    HttpError::new("http.response.invalid_header", message, 500)
}
