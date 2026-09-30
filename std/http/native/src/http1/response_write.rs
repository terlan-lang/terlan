//! HTTP/1 response framing over validated `http` metadata and caller-owned writers.

use std::io::Write;

/// Validated HTTP/1 body framing selected before response-head serialization.
enum Http1BodyFraming<'a> {
    ContentLength {
        explicit: Option<&'a str>,
        body_len: usize,
    },
    Chunked,
}

/// Stable failure class for an HTTP/1 response write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResponseWriteFailureKind {
    ClientClosed,
    Timeout,
    Io,
    InvalidMetadata,
}

/// Package-owned response-write failure, independent of scheduling and sockets.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResponseWriteFailure {
    pub kind: ResponseWriteFailureKind,
    pub message: String,
}

impl std::fmt::Display for ResponseWriteFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ResponseWriteFailure {}

/// Writes one buffered response while retaining a typed terminal failure.
pub fn write_http1_response<B: AsRef<[u8]>>(
    writer: &mut dyn Write,
    response: &::http::Response<B>,
    close_connection: bool,
) -> Result<(), ResponseWriteFailure> {
    if let Some(stream) = response.extensions().get::<crate::HttpResponseChunks>() {
        write_http1_stream_head(writer, response, close_connection)?;
        let mut stream = stream.clone();
        while let Some(chunk) = stream.next_chunk() {
            write_http1_stream_chunk(writer, &chunk)?;
        }
        write_http1_stream_end(writer)?;
        return Ok(());
    }
    write_http1_body_typed(
        writer,
        response.status(),
        response.headers(),
        response.body().as_ref(),
        close_connection,
    )
}

/// Writes validated HTTP/1 metadata for a chunked response stream.
pub fn write_http1_stream_head<B>(
    writer: &mut dyn Write,
    response: &::http::Response<B>,
    close_connection: bool,
) -> Result<usize, ResponseWriteFailure> {
    let status = response.status();
    if status.is_informational()
        || status == ::http::StatusCode::NO_CONTENT
        || status == ::http::StatusCode::NOT_MODIFIED
    {
        return Err(response_write_failure(
            ResponseWriteFailureKind::InvalidMetadata,
            format!(
                "VM HTTP status {} does not permit a streamed response body",
                status.as_u16()
            ),
        ));
    }
    if response
        .headers()
        .contains_key(::http::header::CONTENT_LENGTH)
    {
        return Err(response_write_failure(
            ResponseWriteFailureKind::InvalidMetadata,
            "VM HTTP streamed response cannot declare Content-Length",
        ));
    }
    if response
        .headers()
        .contains_key(::http::header::TRANSFER_ENCODING)
    {
        return Err(response_write_failure(
            ResponseWriteFailureKind::InvalidMetadata,
            "VM HTTP streamed response owns Transfer-Encoding",
        ));
    }
    let head = build_http1_head(
        status,
        response.headers(),
        close_connection,
        Http1BodyFraming::Chunked,
    )?;
    writer.write_all(&head).map_err(|error| {
        response_write_io_failure(error, "failed to write VM HTTP response head")
    })?;
    Ok(head.len())
}

/// Writes one non-empty HTTP/1 chunk and returns its wire byte count.
pub fn write_http1_stream_chunk(
    writer: &mut dyn Write,
    chunk: &[u8],
) -> Result<usize, ResponseWriteFailure> {
    if chunk.is_empty() {
        return Err(response_write_failure(
            ResponseWriteFailureKind::InvalidMetadata,
            "VM HTTP stream chunk cannot be empty",
        ));
    }
    let mut wire = Vec::with_capacity(chunk.len().saturating_add(24));
    write!(&mut wire, "{:x}\r\n", chunk.len())
        .map_err(|error| response_write_io_failure(error, "failed to write VM HTTP chunk size"))?;
    wire.extend_from_slice(chunk);
    wire.extend_from_slice(b"\r\n");
    writer.write_all(&wire).map_err(|error| {
        response_write_io_failure(error, "failed to write VM HTTP stream chunk")
    })?;
    Ok(wire.len())
}

/// Writes the unique terminal marker for an HTTP/1 chunked response.
pub fn write_http1_stream_end(writer: &mut dyn Write) -> Result<usize, ResponseWriteFailure> {
    const END: &[u8] = b"0\r\n\r\n";
    writer
        .write_all(END)
        .map_err(|error| response_write_io_failure(error, "failed to finalize VM HTTP stream"))?;
    Ok(END.len())
}

