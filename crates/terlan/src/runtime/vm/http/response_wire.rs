use std::io::Write;

/// Validated HTTP/1 body framing selected before response-head serialization.
enum VmHttp1BodyFraming<'a> {
    ContentLength {
        explicit: Option<&'a str>,
        body_len: usize,
    },
    Chunked,
}

/// Stable failure class for one buffered VM HTTP/1 response write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmHttpResponseWriteFailureKind {
    ClientClosed,
    Timeout,
    Io,
    InvalidMetadata,
}

impl VmHttpResponseWriteFailureKind {
    /// Returns the machine-readable runtime reason.
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::ClientClosed => "client_closed_during_response_write",
            Self::Timeout => "response_write_timeout",
            Self::Io => "response_write_io_error",
            Self::InvalidMetadata => "invalid_response_metadata",
        }
    }
}

/// Typed VM-owned buffered response-write failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpResponseWriteFailure {
    pub(crate) kind: VmHttpResponseWriteFailureKind,
    pub(crate) message: String,
}

impl VmHttpResponseWriteFailure {
    /// Renders a stable diagnostic while preserving the writer detail.
    pub(crate) fn render(&self) -> String {
        format!(
            "error[vm_http_response_write]: {}: {}",
            self.kind.code(),
            self.message
        )
    }
}

/// Writes one HTTP/1 response from validated metadata and a UTF-8 body.
pub(crate) fn write_http1_response(
    writer: &mut dyn Write,
    response: &::http::Response<String>,
    close_connection: bool,
) -> Result<(), String> {
    write_http1_response_typed(writer, response, close_connection)
        .map_err(|failure| failure.message)
}

/// Writes one buffered response while retaining a typed terminal failure.
pub(crate) fn write_http1_response_typed(
    writer: &mut dyn Write,
    response: &::http::Response<String>,
    close_connection: bool,
) -> Result<(), VmHttpResponseWriteFailure> {
    write_http1_body_typed(
        writer,
        response.status(),
        response.headers(),
        response.body().as_bytes(),
        close_connection,
    )
}

/// Writes one HTTP/1 response while preserving exact binary body bytes.
pub(crate) fn write_http1_bytes_response(
    writer: &mut dyn Write,
    response: &::http::Response<Vec<u8>>,
    close_connection: bool,
) -> Result<(), String> {
    write_http1_body_typed(
        writer,
        response.status(),
        response.headers(),
        response.body(),
        close_connection,
    )
    .map_err(|failure| failure.message)
}

/// Writes validated HTTP/1 metadata for a chunked response stream.
pub(crate) fn write_http1_stream_head(
    writer: &mut dyn Write,
    response: &::http::Response<()>,
    close_connection: bool,
) -> Result<usize, String> {
    let status = response.status();
    if status.is_informational()
        || status == ::http::StatusCode::NO_CONTENT
        || status == ::http::StatusCode::NOT_MODIFIED
    {
        return Err(format!(
            "VM HTTP status {} does not permit a streamed response body",
            status.as_u16()
        ));
    }
    if response
        .headers()
        .contains_key(::http::header::CONTENT_LENGTH)
    {
        return Err("VM HTTP streamed response cannot declare Content-Length".to_string());
    }
    if response
        .headers()
        .contains_key(::http::header::TRANSFER_ENCODING)
    {
        return Err("VM HTTP streamed response owns Transfer-Encoding".to_string());
    }
    let head = build_http1_head(
        status,
        response.headers(),
        close_connection,
        VmHttp1BodyFraming::Chunked,
    )?;
    writer
        .write_all(&head)
        .map_err(|error| format!("failed to write VM HTTP response head: {error}"))?;
    Ok(head.len())
}

/// Writes one non-empty HTTP/1 chunk and returns its wire byte count.
pub(crate) fn write_http1_stream_chunk(
    writer: &mut dyn Write,
    chunk: &[u8],
) -> Result<usize, String> {
    if chunk.is_empty() {
        return Err("VM HTTP stream chunk cannot be empty".to_string());
    }
    let mut wire = Vec::with_capacity(chunk.len().saturating_add(24));
    write!(&mut wire, "{:x}\r\n", chunk.len())
        .map_err(|error| format!("failed to write VM HTTP chunk size: {error}"))?;
    wire.extend_from_slice(chunk);
    wire.extend_from_slice(b"\r\n");
    writer
        .write_all(&wire)
        .map_err(|error| format!("failed to write VM HTTP stream chunk: {error}"))?;
    Ok(wire.len())
}

/// Writes the unique terminal marker for an HTTP/1 chunked response.
pub(crate) fn write_http1_stream_end(writer: &mut dyn Write) -> Result<usize, String> {
    const END: &[u8] = b"0\r\n\r\n";
    writer
        .write_all(END)
        .map_err(|error| format!("failed to finalize VM HTTP stream: {error}"))?;
    Ok(END.len())
}

