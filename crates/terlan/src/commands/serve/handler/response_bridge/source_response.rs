//! Host file access and transport adaptation after package-owned admission.

use super::*;

#[cfg(test)]
#[path = "source_response/source_response_test.rs"]
mod tests;

pub(super) fn decode(
    value: ReplValue,
    package_root: Option<&Path>,
) -> Result<HandlerResponse, String> {
    HandlerResponse::from_source(value, |path, content_type| {
        terlan_http_native::file_response::read_response_file(
            package_root,
            &trusted_file_roots(),
            path,
            content_type,
        )
    })
    .map_err(|error| format!("error[serve_handler]: {}", error.message()))
}
