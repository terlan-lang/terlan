use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;

use crate::terlan_quality::{render_failure, QualityResult};

const RUST_BUILD_FEATURE_MANIFEST: &str = "docs/package/RUST_BUILD_FEATURES.json";
const PACKAGE_HELPER: &str = "tools/package_release_artifact.py";
const CARGO_MANIFESTS: &[&str] = &["Cargo.toml", "crates/terlan/Cargo.toml"];
const FEATURE_CLASSIFICATIONS: &[&str] = &[
    "default",
    "optional",
    "release-only",
    "test-only",
    "tooling-only",
    "unsupported",
    "excluded",
];
const RELEASE_METADATA_FIELDS: &[&str] = &[
    "cargo_features",
    "target_triple",
    "profile",
    "source_revision",
    "crate_versions",
    "binaries",
];

/// Summary produced by the Rust build feature shipping gate.
///
/// Inputs:
/// - Classified Cargo feature count.
/// - Release profile count.
/// - Release metadata field count.
///
/// Output:
/// - Stable success metrics for the quality CLI.
///
/// Transformation:
/// - Keeps Cargo feature declarations, release artifact metadata, and shipped
///   binary names tied to one manifest-backed contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RustBuildFeatureShippingSummary {
    pub classified_feature_count: usize,
    pub release_profile_count: usize,
    pub release_metadata_field_count: usize,
}

/// Runs the Rust build feature shipping contract gate.
///
/// Inputs:
/// - `root`: repository root containing Cargo manifests, release helper, and
///   `docs/package/RUST_BUILD_FEATURES.json`.
///
/// Output:
/// - Success when every Cargo feature is classified, the default release
///   profile records exact artifact metadata, and package metadata uses the
///   same feature set the manifest promises.
/// - Stable diagnostics when Cargo exposes unclassified features, release
///   metadata omits exact build fields, or packaging is only workspace-local.
///
/// Transformation:
/// - Validates feature shipping without requiring a compiled release artifact
///   by asking the packaging helper to emit its metadata contract directly.
pub fn run_rust_build_feature_shipping(
    root: &Path,
) -> QualityResult<RustBuildFeatureShippingSummary> {
    let manifest = read_json(root, RUST_BUILD_FEATURE_MANIFEST)?;
    let metadata = release_metadata(root)?;
    let cargo_features = collect_cargo_features(root)?;

    let mut diagnostics = Vec::new();
    let classified_feature_count =
        validate_feature_manifest(&manifest, &cargo_features, &mut diagnostics);
    let release_profile_count = validate_release_profiles(&manifest, &metadata, &mut diagnostics);
    let release_metadata_field_count = validate_release_metadata(&metadata, &mut diagnostics);
    diagnostics.extend(validate_package_helper_text(root)?);

    if diagnostics.is_empty() {
        Ok(RustBuildFeatureShippingSummary {
            classified_feature_count,
            release_profile_count,
            release_metadata_field_count,
        })
    } else {
        Err(render_failure("rust-build-feature-shipping", &diagnostics))
    }
}

/// Reads one JSON file from the repository.
fn read_json(root: &Path, relative: &str) -> QualityResult<Value> {
    let path = root.join(relative);
    let text = fs::read_to_string(&path)
        .map_err(|err| format!("{}: failed to read JSON: {err}", path.display()))?;
    serde_json::from_str(&text)
        .map_err(|err| format!("{}: failed to parse JSON: {err}", path.display()))
}