/// Writes a buffered response using validated content-length framing.
fn write_http1_body_typed(
    writer: &mut dyn Write,
    status: ::http::StatusCode,
    headers: &::http::HeaderMap,
    body: &[u8],
    close_connection: bool,
) -> Result<(), ResponseWriteFailure> {
    if headers.contains_key(::http::header::TRANSFER_ENCODING) {
        return Err(response_write_failure(
            ResponseWriteFailureKind::InvalidMetadata,
            "VM HTTP buffered response cannot declare Transfer-Encoding",
        ));
    }
    let content_length = headers
        .get(::http::header::CONTENT_LENGTH)
        .map(|value| {
            value.to_str().map_err(|error| {
                response_write_failure(
                    ResponseWriteFailureKind::InvalidMetadata,
                    format!("VM HTTP response Content-Length is not valid text: {error}"),
                )
            })
        })
        .transpose()?;
    let head = build_http1_head(
        status,
        headers,
        close_connection,
        Http1BodyFraming::ContentLength {
            explicit: content_length,
            body_len: body.len(),
        },
    )?;
    writer.write_all(&head).map_err(|error| {
        response_write_io_failure(error, "failed to write VM HTTP response head")
    })?;
    writer
        .write_all(body)
        .map_err(|error| response_write_io_failure(error, "failed to write VM HTTP body"))
}

fn invalid_metadata(message: impl Into<String>) -> ResponseWriteFailure {
    response_write_failure(ResponseWriteFailureKind::InvalidMetadata, message)
}

/// Builds one typed response-write failure.
fn response_write_failure(
    kind: ResponseWriteFailureKind,
    message: impl Into<String>,
) -> ResponseWriteFailure {
    ResponseWriteFailure {
        kind,
        message: message.into(),
    }
}

/// Classifies one host write failure without exposing host-runtime policy.
fn response_write_io_failure(error: std::io::Error, context: &str) -> ResponseWriteFailure {
    let kind = match error.kind() {
        std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
            ResponseWriteFailureKind::Timeout
        }
        std::io::ErrorKind::BrokenPipe
        | std::io::ErrorKind::ConnectionAborted
        | std::io::ErrorKind::ConnectionReset
        | std::io::ErrorKind::NotConnected
        | std::io::ErrorKind::UnexpectedEof => ResponseWriteFailureKind::ClientClosed,
        _ => ResponseWriteFailureKind::Io,
    };
    response_write_failure(kind, format!("{context}: {error}"))
}

/// Serializes one HTTP/1 status line and normalized response headers.
fn build_http1_head(
    status: ::http::StatusCode,
    headers: &::http::HeaderMap,
    close_connection: bool,
    framing: Http1BodyFraming<'_>,
) -> Result<Vec<u8>, ResponseWriteFailure> {
    let reason = status.canonical_reason().unwrap_or("");
    let connection = connection_header(headers, close_connection)?;
    let mut head = Vec::with_capacity(128 + headers.len() * 32);
    write!(&mut head, "HTTP/1.1 {} {}\r\n", status.as_u16(), reason).map_err(|error| {
        invalid_metadata(format!("failed to write VM HTTP status line: {error}"))
    })?;
    match framing {
        Http1BodyFraming::ContentLength { explicit, body_len } => {
            head.extend_from_slice(b"Content-Length: ");
            if let Some(explicit) = explicit {
                head.extend_from_slice(explicit.as_bytes());
            } else {
                write!(&mut head, "{body_len}").map_err(|error| {
                    invalid_metadata(format!("failed to write VM HTTP Content-Length: {error}"))
                })?;
            }
            head.extend_from_slice(b"\r\n");
        }
        Http1BodyFraming::Chunked => head.extend_from_slice(b"Transfer-Encoding: chunked\r\n"),
    }
    write!(&mut head, "Connection: {connection}\r\n").map_err(|error| {
        invalid_metadata(format!(
            "failed to write VM HTTP Connection header: {error}"
        ))
    })?;
    append_response_headers(&mut head, headers)?;
    head.extend_from_slice(b"\r\n");
    Ok(head)
}

/// Selects an explicit or caller-derived HTTP/1 connection header value.
fn connection_header(
    headers: &::http::HeaderMap,
    close_connection: bool,
) -> Result<&str, ResponseWriteFailure> {
    headers
        .get(::http::header::CONNECTION)
        .map(|value| {
            value.to_str().map_err(|error| {
                invalid_metadata(format!(
                    "VM HTTP response Connection is not valid text: {error}"
                ))
            })
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

/// Appends caller headers while excluding framing fields owned by this codec.
fn append_response_headers(
    head: &mut Vec<u8>,
    headers: &::http::HeaderMap,
) -> Result<(), ResponseWriteFailure> {
    for (name, value) in headers {
        if name == ::http::header::CONTENT_LENGTH
            || name == ::http::header::CONNECTION
            || name == ::http::header::TRANSFER_ENCODING
        {
            continue;
        }
        let value = value.to_str().map_err(|error| {
            invalid_metadata(format!(
                "VM HTTP response header `{name}` is invalid: {error}"
            ))
        })?;
        write!(head, "{}: {}\r\n", name.as_str(), value).map_err(|error| {
            invalid_metadata(format!("failed to write VM HTTP header `{name}`: {error}"))
        })?;
    }
    Ok(())
}
