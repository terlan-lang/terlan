use serde::Serialize;

use crate::commands::artifacts::SyntaxAssetImportInput;

/// JavaScript build manifest.
///
/// Inputs:
/// - Created after successful JS module emission.
///
/// Output:
/// - Serializable manifest stored at `_build/js/manifest.json`.
///
/// Transformation:
/// - Records the selected target profile, module format, extension, and emitted
///   module list for downstream release checks.
#[derive(Debug, Serialize)]
pub(super) struct JsBuildManifest<'a> {
    pub(super) schema: &'static str,
    pub(super) target_profile: &'static str,
    pub(super) module_format: &'static str,
    pub(super) module_extension: &'static str,
    pub(super) modules: &'a [JsModuleArtifact],
}

/// JavaScript module artifact manifest entry.
///
/// Inputs:
/// - Created per emitted Terlan module.
///
/// Output:
/// - Serializable module entry inside `manifest.json`.
///
/// Transformation:
/// - Records source, artifact, CoreIR hash, target profile, and validation
///   status without embedding JavaScript source text.
#[derive(Debug, Serialize)]
pub(super) struct JsModuleArtifact {
    pub(super) module: String,
    pub(super) source_path: String,
    pub(super) artifact_path: String,
    pub(super) relative_path: String,
    pub(super) core_ir_hash: u64,
    pub(super) target_profile: String,
    pub(super) validation_status: String,
    pub(super) runtime_smoke_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) declaration_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(super) declaration_relative_path: Option<String>,
    #[serde(skip)]
    pub(super) asset_imports: Vec<SyntaxAssetImportInput>,
}

/// JavaScript declaration artifact manifest metadata.
///
/// Inputs:
/// - Created when `terlc build --target js --declarations` writes a `.d.ts`
///   file for one emitted module.
///
/// Output:
/// - Paths copied into the corresponding JS module manifest entry.
///
/// Transformation:
/// - Separates declaration path construction from module metadata assembly so
///   the manifest can omit declaration fields when the user did not request
///   them.
#[derive(Debug)]
pub(super) struct JsDeclarationArtifact {
    pub(super) artifact_path: String,
    pub(super) relative_path: String,
}

/// JavaScript target-profile metadata file.
///
/// Inputs:
/// - Created from the selected `JsTargetContract`.
///
/// Output:
/// - Serializable metadata stored at `_build/js/metadata/target-profile.json`.
///
/// Transformation:
/// - Captures profile and module-format facts separately from the module
///   manifest so target validators can read them directly later.
#[derive(Debug, Serialize)]
pub(super) struct JsTargetProfileMetadata {
    pub(super) target_profile: &'static str,
    pub(super) module_format: &'static str,
    pub(super) module_extension: &'static str,
    pub(super) unsupported_feature_code: &'static str,
}

/// JavaScript diagnostics metadata file.
///
/// Inputs:
/// - Created after successful JS emission.
///
/// Output:
/// - Serializable empty diagnostics payload stored under JS metadata.
///
/// Transformation:
/// - Pins the diagnostics family and unsupported-feature code even before J0.6
///   starts recording rejected feature metadata.
#[derive(Debug, Serialize)]
pub(super) struct JsDiagnosticsMetadata {
    pub(super) diagnostic_family: &'static str,
    pub(super) unsupported_feature_code: &'static str,
    pub(super) diagnostics: Vec<String>,
}
