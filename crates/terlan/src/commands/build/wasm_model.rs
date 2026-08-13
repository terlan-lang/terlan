use serde::Serialize;

/// Serializable package metadata for a reserved Wasm target.
///
/// Inputs:
/// - Parsed `[target.wasm]` manifest metadata.
///
/// Output:
/// - JSON-ready Wasm target block for build package metadata.
///
/// Transformation:
/// - Records the reserved target contract without selecting a compiler backend
///   or generating a Wasm artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct BuildWasmTargetMetadata {
    pub(super) profile: String,
    pub(super) exports: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) bridge: Option<String>,
    pub(super) capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) world: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) validation_engine: Option<String>,
}

/// Serializable package metadata for a reserved WASI target.
///
/// Inputs:
/// - Parsed `[target.wasi]` manifest metadata.
///
/// Output:
/// - JSON-ready WASI target block for build package metadata.
///
/// Transformation:
/// - Records the reserved target contract without selecting a compiler backend
///   or generating a WASI component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct BuildWasiTargetMetadata {
    pub(super) profile: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) world: Option<String>,
    pub(super) capabilities: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) validation_engine: Option<String>,
}
