use super::ts_input_manifest::load_ts_input_manifest;

/// Current committed TypeScript DOM input manifest path used by tests.
const STD_JS_DOM_INPUT_MANIFEST: &str = "std/js/manifests/std_js_dom_inputs.json";

/// Returns the repository root for bind-command tests.
///
/// Inputs:
/// - None.
///
/// Output:
/// - Absolute repository root path.
///
/// Transformation:
/// - Starts from `crates/terlan_cli` and walks two parents to the repository
///   root used by committed std fixtures.
fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repo root")
}

/// Verifies the committed `std.js` DOM TypeScript input manifest is valid.
///
/// Inputs:
/// - The committed manifest at `STD_JS_DOM_INPUT_MANIFEST`.
/// - The committed tiny TypeScript DOM fixture referenced by that manifest.
///
/// Output:
/// - Test passes when required T0.1 metadata and the pinned SHA-256 match.
///
/// Transformation:
/// - Runs the same manifest validator intended for release preflight so input
///   pinning is checked before TypeScript parser ingestion exists.
#[test]
fn committed_std_js_dom_input_manifest_is_valid() {
    load_ts_input_manifest(
        &repo_root(),
        std::path::Path::new(STD_JS_DOM_INPUT_MANIFEST),
    )
    .unwrap_or_else(|err| {
        panic!(
            "expected committed TypeScript input manifest `{}` to validate: {err}",
            STD_JS_DOM_INPUT_MANIFEST
        )
    });
}
