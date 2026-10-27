//! Host manifest I/O; TLS settings and admission belong to std.http.

#[cfg(any(
    test,
    all(feature = "serve-runtime-bin", not(feature = "native-codegen"))
))]
use std::path::Path;
#[cfg(any(
    test,
    all(feature = "serve-runtime-bin", not(feature = "native-codegen"))
))]
use terlan_runtime_abi::{BoundaryError, ErrorDomain};

pub(crate) use terlan_http_native::tls_config::Config as ProjectServerTls;

#[cfg(any(
    test,
    all(feature = "serve-runtime-bin", not(feature = "native-codegen"))
))]
#[derive(serde::Deserialize, Default)]
struct RuntimeProjectManifest {
    #[serde(default)]
    server: RuntimeServerManifest,
}

#[cfg(any(
    test,
    all(feature = "serve-runtime-bin", not(feature = "native-codegen"))
))]
#[derive(serde::Deserialize, Default)]
struct RuntimeServerManifest {
    tls: Option<terlan_http_native::tls_config::Settings>,
}

#[cfg(any(
    test,
    all(feature = "serve-runtime-bin", not(feature = "native-codegen"))
))]
pub(crate) fn read_runtime_server_tls(
    path: &Path,
) -> Result<Option<ProjectServerTls>, BoundaryError> {
    let source = std::fs::read_to_string(path).map_err(|error| {
        tls_contract_error(path, format!("cannot read project manifest: {error}"))
    })?;
    let manifest: RuntimeProjectManifest = toml::from_str(&source).map_err(|error| {
        tls_contract_error(path, format!("cannot parse runtime TLS metadata: {error}"))
    })?;
    manifest
        .server
        .tls
        .map(|tls| {
            tls.validate()
                .map_err(|error| tls_contract_error(path, error))
        })
        .transpose()
}

#[cfg(any(
    test,
    all(feature = "serve-runtime-bin", not(feature = "native-codegen"))
))]
fn tls_contract_error(path: &Path, message: impl Into<String>) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::CommandExecution,
        "read runtime TLS contract",
        format!("{}: {}", path.display(), message.into()),
    )
}

#[cfg(test)]
#[path = "tls_contract_test.rs"]
mod tests;
