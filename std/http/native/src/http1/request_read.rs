use std::io::Read;

use super::{attach_body, find_header_end, header_limit, parse_head, HTTP_HEADER_LIMIT};

/// Stable failure class for a buffered HTTP/1 request read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestReadFailureKind {
    ClientClosed,
    Timeout,
    Io,
    HeaderLimit,
    BodyLimit,
    Malformed,
}

/// Package-owned decode failure, independent of actor and socket implementations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RequestReadFailure {
    pub kind: RequestReadFailureKind,
    pub message: String,
}

impl std::fmt::Display for RequestReadFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for RequestReadFailure {}

/// Reads one bounded request from a one-shot stream with typed failures.
/// Reusable connections use the incremental buffer API to retain pipelined bytes.
pub fn read_http1_request(
    reader: &mut dyn Read,
) -> Result<http::Request<String>, RequestReadFailure> {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];
    let head = loop {
        let read = read_chunk(reader, &mut chunk, "failed to read VM HTTP request")?;
        if read == 0 {
            return Err(closed("VM HTTP request closed before headers completed"));
        }
        buffer.extend_from_slice(&chunk[..read]);
        if find_header_end(&buffer).is_some() {
            break parse_head(&buffer)?;
        }
        if buffer.len() > HTTP_HEADER_LIMIT {
            return Err(header_limit());
        }
    };
    let complete_len = head.head_length + head.content_length;
    while buffer.len() < complete_len {
        let remaining = (complete_len - buffer.len()).min(chunk.len());
        let read = read_chunk(
            reader,
            &mut chunk[..remaining],
            "failed to read VM HTTP request body",
        )?;
        if read == 0 {
            return Err(closed("VM HTTP request body ended early"));
        }
        buffer.extend_from_slice(&chunk[..read]);
    }
    let body = &buffer[head.head_length..complete_len];
    attach_body(head, body)
}

fn closed(message: &str) -> RequestReadFailure {
    RequestReadFailure {
        kind: RequestReadFailureKind::ClientClosed,
        message: message.to_string(),
    }
}

fn read_chunk(
    reader: &mut dyn Read,
    chunk: &mut [u8],
    context: &str,
) -> Result<usize, RequestReadFailure> {
    loop {
        match reader.read(chunk) {
            Ok(read) => return Ok(read),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => {
                let kind = match error.kind() {
                    std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock => {
                        RequestReadFailureKind::Timeout
                    }
                    _ => RequestReadFailureKind::Io,
                };
                return Err(RequestReadFailure {
                    kind,
                    message: format!("{context}: {error}"),
                });
            }
        }
    }
}
