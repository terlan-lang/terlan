//! Buffered HTTP/1 decoding via httparse and live connections via Hyper.
//! Callers retain transport resources and scheduling ownership.

mod connection;
mod request_read;
mod response_write;
#[cfg(test)]
mod response_write_test;
#[cfg(test)]
mod tests;

pub use connection::serve_connection;
pub use request_read::{read_http1_request, RequestReadFailure, RequestReadFailureKind};
pub use response_write::{
    write_http1_response, write_http1_stream_chunk, write_http1_stream_end,
    write_http1_stream_head, ResponseWriteFailure, ResponseWriteFailureKind,
};

/// Maximum encoded request-head size, including its terminator.
pub const HTTP_HEADER_LIMIT: usize = 64 * 1024;
/// Maximum buffered request-body size.
pub const HTTP_BODY_LIMIT: usize = 1024 * 1024;

/// Parsed metadata ready for attachment to its decoded body.
#[derive(Debug)]
pub struct RequestHead {
    pub parts: http::request::Parts,
    pub content_length: usize,
    pub head_length: usize,
}

/// Parses a complete request head, rejecting unsupported or ambiguous framing.
pub fn parse_http1_request_headers(bytes: &[u8]) -> Result<RequestHead, RequestReadFailure> {
    parse_head(bytes)
}

fn malformed(message: impl Into<String>) -> RequestReadFailure {
    RequestReadFailure {
        kind: RequestReadFailureKind::Malformed,
        message: message.into(),
    }
}

fn header_limit() -> RequestReadFailure {
    RequestReadFailure {
        kind: RequestReadFailureKind::HeaderLimit,
        message: "VM HTTP request exceeded 64 KiB header limit".to_string(),
    }
}

fn parse_head(bytes: &[u8]) -> Result<RequestHead, RequestReadFailure> {
    let head_length = find_header_end(bytes).map(|end| end + 4);
    if head_length.unwrap_or(bytes.len()) > HTTP_HEADER_LIMIT {
        return Err(header_limit());
    }
    let mut headers = [httparse::EMPTY_HEADER; 32];
    let mut request = httparse::Request::new(&mut headers);
    let head_length = match request
        .parse(bytes)
        .map_err(|error| malformed(format!("failed to parse VM HTTP request: {error}")))?
    {
        httparse::Status::Complete(length) => length,
        httparse::Status::Partial => {
            return Err(malformed("VM HTTP parser reported partial headers"))
        }
    };
    if head_length > HTTP_HEADER_LIMIT {
        return Err(header_limit());
    }
    let method = request
        .method
        .ok_or_else(|| malformed("VM HTTP request missing method"))?;
    let uri = request
        .path
        .ok_or_else(|| malformed("VM HTTP request missing path"))?;
    let version = match request.version {
        Some(0) => http::Version::HTTP_10,
        Some(1) => http::Version::HTTP_11,
        _ => return Err(malformed("VM HTTP request has unsupported version")),
    };
    let mut builder = http::Request::builder()
        .method(method)
        .uri(uri)
        .version(version);
    let mut content_length = None;
    for header in request.headers.iter() {
        let value = std::str::from_utf8(header.value).map_err(|error| {
            malformed(format!(
                "VM HTTP header `{}` is not UTF-8: {error}",
                header.name
            ))
        })?;
        if header.name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(malformed(
                "VM HTTP buffered request does not support Transfer-Encoding",
            ));
        }
        if header.name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err(malformed(
                    "VM HTTP request has multiple Content-Length headers",
                ));
            }
            if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
                return Err(malformed(format!(
                    "VM HTTP Content-Length `{value}` is invalid"
                )));
            }
            content_length = Some(value.parse::<usize>().map_err(|error| {
                malformed(format!(
                    "VM HTTP Content-Length `{value}` is invalid: {error}"
                ))
            })?);
        }
        builder = builder.header(header.name, value);
    }
    let content_length = content_length.unwrap_or(0);
    if content_length > HTTP_BODY_LIMIT {
        return Err(RequestReadFailure {
            kind: RequestReadFailureKind::BodyLimit,
            message: "VM HTTP request exceeded 1 MiB body limit".to_string(),
        });
    }
    let (parts, ()) = builder
        .body(())
        .map_err(|error| malformed(format!("failed to build parsed VM HTTP request: {error}")))?
        .into_parts();
    Ok(RequestHead {
        parts,
        content_length,
        head_length,
    })
}

fn attach_body(
    head: RequestHead,
    body: &[u8],
) -> Result<http::Request<String>, RequestReadFailure> {
    let body = String::from_utf8(body.to_vec())
        .map_err(|error| malformed(format!("VM HTTP request body must be UTF-8: {error}")))?;
    Ok(http::Request::from_parts(head.parts, body))
}

/// Returns one complete request and its consumed byte count, retaining pipeline boundaries.
pub fn try_parse_http1_request_buffer(
    buffer: &[u8],
) -> Result<Option<(http::Request<String>, usize)>, RequestReadFailure> {
    if find_header_end(buffer).is_none() {
        return if buffer.len() > HTTP_HEADER_LIMIT {
            Err(header_limit())
        } else {
            Ok(None)
        };
    }
    let head = parse_head(buffer)?;
    // Both lengths have already been bounded independently.
    let complete_len = head.head_length + head.content_length;
    if buffer.len() < complete_len {
        return Ok(None);
    }
    let body = &buffer[head.head_length..complete_len];
    attach_body(head, body).map(|request| Some((request, complete_len)))
}

/// Classifies EOF for a buffered exchange without consuming bytes.
pub fn incomplete_http1_request_error(buffer: &[u8]) -> String {
    if let Ok(head) = parse_head(buffer) {
        if buffer.len() < head.head_length + head.content_length {
            return "VM HTTP request body ended early".to_string();
        }
    }
    "VM HTTP request closed before headers completed".to_string()
}

/// Finds the CRLF request/response head terminator.
pub fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

/// Applies HTTP/1 connection policy across all Connection field values.
pub fn request_wants_http1_close<B>(request: &http::Request<B>) -> bool {
    let mut keep_alive = false;
    for value in request.headers().get_all(http::header::CONNECTION) {
        let Ok(value) = value.to_str() else {
            return true;
        };
        for token in value.split(',').map(str::trim) {
            if token.eq_ignore_ascii_case("close") {
                return true;
            }
            keep_alive |= token.eq_ignore_ascii_case("keep-alive");
        }
    }
    request.version() == http::Version::HTTP_10 && !keep_alive
}
