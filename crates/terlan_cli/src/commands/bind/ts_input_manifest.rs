use std::fs;
use std::path::{Path, PathBuf};

use crate::support::{is_valid_sha256_hex, sha256sum_file};
use serde::Deserialize;

/// Expected schema for committed TypeScript input manifests.
const TS_INPUT_MANIFEST_SCHEMA: &str = "terlan.std.js.input-manifest.v1";

/// Parsed TypeScript input manifest.
///
/// Inputs:
/// - JSON document from `STD_JS_DOM_INPUT_MANIFEST`.
///
/// Output:
/// - Deserialized manifest carrying generator, package, target, and input
///   pinning metadata.
///
/// Transformation:
/// - Serde maps the committed JSON contract into typed fields so tests and
///   later generator commands do not depend on ad hoc JSON lookups.
#[derive(Debug, Deserialize)]
pub(super) struct TsInputManifest {
    pub(super) schema: String,
    pub(super) generator: TsInputGenerator,
    pub(super) target_profile: String,
    pub(super) source_package: TsSourcePackage,
    pub(super) inputs: Vec<TsInputFile>,
}

/// Generator metadata recorded in a TypeScript input manifest.
#[derive(Debug, Deserialize)]
pub(super) struct TsInputGenerator {
    pub(super) name: String,
    pub(super) version: String,
    pub(super) profile: String,
    pub(super) oxc_parser: bool,
}

/// Source package metadata recorded in a TypeScript input manifest.
#[derive(Debug, Deserialize)]
pub(super) struct TsSourcePackage {
    pub(super) name: String,
    pub(super) version: String,
    pub(super) resolution: String,
}

/// One pinned TypeScript declaration input.
#[derive(Debug, Deserialize)]
pub(super) struct TsInputFile {
    pub(super) path: String,
    pub(super) sha256: String,
    pub(super) kind: String,
    pub(super) namespace: String,
}

/// Loads and validates a TypeScript input manifest.
///
/// Inputs:
/// - `repo_root`: repository root used to resolve manifest input paths.
/// - `manifest_path`: path to the manifest, absolute or relative to
///   `repo_root`.
///
/// Output:
/// - `Ok(TsInputManifest)` when the manifest is valid and all pinned inputs
///   match their recorded hashes.
/// - `Err(String)` when loading, parsing, validation, or hashing fails.
///
/// Transformation:
/// - Reads the manifest from disk, parses it into typed metadata, and applies
///   the same validation contract used by release checks before generation.
pub(super) fn load_ts_input_manifest(
    repo_root: &Path,
    manifest_path: &Path,
) -> Result<TsInputManifest, String> {
    let manifest_path = if manifest_path.is_absolute() {
        manifest_path.to_path_buf()
    } else {
        repo_root.join(manifest_path)
    };
    let manifest_text = fs::read_to_string(&manifest_path).map_err(|err| {
        format!(
            "failed to read TypeScript input manifest `{}`: {err}",
            manifest_path.display()
        )
    })?;
    let manifest = parse_ts_input_manifest(&manifest_text)?;
    validate_ts_input_manifest(repo_root, &manifest)?;
    Ok(manifest)
}

/// Parses TypeScript input manifest JSON.
///
/// Inputs:
/// - `manifest_text`: JSON document to parse.
///
/// Output:
/// - `Ok(TsInputManifest)` when JSON matches the manifest shape.
/// - `Err(String)` when JSON parsing fails.
///
/// Transformation:
/// - Delegates to `serde_json` and wraps errors in the `ts_bindgen` diagnostic
///   family vocabulary used by the 0.0.4 roadmap.
fn parse_ts_input_manifest(manifest_text: &str) -> Result<TsInputManifest, String> {
    serde_json::from_str(manifest_text)
        .map_err(|err| format!("ts_bindgen.input_manifest_parse_failed: {err}"))
}

/// Validates a parsed TypeScript input manifest.
///
/// Inputs:
/// - `repo_root`: repository root used to resolve input paths.
/// - `manifest`: parsed manifest metadata.
///
/// Output:
/// - `Ok(())` when the manifest satisfies the T0.1 contract.
/// - `Err(String)` when schema, generator, package, path, or hash metadata is
///   invalid.
///
/// Transformation:
/// - Performs deterministic structural checks and verifies that each declared
///   input file exists with the recorded SHA-256 hash.
fn validate_ts_input_manifest(repo_root: &Path, manifest: &TsInputManifest) -> Result<(), String> {
    if manifest.schema != TS_INPUT_MANIFEST_SCHEMA {
        return Err(format!(
            "ts_bindgen.input_manifest_schema_mismatch: expected `{TS_INPUT_MANIFEST_SCHEMA}`, found `{}`",
            manifest.schema
        ));
    }
    validate_generator_metadata(&manifest.generator)?;
    validate_source_package_metadata(&manifest.source_package)?;
    if manifest.target_profile != "js.browser" {
        return Err(format!(
            "ts_bindgen.input_manifest_target_profile: expected `js.browser`, found `{}`",
            manifest.target_profile
        ));
    }
    if manifest.inputs.is_empty() {
        return Err("ts_bindgen.input_manifest_empty: expected at least one input".to_string());
    }
    for input in &manifest.inputs {
        validate_input_file(repo_root, input)?;
    }
    Ok(())
}

