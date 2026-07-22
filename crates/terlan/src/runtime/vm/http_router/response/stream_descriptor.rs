use super::DecodedStdHttpResponse;
use crate::runtime::vm::http_static::{VmHttpStaticError, VmHttpStreamPlan};
use crate::runtime::vm::ReplValue;

/// Validated stream metadata decoded from a standard HTTP response value.
pub(super) struct DecodedStdHttpStream {
    pub(super) plan: VmHttpStreamPlan,
    pub(super) chunks: Vec<Vec<u8>>,
}

/// Decodes stream body, content type, chunk size, and queue capacity.
pub(super) fn decode(
    payload: &ReplValue,
    remaining: &[ReplValue],
    status: i64,
) -> Result<DecodedStdHttpResponse, VmHttpStaticError> {
    let ReplValue::List(values) = payload else {
        return Err(VmHttpStaticError::InvalidResponse);
    };
    let chunks = values
        .iter()
        .map(|value| match value {
            ReplValue::String(chunk) => Ok(chunk.as_bytes().to_vec()),
            _ => Err(VmHttpStaticError::InvalidResponse),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let positional_len = remaining
        .iter()
        .position(|value| matches!(value, ReplValue::Tuple(_)))
        .unwrap_or(remaining.len());
    let [ReplValue::Int(_), ReplValue::String(content_type), ReplValue::Int(chunk_size), ReplValue::Int(max_pending_writes)] =
        &remaining[..positional_len]
    else {
        return Err(VmHttpStaticError::InvalidResponse);
    };
    if content_type.trim().is_empty() {
        return Err(VmHttpStaticError::InvalidResponse);
    }
    let chunk_size = positive_limit(*chunk_size)?;
    let max_pending_writes = positive_limit(*max_pending_writes)?;

    Ok(DecodedStdHttpResponse {
        status,
        body: Vec::new(),
        stream: Some(DecodedStdHttpStream {
            plan: VmHttpStreamPlan::new(chunk_size, max_pending_writes)?,
            chunks,
        }),
        content_type: Some(content_type.clone()),
        cache_control: None,
        location: None,
        headers: Vec::new(),
    })
}

/// Converts a positive descriptor integer into a platform stream limit.
fn positive_limit(value: i64) -> Result<usize, VmHttpStaticError> {
    usize::try_from(value)
        .ok()
        .filter(|value| *value > 0)
        .ok_or(VmHttpStaticError::InvalidStreamLimit)
}
