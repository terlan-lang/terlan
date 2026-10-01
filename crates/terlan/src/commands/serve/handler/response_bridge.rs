use std::borrow::Cow;
use std::fs;
use std::path::{Path, PathBuf};

use crate::runtime::vm::ReplValue;
use terlan_http_native::HttpResponseChunks;

use super::super::package_relative_path;
use super::types::{WebPackageResponseHeader, WebPackageStaticResponse};

/// An admitted handler response, separate from socket writing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HandlerResponse {
    pub(crate) status: u16,
    pub(crate) content_type: Cow<'static, str>,
    pub(crate) headers: Vec<(String, String)>,
    pub(crate) body: HandlerBody,
}

/// Preserves text ownership while retaining exact bytes for file responses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum HandlerBody {
    Text(String),
    Bytes(Vec<u8>),
    Stream(HttpResponseChunks),
}

impl HandlerBody {
    #[cfg(test)]
    pub(crate) fn as_bytes(&self) -> &[u8] {
        match self {
            Self::Text(body) => body.as_bytes(),
            Self::Bytes(body) => body,
            Self::Stream(_) => panic!("stream bodies must be polled through the transport"),
        }
    }
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.as_bytes().is_empty()
    }
}

impl HandlerResponse {
    /// Admits source values through the owning package without copying the body.
    pub(crate) fn from_owned_vm_response_with_package_root(
        response: ReplValue,
        package_root: &Path,
    ) -> Result<Self, String> {
        source_response::decode(response, Some(package_root))
    }

    /// Preserves the borrowed caller's value before package-owned admission.
    pub(crate) fn from_vm_response_with_package_root(
        response: &ReplValue,
        package_root: &Path,
    ) -> Result<Self, String> {
        source_response::decode(response.clone(), Some(package_root))
    }
}

/// Authorizes and opens a package-admitted file intent using host capabilities.
fn read_response_file(
    relative_path: &str,
    content_type: String,
    package_root: Option<&Path>,
) -> Result<(String, Vec<u8>), String> {
    let package_root = package_root.ok_or_else(|| {
        "error[serve_handler]: Response.file requires package file-serving context".to_string()
    })?;
    let response_path = package_relative_path(package_root, relative_path)
        .or_else(|| trusted_response_file_path(relative_path))
        .ok_or_else(|| {
            format!(
                "error[serve_handler]: Response.file path `{relative_path}` is not package-relative or admitted by a trusted file root"
            )
        })?;
    if !response_path.is_file() {
        return Err(format!(
            "error[serve_handler]: Response.file path `{relative_path}` does not name a file"
        ));
    }
    let body = fs::read(&response_path).map_err(|err| {
        format!("error[serve_handler]: Response.file path `{relative_path}` cannot be read: {err}")
    })?;
    let content_type = if content_type.is_empty() {
        terlan_http_native::content_type_for_path(&response_path)
    } else {
        content_type
    };
    Ok((content_type, body))
}

/// Resolves an absolute service-owned file only under explicitly admitted roots.
fn trusted_response_file_path(path: &str) -> Option<PathBuf> {
    if std::env::var("TERLAN_SERVE_TRUSTED_HOST_CAPABILITIES").as_deref() != Ok("1") {
        return None;
    }
    let candidate = Path::new(path);
    if !candidate.is_absolute() {
        return None;
    }
    let candidate = candidate.canonicalize().ok()?;
    std::env::var_os("TERLAN_SERVE_TRUSTED_FILE_ROOTS")?
        .to_string_lossy()
        .split(':')
        .filter(|root| !root.is_empty())
        .filter_map(|root| Path::new(root).canonicalize().ok())
        .any(|root| candidate.starts_with(root))
        .then_some(candidate)
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
    crate::runtime::vm::native_value::from_native(
        terlan_http_native::source_descriptor::cached_response(
            response.status,
            response.content_type.clone(),
            response.body.clone(),
            response
                .headers
                .iter()
                .map(|header| (header.name.clone(), header.value.clone()))
                .collect(),
        ),
    )
}

#[cfg(test)]
#[path = "response_bridge_test.rs"]
mod response_bridge_test;

mod source_response;