/// Runs the release packaging helper's metadata mode.
fn release_metadata(root: &Path) -> QualityResult<Value> {
    let output = Command::new("python3")
        .arg("-B")
        .arg(PACKAGE_HELPER)
        .arg("metadata")
        .current_dir(root)
        .output()
        .map_err(|err| format!("{PACKAGE_HELPER}: failed to run metadata mode: {err}"))?;
    if !output.status.success() {
        return Err(format!(
            "{PACKAGE_HELPER}: metadata mode failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    serde_json::from_slice(&output.stdout)
        .map_err(|err| format!("{PACKAGE_HELPER}: metadata mode emitted invalid JSON: {err}"))
}

/// Collects Cargo features from repository manifests.
fn collect_cargo_features(root: &Path) -> QualityResult<BTreeSet<String>> {
    let mut features = BTreeSet::new();
    for relative in CARGO_MANIFESTS {
        let text = fs::read_to_string(root.join(relative))
            .map_err(|err| format!("{relative}: failed to read Cargo manifest: {err}"))?;
        features.extend(parse_cargo_features(&text).into_keys());
    }
    Ok(features)
}

/// Extracts features declared under a Cargo `[features]` section.
fn parse_cargo_features(text: &str) -> BTreeMap<String, String> {
    let mut in_features = false;
    let mut features = BTreeMap::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_features = trimmed == "[features]";
            continue;
        }
        if !in_features || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((name, value)) = trimmed.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        features.insert(name.to_string(), value.trim().to_string());
    }
    features
}

/// Validates the checked-in Rust feature manifest.
fn validate_feature_manifest(
    manifest: &Value,
    cargo_features: &BTreeSet<String>,
    diagnostics: &mut Vec<String>,
) -> usize {
    if manifest.get("schema").and_then(Value::as_str) != Some("terlan.rust-build-features.v1") {
        diagnostics.push(format!(
            "{RUST_BUILD_FEATURE_MANIFEST}: schema must be `terlan.rust-build-features.v1`"
        ));
    }
    if manifest.get("package").and_then(Value::as_str) != Some("terlan") {
        diagnostics.push(format!(
            "{RUST_BUILD_FEATURE_MANIFEST}: package must be `terlan`"
        ));
    }

    let manifest_features = manifest_features(manifest, diagnostics);
    for feature in cargo_features {
        if !manifest_features.contains_key(feature) {
            diagnostics.push(format!(
                "{RUST_BUILD_FEATURE_MANIFEST}: Cargo feature `{feature}` is missing a manifest classification"
            ));
        }
    }
    for feature in manifest_features.keys() {
        if !cargo_features.contains(feature) {
            diagnostics.push(format!(
                "{RUST_BUILD_FEATURE_MANIFEST}: manifest feature `{feature}` is not declared in Cargo.toml"
            ));
        }
    }
    validate_default_features(manifest, &manifest_features, cargo_features, diagnostics);
    manifest_features.len()
}

/// Returns feature classifications declared by the feature manifest.
fn manifest_features(manifest: &Value, diagnostics: &mut Vec<String>) -> BTreeMap<String, String> {
    let Some(features) = manifest.get("features").and_then(Value::as_array) else {
        diagnostics.push(format!(
            "{RUST_BUILD_FEATURE_MANIFEST}: missing `features` array"
        ));
        return BTreeMap::new();
    };

    let mut classified = BTreeMap::new();
    for (index, feature) in features.iter().enumerate() {
        let row = index + 1;
        let Some(name) = nonempty_string(feature, "name") else {
            diagnostics.push(format!(
                "{RUST_BUILD_FEATURE_MANIFEST}: features[{row}] missing non-empty `name`"
            ));
            continue;
        };
        let Some(classification) = nonempty_string(feature, "classification") else {
            diagnostics.push(format!(
                "{RUST_BUILD_FEATURE_MANIFEST}: feature `{name}` missing `classification`"
            ));
            continue;
        };
        if !FEATURE_CLASSIFICATIONS.contains(&classification) {
            diagnostics.push(format!(
                "{RUST_BUILD_FEATURE_MANIFEST}: feature `{name}` has unsupported classification `{classification}`"
            ));
        }
        if classified
            .insert(name.to_string(), classification.to_string())
            .is_some()
        {
            diagnostics.push(format!(
                "{RUST_BUILD_FEATURE_MANIFEST}: duplicate feature `{name}`"
            ));
        }
    }
    classified
}

/// Validates default feature rows against Cargo and classifications.
fn validate_default_features(
    manifest: &Value,
    manifest_features: &BTreeMap<String, String>,
    cargo_features: &BTreeSet<String>,
    diagnostics: &mut Vec<String>,
) {
    let Some(default_features) = manifest.get("default_features").and_then(Value::as_array) else {
        diagnostics.push(format!(
            "{RUST_BUILD_FEATURE_MANIFEST}: missing `default_features` array"
        ));
        return;
    };
    for feature in default_features {
        let Some(feature) = feature.as_str() else {
            diagnostics.push(format!(
                "{RUST_BUILD_FEATURE_MANIFEST}: default feature entries must be strings"
            ));
            continue;
        };
        if !cargo_features.contains(feature) {
            diagnostics.push(format!(
                "{RUST_BUILD_FEATURE_MANIFEST}: default feature `{feature}` is not declared in Cargo.toml"
            ));
        }
        if manifest_features.get(feature).map(String::as_str) != Some("default") {
            diagnostics.push(format!(
                "{RUST_BUILD_FEATURE_MANIFEST}: default feature `{feature}` must have classification `default`"
            ));
        }
    }
}

/// Validates release profiles against metadata emitted by the package helper.
fn validate_release_profiles(
    manifest: &Value,
    metadata: &Value,
    diagnostics: &mut Vec<String>,
) -> usize {
    let Some(profiles) = manifest.get("release_profiles").and_then(Value::as_array) else {
        diagnostics.push(format!(
            "{RUST_BUILD_FEATURE_MANIFEST}: missing `release_profiles` array"
        ));
        return 0;
    };
    let mut has_release = false;
    for (index, profile) in profiles.iter().enumerate() {
        let row = index + 1;
        let name = nonempty_string(profile, "name");
        let cargo_package = nonempty_string(profile, "cargo_package");
        let build_profile = nonempty_string(profile, "profile");
        if name == Some("release") {
            has_release = true;
        }
        if cargo_package != Some("terlan") {
            diagnostics.push(format!(
                "{RUST_BUILD_FEATURE_MANIFEST}: release_profiles[{row}] cargo_package must be `terlan`"
            ));
        }
        if build_profile.is_none() {
            diagnostics.push(format!(
                "{RUST_BUILD_FEATURE_MANIFEST}: release_profiles[{row}] missing `profile`"
            ));
        }
        validate_profile_feature_set(profile, metadata, row, diagnostics);
        validate_profile_binaries(profile, metadata, row, diagnostics);
    }
    if !has_release {
        diagnostics.push(format!(
            "{RUST_BUILD_FEATURE_MANIFEST}: missing default `release` profile"
        ));
    }
    profiles.len()
}

/// Validates one profile's feature set against package metadata.
fn validate_profile_feature_set(
    profile: &Value,
    metadata: &Value,
    row: usize,
    diagnostics: &mut Vec<String>,
) {
    let Some(feature_set) = string_array(profile, "feature_set") else {
        diagnostics.push(format!(
            "{RUST_BUILD_FEATURE_MANIFEST}: release_profiles[{row}] missing `feature_set` array"
        ));
        return;
    };
    if profile.get("name").and_then(Value::as_str) == Some("release") {
        let Some(metadata_features) = string_array(metadata, "cargo_features") else {
            diagnostics.push("release metadata missing `cargo_features` array".to_string());
            return;
        };
        if metadata_features != feature_set {
            diagnostics.push(format!(
                "release metadata cargo_features {:?} does not match manifest feature_set {:?}",
                metadata_features, feature_set
            ));
        }
    }
}

/// Validates one profile's binary set against package metadata.
fn validate_profile_binaries(
    profile: &Value,
    metadata: &Value,
    row: usize,
    diagnostics: &mut Vec<String>,
) {
    let Some(binaries) = string_array(profile, "binaries") else {
        diagnostics.push(format!(
            "{RUST_BUILD_FEATURE_MANIFEST}: release_profiles[{row}] missing `binaries` array"
        ));
        return;
    };
    if binaries.is_empty() {
        diagnostics.push(format!(
            "{RUST_BUILD_FEATURE_MANIFEST}: release_profiles[{row}] must ship at least one binary"
        ));
    }
    if profile.get("name").and_then(Value::as_str) != Some("release") {
        return;
    }
    let metadata_binaries = metadata_binary_names(metadata);
    for binary in binaries {
        if !metadata_binaries.contains(&binary) {
            diagnostics.push(format!(
                "release metadata omitted promised binary `{binary}`"
            ));
        }
    }
}

/// Validates package helper metadata has exact release fields.
fn validate_release_metadata(metadata: &Value, diagnostics: &mut Vec<String>) -> usize {
    if metadata.get("schema").and_then(Value::as_str) != Some("terlan.release-artifact.v1") {
        diagnostics
            .push("release metadata schema must be `terlan.release-artifact.v1`".to_string());
    }
    for field in RELEASE_METADATA_FIELDS {
        if metadata.get(*field).is_none() {
            diagnostics.push(format!("release metadata missing `{field}`"));
        }
    }
    if nonempty_string(metadata, "target_triple").is_none() {
        diagnostics.push("release metadata `target_triple` must be non-empty".to_string());
    }
    if nonempty_string(metadata, "source_revision").is_none() {
        diagnostics.push("release metadata `source_revision` must be non-empty".to_string());
    }
    if metadata
        .get("crate_versions")
        .and_then(Value::as_object)
        .is_none()
    {
        diagnostics.push("release metadata `crate_versions` must be an object".to_string());
    }
    RELEASE_METADATA_FIELDS.len()
}

/// Validates the packaging helper is artifact-backed and metadata-aware.
fn validate_package_helper_text(root: &Path) -> QualityResult<Vec<String>> {
    let relative = PACKAGE_HELPER;
    let text = fs::read_to_string(root.join(relative))
        .map_err(|err| format!("{relative}: failed to read package helper: {err}"))?;
    let mut diagnostics = Vec::new();
    for required in [
        "RELEASE_METADATA_NAME",
        "write_release_metadata_to_dist",
        "archive.add(metadata_path",
        "archive.write(metadata_path",
        "\"metadata\"",
        "release_feature_set",
    ] {
        if !text.contains(required) {
            diagnostics.push(format!(
                "{relative}: missing artifact metadata hook `{required}`"
            ));
        }
    }
    Ok(diagnostics)
}

/// Returns metadata binary names.
fn metadata_binary_names(metadata: &Value) -> BTreeSet<String> {
    metadata
        .get("binaries")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|row| nonempty_string(row, "name"))
        .map(ToString::to_string)
        .collect()
}

/// Reads a non-empty string object field.
fn nonempty_string<'a>(value: &'a Value, field: &str) -> Option<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .filter(|text| !text.trim().is_empty())
}

/// Reads an array of strings from an object field.
fn string_array(value: &Value, field: &str) -> Option<Vec<String>> {
    value
        .get(field)?
        .as_array()?
        .iter()
        .map(|entry| entry.as_str().map(ToString::to_string))
        .collect()
}

#[cfg(test)]
#[path = "rust_build_feature_shipping_test.rs"]
mod rust_build_feature_shipping_test;
