use serde_json::json;

use super::*;

/// Verifies Cargo feature parsing reads only the `[features]` section.
///
/// Inputs:
/// - Cargo manifest text with dependency feature lists and package features.
///
/// Output:
/// - Parsed feature rows for package features only.
///
/// Transformation:
/// - Prevents dependency feature syntax from being mistaken for shipped
///   Terlan build features.
#[test]
fn cargo_feature_parser_ignores_dependency_feature_options() {
    let text = r#"
[dependencies]
serde = { version = "1", features = ["derive"] }

[features]
default = ["http"]
http = []

[dev-dependencies]
wat = "1"
"#;

    let features = parse_cargo_features(text);

    assert_eq!(features.get("default"), Some(&"[\"http\"]".to_string()));
    assert_eq!(features.get("http"), Some(&"[]".to_string()));
    assert!(!features.contains_key("serde"));
}

/// Verifies manifest validation rejects unclassified Cargo features.
///
/// Inputs:
/// - A feature manifest with no feature rows.
/// - Cargo feature set containing `http`.
///
/// Output:
/// - Diagnostic naming the unclassified Cargo feature.
///
/// Transformation:
/// - Makes adding `[features]` to Cargo.toml a release-contract change rather
///   than an invisible local build option.
#[test]
fn feature_manifest_rejects_unclassified_cargo_feature() {
    let manifest = json!({
        "schema": "terlan.rust-build-features.v1",
        "package": "terlan",
        "default_features": [],
        "features": []
    });
    let cargo_features = BTreeSet::from(["http".to_string()]);
    let mut diagnostics = Vec::new();

    validate_feature_manifest(&manifest, &cargo_features, &mut diagnostics);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("Cargo feature `http`")),
        "diagnostics should report unclassified feature: {diagnostics:?}"
    );
}

/// Verifies release profile validation rejects feature-set drift.
///
/// Inputs:
/// - Feature manifest promising the default release feature set.
/// - Release metadata that records a different Cargo feature set.
///
/// Output:
/// - Diagnostic explaining the metadata and manifest mismatch.
///
/// Transformation:
/// - Ensures the package helper cannot silently ship binaries built with a
///   different feature set than the manifest promises.
#[test]
fn release_profile_rejects_metadata_feature_set_mismatch() {
    let manifest = json!({
        "release_profiles": [
            {
                "name": "release",
                "cargo_package": "terlan",
                "profile": "release",
                "feature_set": [],
                "binaries": ["terlc", "terlan-vm"]
            }
        ]
    });
    let metadata = json!({
        "cargo_features": ["http"],
        "binaries": [
            {"name": "terlc", "path": "terlc"},
            {"name": "terlan-vm", "path": "terlan-vm"}
        ]
    });
    let mut diagnostics = Vec::new();

    validate_release_profiles(&manifest, &metadata, &mut diagnostics);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("does not match manifest feature_set")),
        "diagnostics should report feature-set drift: {diagnostics:?}"
    );
}

/// Verifies release profile validation rejects omitted shipped binaries.
///
/// Inputs:
/// - Feature manifest promising `terlc` and `terlan-vm`.
/// - Release metadata containing only `terlc`.
///
/// Output:
/// - Diagnostic naming the omitted VM binary.
///
/// Transformation:
/// - Keeps release artifacts from falling back to compiler-only packaging.
#[test]
fn release_profile_rejects_missing_promised_binary() {
    let manifest = json!({
        "release_profiles": [
            {
                "name": "release",
                "cargo_package": "terlan",
                "profile": "release",
                "feature_set": [],
                "binaries": ["terlc", "terlan-vm"]
            }
        ]
    });
    let metadata = json!({
        "cargo_features": [],
        "binaries": [
            {"name": "terlc", "path": "terlc"}
        ]
    });
    let mut diagnostics = Vec::new();

    validate_release_profiles(&manifest, &metadata, &mut diagnostics);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("promised binary `terlan-vm`")),
        "diagnostics should report missing VM binary: {diagnostics:?}"
    );
}

/// Verifies release metadata requires exact build fields.
///
/// Inputs:
/// - Metadata missing the target triple and source revision.
///
/// Output:
/// - Diagnostics for missing exact build metadata.
///
/// Transformation:
/// - Prevents release artifacts from being validated only as local workspace
///   builds.
#[test]
fn release_metadata_rejects_missing_exact_build_fields() {
    let metadata = json!({
        "schema": "terlan.release-artifact.v1",
        "cargo_features": [],
        "profile": "release",
        "crate_versions": {},
        "binaries": []
    });
    let mut diagnostics = Vec::new();

    validate_release_metadata(&metadata, &mut diagnostics);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("target_triple")),
        "diagnostics should report missing target triple: {diagnostics:?}"
    );
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("source_revision")),
        "diagnostics should report missing source revision: {diagnostics:?}"
    );
}
