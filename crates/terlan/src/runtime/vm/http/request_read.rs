use std::io::Read;

use super::{find_header_end, parse_http1_request_headers, HTTP_BODY_LIMIT, HTTP_HEADER_LIMIT};

/// Stable failure class for one VM-owned HTTP/1 request read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmHttpRequestReadFailureKind {
    ClientClosed,
    Timeout,
    Io,
    HeaderLimit,
    BodyLimit,
    Malformed,
}

impl VmHttpRequestReadFailureKind {
    /// Returns the machine-readable runtime reason.
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::ClientClosed => "client_closed",
            Self::Timeout => "request_timeout",
            Self::Io => "request_io_error",
            Self::HeaderLimit => "header_limit_exceeded",
            Self::BodyLimit => "body_limit_exceeded",
            Self::Malformed => "malformed_request",
        }
    }
}

/// Typed VM-owned HTTP/1 request-read failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpRequestReadFailure {
    pub(crate) kind: VmHttpRequestReadFailureKind,
    pub(crate) message: String,
}

impl VmHttpRequestReadFailure {
    /// Renders a stable diagnostic while preserving the underlying detail.
    pub(crate) fn render(&self) -> String {
        format!(
            "error[vm_http_request_read]: {}: {}",
            self.kind.code(),
            self.message
        )
    }
}

/// Reads one HTTP/1 request while retaining the legacy text error contract.
pub(crate) fn read_http1_request(reader: &mut dyn Read) -> Result<::http::Request<String>, String> {
    read_http1_request_typed(reader).map_err(|failure| failure.message)
}

/// Reads one HTTP/1 request while retaining a typed terminal failure reason.
pub(crate) fn read_http1_request_typed(
    reader: &mut dyn Read,
) -> Result<::http::Request<String>, VmHttpRequestReadFailure> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let header_end = loop {
        let read = reader
            .read(&mut chunk)
            .map_err(|error| request_read_io_failure(error, "failed to read VM HTTP request"))?;
        if read == 0 {
            return Err(request_read_failure(
                VmHttpRequestReadFailureKind::ClientClosed,
                "VM HTTP request closed before headers completed",
            ));
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > HTTP_HEADER_LIMIT {
            return Err(request_read_failure(
                VmHttpRequestReadFailureKind::HeaderLimit,
                "VM HTTP request exceeded 64 KiB header limit",
            ));
        }
        if let Some(position) = find_header_end(&buffer) {
            break position;
        }
    };

    let body_start = header_end + 4;
    let (method, uri, headers, content_length) = parse_http1_request_headers(&buffer[..body_start])
        .map_err(|message| {
            request_read_failure(VmHttpRequestReadFailureKind::Malformed, message)
        })?;
    if content_length > HTTP_BODY_LIMIT {
        return Err(request_read_failure(
            VmHttpRequestReadFailureKind::BodyLimit,
            "VM HTTP request exceeded 1 MiB body limit",
        ));
    }
    while buffer.len() < body_start + content_length {
        let read = reader.read(&mut chunk).map_err(|error| {
            request_read_io_failure(error, "failed to read VM HTTP request body")
        })?;
        if read == 0 {
            return Err(request_read_failure(
                VmHttpRequestReadFailureKind::ClientClosed,
                "VM HTTP request body ended early",
            ));
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let body = String::from_utf8(buffer[body_start..body_start + content_length].to_vec())
        .map_err(|error| {
            request_read_failure(
                VmHttpRequestReadFailureKind::Malformed,
                format!("VM HTTP request body must be UTF-8: {error}"),
            )
        })?;

    let mut builder = ::http::Request::builder()
        .method(method.as_str())
        .uri(uri.as_str());
    for (name, value) in headers {
        builder = builder.header(name.as_str(), value.as_str());
    }
    builder.body(body).map_err(|error| {
        request_read_failure(
            VmHttpRequestReadFailureKind::Malformed,
            format!("failed to build parsed VM HTTP request: {error}"),
        )
    })
}

/// Builds one typed request-read failure.
fn request_read_failure(
    kind: VmHttpRequestReadFailureKind,
    message: impl Into<String>,
) -> VmHttpRequestReadFailure {
    VmHttpRequestReadFailure {
        kind,
        message: message.into(),
    }
}

/// Classifies one host read failure without exposing host-runtime policy.
fn request_read_io_failure(error: std::io::Error, context: &str) -> VmHttpRequestReadFailure {
    let kind = match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
            VmHttpRequestReadFailureKind::Timeout
        }
        _ => VmHttpRequestReadFailureKind::Io,
    };
    request_read_failure(kind, format!("{context}: {error}"))
}
