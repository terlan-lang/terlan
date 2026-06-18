use std::path::PathBuf;

use super::target_contract::{
    js_declaration_artifact_relative_path, js_metadata_relative_path,
    js_module_artifact_relative_path, js_target_contract, parse_js_build_target_profile,
    JS_BROWSER_PROFILE, JS_BUILD_ROOT, JS_DECLARATION_EXTENSION, JS_DIAGNOSTICS_FILE,
    JS_MANIFEST_FILE, JS_METADATA_DIR, JS_MODULES_DIR, JS_MODULE_FORMAT, JS_SHARED_PROFILE,
    JS_TARGET_ALIAS, JS_TARGET_PROFILE_FILE, JS_UNSUPPORTED_FEATURE_CODE, JS_WORKER_PROFILE,
};
use crate::validation::target_profile::TargetProfile;

/// Verifies JavaScript target spellings normalize to release profiles.
///
/// Inputs:
/// - The short `js` alias and each explicit 0.0.4 JavaScript profile spelling.
///
/// Output:
/// - Assertions over normalized `TargetProfile` values.
///
/// Transformation:
/// - Confirms J0.1 profile names are centralized before `terlc build --target
///   js` begins consuming them.
#[test]
fn js_target_contract_parses_profile_spellings() {
    assert_eq!(
        parse_js_build_target_profile(JS_TARGET_ALIAS),
        Some(TargetProfile::JsShared)
    );
    assert_eq!(
        parse_js_build_target_profile(JS_SHARED_PROFILE),
        Some(TargetProfile::JsShared)
    );
    assert_eq!(
        parse_js_build_target_profile(JS_BROWSER_PROFILE),
        Some(TargetProfile::JsBrowser)
    );
    assert_eq!(
        parse_js_build_target_profile(JS_WORKER_PROFILE),
        Some(TargetProfile::JsWorker)
    );
    assert_eq!(parse_js_build_target_profile("erlang"), None);
}

/// Verifies the JavaScript target contract owns the 0.0.4 artifact layout.
///
/// Inputs:
/// - The shared JS target profile.
///
/// Output:
/// - Assertions over build-root, module, metadata, manifest, and diagnostic
///   constants.
///
/// Transformation:
/// - Converts the normalized profile into the typed contract that future build
///   code will use to write `_build/js` artifacts.
#[test]
fn js_target_contract_records_artifact_layout() {
    let contract = js_target_contract(TargetProfile::JsShared).expect("JS shared contract");

    assert_eq!(contract.profile, TargetProfile::JsShared);
    assert_eq!(contract.profile_name, JS_SHARED_PROFILE);
    assert_eq!(contract.build_root, JS_BUILD_ROOT);
    assert_eq!(contract.modules_dir, JS_MODULES_DIR);
    assert_eq!(contract.metadata_dir, JS_METADATA_DIR);
    assert_eq!(contract.manifest_file, JS_MANIFEST_FILE);
    assert_eq!(contract.target_profile_file, JS_TARGET_PROFILE_FILE);
    assert_eq!(contract.diagnostics_file, JS_DIAGNOSTICS_FILE);
    assert_eq!(contract.declaration_extension, JS_DECLARATION_EXTENSION);
    assert_eq!(contract.module_format, JS_MODULE_FORMAT);
    assert_eq!(
        contract.unsupported_feature_code,
        JS_UNSUPPORTED_FEATURE_CODE
    );
    assert_eq!(
        js_target_contract(TargetProfile::Erlang),
        None,
        "non-JS profiles must not expose a JS artifact contract"
    );
}

/// Verifies Terlan module paths map to deterministic JavaScript artifacts.
///
/// Inputs:
/// - A package-rooted Terlan module path.
///
/// Output:
/// - Relative artifact and metadata paths.
///
/// Transformation:
/// - Converts dot-separated module segments into the 0.0.4 `_build/js`
///   directory layout without flattening names.
#[test]
fn js_target_contract_computes_relative_paths() {
    assert_eq!(
        js_module_artifact_relative_path("examples.js.Add"),
        PathBuf::from("modules/examples/js/Add.js")
    );
    assert_eq!(
        js_declaration_artifact_relative_path("examples.js.Add"),
        PathBuf::from("modules/examples/js/Add.d.ts")
    );
    assert_eq!(
        js_metadata_relative_path(JS_TARGET_PROFILE_FILE),
        PathBuf::from("metadata/target-profile.json")
    );
    assert_eq!(
        js_metadata_relative_path(JS_DIAGNOSTICS_FILE),
        PathBuf::from("metadata/diagnostics.json")
    );
}
