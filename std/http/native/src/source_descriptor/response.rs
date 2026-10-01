//! Admission of source-owned responses without VM layout knowledge or host I/O.

use super::{error, Result};
use crate::{validate_response_header, HttpResponseChunks};
use bytes::Bytes;
use terlan_runtime_abi::{native_record, DescriptorValue, NativeValue, OwnedDescriptor as D};

/// Validated source metadata ready for a transport adapter.
#[derive(Debug)]
pub struct SourceResponse {
    pub status: u16,
    pub content_type: String,
    pub headers: Vec<(String, String)>,
    pub body: SourceResponseBody,
}

/// File paths remain untrusted requests; the host must authorize and open them.
#[derive(Debug)]
pub enum SourceResponseBody {
    Text(String),
    File(String),
    Stream(HttpResponseChunks),
}

/// Projects cached manifest output into the source contract for middleware.
/// This is value construction, not admission; `response` validates the result
/// before transport even when a host has supplied malformed cached metadata.
pub fn cached_response(
    status: u16,
    content_type: String,
    payload: String,
    headers: Vec<(String, String)>,
) -> NativeValue {
    let kind = if content_type == "text/html; charset=utf-8" {
        1_i64
    } else {
        0
    };
    let status = i64::from(status);
    let chunks = Vec::<String>::new();
    let chunk_size = 0_i64;
    let max_pending_writes = 0_i64;
    native_record!(Response, {kind, payload, status, content_type, headers, chunks, chunk_size, max_pending_writes})
}

/// Consumes an executed Response, preserving body/header allocations and order.
/// No filesystem or callback access is performed during admission.
pub fn response<V: DescriptorValue>(value: V) -> Result<SourceResponse> {
    let D::Record(name, mut fields) = value.into_descriptor() else {
        return Err(error("expected source Response record"));
    };
    fields.sort_unstable_by(|left, right| left.0.cmp(&right.0));
    if name != "Response"
        || !fields.iter().map(|(name, _)| name.as_str()).eq([
            "chunk_size",
            "chunks",
            "content_type",
            "headers",
            "kind",
            "max_pending_writes",
            "payload",
            "status",
        ])
    {
        return Err(error("malformed source Response record"));
    }
    let mut fields = fields.into_iter().map(|(_, value)| value.into_descriptor());
    let (
        Some(D::Int(chunk_size)),
        Some(D::List(chunks)),
        Some(D::String(content_type)),
        Some(D::List(headers)),
        Some(D::Int(kind)),
        Some(D::Int(max_pending_writes)),
        Some(D::String(payload)),
        Some(D::Int(status)),
    ) = (
        fields.next(),
        fields.next(),
        fields.next(),
        fields.next(),
        fields.next(),
        fields.next(),
        fields.next(),
        fields.next(),
    )
    else {
        return Err(error("malformed source Response record"));
    };
    if !(100..=599).contains(&status) {
        return Err(error(format!(
            "HTTP response status `{status}` is outside HTTP range"
        )));
    }
    http::HeaderValue::from_str(&content_type)
        .map_err(|reason| error(format!("invalid response content type: {reason}")))?;
    let headers = headers.into_iter().map(header).collect::<Result<_>>()?;
    let body = match kind {
        0..=2 => SourceResponseBody::Text(payload),
        4 => SourceResponseBody::File(payload),
        5 => {
            let chunks = chunks
                .into_iter()
                .map(|chunk| match chunk.into_descriptor() {
                    D::String(chunk) => Ok(Bytes::from(chunk)),
                    _ => Err(error("Response.stream chunks must be String")),
                })
                .collect::<Result<_>>()?;
            SourceResponseBody::Stream(
                HttpResponseChunks::new(chunks, chunk_size, max_pending_writes)
                    .map_err(|reason| error(format!("invalid Response.stream limits: {reason}")))?,
            )
        }
        _ => return Err(error(format!("unsupported source Response kind `{kind}`"))),
    };
    Ok(SourceResponse {
        status: status as u16,
        content_type,
        headers,
        body,
    })
}

fn header<V: DescriptorValue>(value: V) -> Result<(String, String)> {
    let D::Tuple(pair) = value.into_descriptor() else {
        return Err(error("malformed response header"));
    };
    let mut pair = pair.into_iter().map(DescriptorValue::into_descriptor);
    let (Some(D::String(name)), Some(D::String(value)), None) =
        (pair.next(), pair.next(), pair.next())
    else {
        return Err(error("malformed response header"));
    };
    validate_response_header(&name, &value).map_err(|reason| error(reason.message()))?;
    Ok((name, value))
}

#[cfg(test)]
#[path = "response_test.rs"]
mod tests;
