//! Validation of the compiler-owned bounded streaming response layout.

use super::*;

#[derive(Debug)]
pub(super) enum StreamResponseError {
    Shape,
    Chunk,
    Metadata(String),
    Limits(crate::runtime::vm::http_response_chunks::InvalidHttpStreamLimits),
}

impl std::fmt::Display for StreamResponseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Shape => formatter.write_str("malformed Response.stream descriptor"),
            Self::Chunk => formatter.write_str("Response.stream chunks must be String"),
            Self::Metadata(message) => formatter.write_str(message),
            Self::Limits(error) => write!(formatter, "invalid Response.stream limits: {error}"),
        }
    }
}

impl std::error::Error for StreamResponseError {}

/// Decodes the exact stream shape before the HTTP transport accepts it.
pub(super) fn decode(rest: &[ReplValue]) -> Result<HandlerResponse, StreamResponseError> {
    let [ReplValue::String(_), ReplValue::Int(status), ReplValue::String(content_type), headers, ReplValue::List(chunks), ReplValue::Int(chunk_size), ReplValue::Int(max_pending_writes)] =
        rest
    else {
        return Err(StreamResponseError::Shape);
    };
    let status = vm_status_to_u16(*status).map_err(StreamResponseError::Metadata)?;
    validate_handler_status(status).map_err(StreamResponseError::Metadata)?;
    let mut validated_headers = Vec::new();
    apply_native_response_headers(Some(headers), &mut validated_headers)
        .map_err(StreamResponseError::Metadata)?;
    let chunks = chunks
        .iter()
        .map(|chunk| match chunk {
            ReplValue::String(chunk) => Ok(Bytes::copy_from_slice(chunk.as_bytes())),
            _ => Err(StreamResponseError::Chunk),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let stream = VmHttpResponseChunks::new(chunks, *chunk_size, *max_pending_writes)
        .map_err(StreamResponseError::Limits)?;
    Ok(HandlerResponse {
        status,
        content_type: Cow::Owned(content_type.clone()),
        headers: validated_headers,
        body: HandlerBody::Stream(stream),
    })
}
