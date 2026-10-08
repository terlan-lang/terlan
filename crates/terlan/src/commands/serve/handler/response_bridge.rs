use std::path::Path;

use crate::runtime::vm::ReplValue;

use super::types::{WebPackageResponseHeader, WebPackageStaticResponse};

pub(crate) use terlan_http_native::admitted_response::{
    AdmittedResponse as HandlerResponse, ResponseBody as HandlerBody,
};

/// Supplies host capabilities while source admission remains package-owned.
pub(crate) fn decode_owned_response(
    response: ReplValue,
    package_root: &Path,
) -> Result<HandlerResponse, String> {
    source_response::decode(response, Some(package_root))
}

pub(crate) fn decode_response(
    response: &ReplValue,
    package_root: &Path,
) -> Result<HandlerResponse, String> {
    decode_owned_response(response.clone(), package_root)
}

/// Supplies explicitly enabled host capabilities to the owning package.
fn trusted_file_roots() -> Vec<std::path::PathBuf> {
    if std::env::var("TERLAN_SERVE_TRUSTED_HOST_CAPABILITIES").as_deref() == Ok("1") {
        std::env::var_os("TERLAN_SERVE_TRUSTED_FILE_ROOTS")
            .map(|roots| {
                std::env::split_paths(&roots)
                    .filter(|root| !root.as_os_str().is_empty())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    }
}

/// Revalidates manifest headers through package policy before socket emission.
pub(crate) fn static_response_header_tuples(
    headers: &[WebPackageResponseHeader],
) -> Result<Vec<(String, String)>, String> {
    headers
        .iter()
        .map(|header| validate_response_header(&header.name, &header.value))
        .collect()
}

/// Uses maintained package parsing and framing protection for handler headers.
pub(super) fn validate_response_header(
    name: &str,
    value: &str,
) -> Result<(String, String), String> {
    terlan_http_native::validate_response_header(name, value)
        .map_err(|error| format!("error[serve_handler]: {}", error.message()))?;
    Ok((name.to_string(), value.to_string()))
}

/// Projects cached metadata through the package, not a compiler-owned layout.
pub(super) fn static_response_vm_value(response: &WebPackageStaticResponse) -> ReplValue {
    let headers: Vec<_> = response
        .headers
        .iter()
        .map(|header| (header.name.clone(), header.value.clone()))
        .collect();
    let defaults = terlan_http_native::server_default_headers(&headers);
    crate::runtime::vm::native_value::from_native(
        terlan_http_native::source_descriptor::cached_response(
            response.status,
            response.content_type.clone(),
            response.body.clone(),
            headers,
            defaults,
        ),
    )
}

#[cfg(test)]
#[path = "response_bridge_test.rs"]
mod response_bridge_test;

mod source_response;
