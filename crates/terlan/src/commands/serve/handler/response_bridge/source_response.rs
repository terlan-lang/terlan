//! Host file access and transport adaptation after package-owned admission.

use super::*;

#[cfg(test)]
#[path = "source_response/source_response_test.rs"]
mod tests;

pub(super) fn decode(
    value: ReplValue,
    package_root: Option<&Path>,
) -> Result<HandlerResponse, String> {
    use terlan_http_native::source_descriptor::{response, SourceResponse, SourceResponseBody};
    let SourceResponse {
        status,
        content_type,
        headers,
        body,
    } = response(value).map_err(|error| format!("error[serve_handler]: {}", error.message()))?;
    let (content_type, body) = match body {
        SourceResponseBody::Text(payload) => (content_type, HandlerBody::Text(payload)),
        SourceResponseBody::File(path) => {
            let (content_type, bytes) = read_response_file(&path, content_type, package_root)?;
            (content_type, HandlerBody::Bytes(bytes))
        }
        SourceResponseBody::Stream(stream) => (content_type, HandlerBody::Stream(stream)),
    };
    Ok(HandlerResponse {
        status,
        content_type: Cow::Owned(content_type),
        headers,
        body,
    })
}
