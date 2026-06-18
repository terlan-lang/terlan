#![allow(dead_code)]

// Consumed by the upcoming `terlc build --target js` implementation; tests pin
// the release contract before the build command writes JS artifacts.

use std::path::PathBuf;

use crate::validation::target_profile::TargetProfile;

/// Default JS build target spelling accepted by future `terlc build --target`.
pub(crate) const JS_TARGET_ALIAS: &str = "js";

/// Shared JS profile spelling selected by `--target js`.
pub(crate) const JS_SHARED_PROFILE: &str = "js.shared";

/// Browser JS profile spelling for DOM-capable generated bindings.
pub(crate) const JS_BROWSER_PROFILE: &str = "js.browser";

/// Worker JS profile spelling for worker-safe generated bindings.
pub(crate) const JS_WORKER_PROFILE: &str = "js.worker";

/// Default build root for JavaScript artifacts.
pub(crate) const JS_BUILD_ROOT: &str = "_build/js";

/// Relative directory that owns emitted JavaScript modules.
pub(crate) const JS_MODULES_DIR: &str = "modules";

/// Relative directory that owns JS target metadata.
pub(crate) const JS_METADATA_DIR: &str = "metadata";

/// JavaScript target manifest file name.
pub(crate) const JS_MANIFEST_FILE: &str = "manifest.json";

/// JavaScript target-profile metadata file name.
pub(crate) const JS_TARGET_PROFILE_FILE: &str = "target-profile.json";

/// JavaScript target diagnostics metadata file name.
pub(crate) const JS_DIAGNOSTICS_FILE: &str = "diagnostics.json";

/// File extension used for JavaScript modules in 0.0.4.
pub(crate) const JS_MODULE_EXTENSION: &str = "js";

/// File extension used for TypeScript declaration artifacts in 0.0.4.
pub(crate) const JS_DECLARATION_EXTENSION: &str = "d.ts";

/// Module format emitted by the 0.0.4 JavaScript target.
pub(crate) const JS_MODULE_FORMAT: &str = "es-module";

/// Diagnostic family for unsupported JS target features.
pub(crate) const JS_UNSUPPORTED_FEATURE_CODE: &str = "js_emit_unsupported";

/// Immutable JavaScript target contract for one selected profile.
///
/// Inputs:
/// - A normalized `TargetProfile` selected by the CLI or build command.
///
/// Output:
/// - Backend-owned artifact layout and target-profile metadata.
///
/// Transformation:
/// - Carries release-contract constants from roadmap prose into a typed shape
///   that later build and manifest code can consume without duplicating string
///   literals.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct JsTargetContract {
    pub(crate) profile: TargetProfile,
    pub(crate) profile_name: &'static str,
    pub(crate) build_root: &'static str,
    pub(crate) modules_dir: &'static str,
    pub(crate) metadata_dir: &'static str,
    pub(crate) manifest_file: &'static str,
    pub(crate) target_profile_file: &'static str,
    pub(crate) diagnostics_file: &'static str,
    pub(crate) module_extension: &'static str,
    pub(crate) declaration_extension: &'static str,
    pub(crate) module_format: &'static str,
    pub(crate) unsupported_feature_code: &'static str,
}

/// Parses a JS build target spelling into the corresponding target profile.
///
/// Inputs:
/// - `value`: user-facing target string from future `terlc build --target`.
///
/// Output:
/// - `Some(TargetProfile)` for the 0.0.4 JS target family.
/// - `None` for non-JS or unsupported target names.
///
/// Transformation:
/// - Normalizes the short `js` alias to `js.shared` and keeps browser/worker
///   profiles explicit.
pub(crate) fn parse_js_build_target_profile(value: &str) -> Option<TargetProfile> {
    match value {
        JS_TARGET_ALIAS | JS_SHARED_PROFILE => Some(TargetProfile::JsShared),
        JS_BROWSER_PROFILE => Some(TargetProfile::JsBrowser),
        JS_WORKER_PROFILE => Some(TargetProfile::JsWorker),
        _ => None,
    }
}

/// Returns the JavaScript target contract for a normalized profile.
///
/// Inputs:
/// - `profile`: target profile selected by CLI parsing or build target parsing.
///
/// Output:
/// - `Some(JsTargetContract)` for JS profiles.
/// - `None` for Erlang, CoreIR, native, or future non-JS profiles.
///
/// Transformation:
/// - Converts the normalized profile into a stable artifact-layout contract.
pub(crate) fn js_target_contract(profile: TargetProfile) -> Option<JsTargetContract> {
    if !profile.is_js() {
        return None;
    }

    Some(JsTargetContract {
        profile,
        profile_name: profile.as_str(),
        build_root: JS_BUILD_ROOT,
        modules_dir: JS_MODULES_DIR,
        metadata_dir: JS_METADATA_DIR,
        manifest_file: JS_MANIFEST_FILE,
        target_profile_file: JS_TARGET_PROFILE_FILE,
        diagnostics_file: JS_DIAGNOSTICS_FILE,
        module_extension: JS_MODULE_EXTENSION,
        declaration_extension: JS_DECLARATION_EXTENSION,
        module_format: JS_MODULE_FORMAT,
        unsupported_feature_code: JS_UNSUPPORTED_FEATURE_CODE,
    })
}

/// Computes the relative module artifact path for a Terlan module.
///
/// Inputs:
/// - `module_path`: Terlan module path such as `examples.js.Add`.
///
/// Output:
/// - Relative path under `_build/js/modules`, with package segments converted
///   to directories and the final segment emitted as `.js`.
///
/// Transformation:
/// - Splits Terlan module path segments on `.`, appends them under the JS
///   modules directory, and applies the 0.0.4 plain `.js` extension.
pub(crate) fn js_module_artifact_relative_path(module_path: &str) -> PathBuf {
    let mut path = PathBuf::from(JS_MODULES_DIR);
    for segment in module_path.split('.').filter(|segment| !segment.is_empty()) {
        path.push(segment);
    }
    path.set_extension(JS_MODULE_EXTENSION);
    path
}

/// Computes the relative TypeScript declaration artifact path for a module.
///
/// Inputs:
/// - `module_path`: Terlan module path such as `examples.js.Add`.
///
/// Output:
/// - Relative path under `_build/js/modules`, with package segments converted
///   to directories and the final segment emitted as `.d.ts`.
///
/// Transformation:
/// - Reuses the JS module path mapping and swaps only the extension so `.js`
///   artifacts and `.d.ts` declarations stay beside each other.
pub(crate) fn js_declaration_artifact_relative_path(module_path: &str) -> PathBuf {
    let mut path = js_module_artifact_relative_path(module_path);
    path.set_extension(JS_DECLARATION_EXTENSION);
    path
}

/// Computes the metadata path for a JS target metadata file.
///
/// Inputs:
/// - `file_name`: metadata file name from the JS target contract.
///
/// Output:
/// - Relative path under `_build/js/metadata`.
///
/// Transformation:
/// - Keeps metadata layout construction centralized for future manifest and
///   diagnostics writers.
pub(crate) fn js_metadata_relative_path(file_name: &str) -> PathBuf {
    let mut path = PathBuf::from(JS_METADATA_DIR);
    path.push(file_name);
    path
}
