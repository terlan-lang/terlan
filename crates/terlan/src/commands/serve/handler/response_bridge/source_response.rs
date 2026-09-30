//! HTTP transport admission for the source-owned Response record.

use super::*;

#[cfg(test)]
#[path = "source_response/source_response_test.rs"]
mod tests;

pub(super) fn decode(
    name: &str,
    mut fields: Vec<(String, ReplValue)>,
    package_root: Option<&Path>,
) -> Result<HandlerResponse, String> {
    let malformed = || "error[serve_handler]: malformed source Response record".to_string();
    if name != "Response" {
        return Err(malformed());
    }
    fields.sort_by(|left, right| left.0.cmp(&right.0));
    if !fields.iter().map(|(name, _)| name.as_str()).eq([
        "chunk_size",
        "chunks",
        "content_type",
        "headers",
        "kind",
        "max_pending_writes",
        "payload",
        "status",
    ]) {
        return Err(malformed());
    }
    let fields: [(String, ReplValue); 8] = fields.try_into().map_err(|_| malformed())?;
    let [ReplValue::Int(chunk_size), ReplValue::List(chunks), ReplValue::String(content_type), headers, ReplValue::Int(kind), ReplValue::Int(max_pending_writes), ReplValue::String(payload), ReplValue::Int(status)] =
        fields.map(|(_, value)| value)
    else {
        return Err(malformed());
    };
    let status = vm_status_to_u16(status)?;
    validate_handler_status(status)?;
    ::http::HeaderValue::from_str(&content_type)
        .map_err(|error| format!("error[serve_handler]: invalid response content type: {error}"))?;
    let headers = owned_native_response_headers(headers)?;
    let (content_type, body) = match kind {
        0..=2 => (content_type, HandlerBody::Text(payload)),
        4 => {
            let (_, content_type, bytes, _) = vm_file_response(
                &[
                    ReplValue::String(payload),
                    ReplValue::Int(i64::from(status)),
                    ReplValue::String(content_type),
                ],
                package_root,
            )?;
            (content_type, HandlerBody::Bytes(bytes))
        }
        5 => {
            let chunks = chunks
                .into_iter()
                .map(|chunk| match chunk {
                    ReplValue::String(chunk) => Ok(Bytes::from(chunk)),
                    _ => Err(
                        "error[serve_handler]: Response.stream chunks must be String".to_string(),
                    ),
                })
                .collect::<Result<Vec<_>, _>>()?;
            let stream = HttpResponseChunks::new(chunks, chunk_size, max_pending_writes).map_err(
                |error| format!("error[serve_handler]: invalid Response.stream limits: {error}"),
            )?;
            (content_type, HandlerBody::Stream(stream))
        }
        _ => {
            return Err(format!(
                "error[serve_handler]: unsupported source Response kind `{kind}`"
            ))
        }
    };
    Ok(HandlerResponse {
        status,
        content_type: Cow::Owned(content_type),
        headers,
        body,
    })
}