/// Writes a buffered response using validated content-length framing.
fn write_http1_body_typed(
    writer: &mut dyn Write,
    status: ::http::StatusCode,
    headers: &::http::HeaderMap,
    body: &[u8],
    close_connection: bool,
) -> Result<(), VmHttpResponseWriteFailure> {
    if headers.contains_key(::http::header::TRANSFER_ENCODING) {
        return Err(response_write_failure(
            VmHttpResponseWriteFailureKind::InvalidMetadata,
            "VM HTTP buffered response cannot declare Transfer-Encoding",
        ));
    }
    let content_length = headers
        .get(::http::header::CONTENT_LENGTH)
        .map(|value| {
            value.to_str().map_err(|error| {
                response_write_failure(
                    VmHttpResponseWriteFailureKind::InvalidMetadata,
                    format!("VM HTTP response Content-Length is not valid text: {error}"),
                )
            })
        })
        .transpose()?;
    let head = build_http1_head(
        status,
        headers,
        close_connection,
        VmHttp1BodyFraming::ContentLength {
            explicit: content_length,
            body_len: body.len(),
        },
    )
    .map_err(|message| {
        response_write_failure(VmHttpResponseWriteFailureKind::InvalidMetadata, message)
    })?;
    writer.write_all(&head).map_err(|error| {
        response_write_io_failure(error, "failed to write VM HTTP response head")
    })?;
    writer
        .write_all(body)
        .map_err(|error| response_write_io_failure(error, "failed to write VM HTTP body"))
}

/// Builds one typed response-write failure.
fn response_write_failure(
    kind: VmHttpResponseWriteFailureKind,
    message: impl Into<String>,
) -> VmHttpResponseWriteFailure {
    VmHttpResponseWriteFailure {
        kind,
        message: message.into(),
    }
}

/// Classifies one host write failure without exposing host-runtime policy.
fn response_write_io_failure(error: std::io::Error, context: &str) -> VmHttpResponseWriteFailure {
    let kind = match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
            VmHttpResponseWriteFailureKind::Timeout
        }
        std::io::ErrorKind::BrokenPipe
        | std::io::ErrorKind::ConnectionAborted
        | std::io::ErrorKind::ConnectionReset
        | std::io::ErrorKind::NotConnected
        | std::io::ErrorKind::UnexpectedEof => VmHttpResponseWriteFailureKind::ClientClosed,
        _ => VmHttpResponseWriteFailureKind::Io,
    };
    response_write_failure(kind, format!("{context}: {error}"))
}

/// Serializes one HTTP/1 status line and normalized response headers.
fn build_http1_head(
    status: ::http::StatusCode,
    headers: &::http::HeaderMap,
    close_connection: bool,
    framing: VmHttp1BodyFraming<'_>,
) -> Result<Vec<u8>, String> {
    let reason = status.canonical_reason().unwrap_or("");
    let connection = connection_header(headers, close_connection)?;
    let mut head = Vec::with_capacity(128 + headers.len() * 32);
    write!(&mut head, "HTTP/1.1 {} {}\r\n", status.as_u16(), reason)
        .map_err(|error| format!("failed to write VM HTTP status line: {error}"))?;
    match framing {
        VmHttp1BodyFraming::ContentLength { explicit, body_len } => {
            head.extend_from_slice(b"Content-Length: ");
            if let Some(explicit) = explicit {
                head.extend_from_slice(explicit.as_bytes());
            } else {
                write!(&mut head, "{body_len}")
                    .map_err(|error| format!("failed to write VM HTTP Content-Length: {error}"))?;
            }
            head.extend_from_slice(b"\r\n");
        }
        VmHttp1BodyFraming::Chunked => head.extend_from_slice(b"Transfer-Encoding: chunked\r\n"),
    }
    write!(&mut head, "Connection: {connection}\r\n")
        .map_err(|error| format!("failed to write VM HTTP Connection header: {error}"))?;
    append_response_headers(&mut head, headers)?;
    head.extend_from_slice(b"\r\n");
    Ok(head)
}

/// Selects an explicit or VM-derived HTTP/1 connection header value.
fn connection_header(headers: &::http::HeaderMap, close_connection: bool) -> Result<&str, String> {
    headers
        .get(::http::header::CONNECTION)
        .map(|value| {
            value
                .to_str()
                .map_err(|error| format!("VM HTTP response Connection is not valid text: {error}"))
        })
        .transpose()
        .map(|value| {
            value.unwrap_or(if close_connection {
                "close"
            } else {
                "keep-alive"
            })
        })
}

/// Appends caller headers while excluding framing fields owned by the VM.
fn append_response_headers(head: &mut Vec<u8>, headers: &::http::HeaderMap) -> Result<(), String> {
    for (name, value) in headers {
        if name == ::http::header::CONTENT_LENGTH
            || name == ::http::header::CONNECTION
            || name == ::http::header::TRANSFER_ENCODING
        {
            continue;
        }
        let value = value
            .to_str()
            .map_err(|error| format!("VM HTTP response header `{name}` is invalid: {error}"))?;
        write!(head, "{}: {}\r\n", name.as_str(), value)
            .map_err(|error| format!("failed to write VM HTTP header `{name}`: {error}"))?;
    }
    Ok(())
}