/// Validates generator metadata for a TypeScript input manifest.
///
/// Inputs:
/// - `generator`: parsed generator metadata.
///
/// Output:
/// - `Ok(())` when the current T0.1 generator metadata is valid.
/// - `Err(String)` when a required value is missing or not owned by `terlc`.
///
/// Transformation:
/// - Enforces the committed generator identity and records that Oxc parser
///   ownership is part of the input contract before parser ingestion begins.
fn validate_generator_metadata(generator: &TsInputGenerator) -> Result<(), String> {
    if generator.name != "terlc" {
        return Err(format!(
            "ts_bindgen.input_manifest_generator: expected `terlc`, found `{}`",
            generator.name
        ));
    }
    if generator.version.trim().is_empty() {
        return Err("ts_bindgen.input_manifest_generator_version_empty".to_string());
    }
    if generator.profile.trim().is_empty() {
        return Err("ts_bindgen.input_manifest_generator_profile_empty".to_string());
    }
    if !generator.oxc_parser {
        return Err("ts_bindgen.input_manifest_oxc_parser_required".to_string());
    }
    Ok(())
}

/// Validates source package metadata for a TypeScript input manifest.
///
/// Inputs:
/// - `source_package`: parsed package metadata.
///
/// Output:
/// - `Ok(())` when the manifest pins the TypeScript source package.
/// - `Err(String)` when package identity, version, or resolution is invalid.
///
/// Transformation:
/// - Keeps the initial tiny fixture explicit as a committed TypeScript fixture
///   while reserving the same package fields for later installed-package
///   manifests.
fn validate_source_package_metadata(source_package: &TsSourcePackage) -> Result<(), String> {
    if source_package.name != "typescript" {
        return Err(format!(
            "ts_bindgen.input_manifest_package: expected `typescript`, found `{}`",
            source_package.name
        ));
    }
    if source_package.version.trim().is_empty() {
        return Err("ts_bindgen.input_manifest_package_version_empty".to_string());
    }
    if source_package.resolution.trim().is_empty() {
        return Err("ts_bindgen.input_manifest_package_resolution_empty".to_string());
    }
    Ok(())
}

/// Validates one pinned TypeScript declaration input.
///
/// Inputs:
/// - `repo_root`: repository root used to resolve the input path.
/// - `input`: parsed input metadata.
///
/// Output:
/// - `Ok(())` when the input path, kind, namespace, and SHA-256 are valid.
/// - `Err(String)` when the file is missing, unsafe, or hash-mismatched.
///
/// Transformation:
/// - Resolves only safe repository-relative paths and compares the manifest
///   SHA-256 with the local fixture hash.
fn validate_input_file(repo_root: &Path, input: &TsInputFile) -> Result<(), String> {
    if input.kind != "typescript-declaration" {
        return Err(format!(
            "ts_bindgen.input_manifest_kind: expected `typescript-declaration`, found `{}`",
            input.kind
        ));
    }
    if !matches!(input.namespace.as_str(), "std.js" | "std.js.Dom") {
        return Err(format!(
            "ts_bindgen.input_manifest_namespace: expected `std.js` or `std.js.Dom`, found `{}`",
            input.namespace
        ));
    }
    if !is_valid_sha256_hex(&input.sha256) {
        return Err(format!(
            "ts_bindgen.input_manifest_sha256_invalid: `{}`",
            input.sha256
        ));
    }
    let relative_path = safe_repo_relative_path(&input.path)?;
    let input_path = repo_root.join(relative_path);
    if !input_path.exists() {
        return Err(format!(
            "ts_bindgen.input_manifest_missing_input: `{}`",
            input.path
        ));
    }
    let actual = sha256sum_file(&input_path)
        .map_err(|error| format!("ts_bindgen.input_manifest_sha256_tool_failed: {error}"))?;
    if actual != input.sha256 {
        return Err(format!(
            "ts_bindgen.input_manifest_sha256_mismatch: `{}` expected `{}`, found `{actual}`",
            input.path, input.sha256
        ));
    }
    Ok(())
}

/// Converts a manifest path to a safe repository-relative path.
///
/// Inputs:
/// - `path`: path string from the committed manifest.
///
/// Output:
/// - `Ok(PathBuf)` for relative paths that stay inside the repository.
/// - `Err(String)` for absolute paths, parent traversals, or empty paths.
///
/// Transformation:
/// - Splits path components and rejects filesystem forms that would let a
///   manifest read outside the published repository.
pub(super) fn safe_repo_relative_path(path: &str) -> Result<PathBuf, String> {
    let path = Path::new(path);
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err("ts_bindgen.input_manifest_path_not_relative".to_string());
    }
    if path
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err("ts_bindgen.input_manifest_path_parent_dir".to_string());
    }
    Ok(path.to_path_buf())
}
