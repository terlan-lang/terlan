use crate::validation::target_profile::{
    target_profile_std_module_import_error_with_options, TargetFamily, TargetProfile,
    TargetProfileCheckOptions,
};

/// Returns whether a target profile can produce Erlang artifacts.
///
/// Inputs:
/// - `profile`: globally selected target-profile gate.
///
/// Output:
/// - `true` when the profile is Erlang-compatible.
///
/// Transformation:
/// - Treats the general `erlang` profile and release-slice `*-erlang` profiles
///   as valid build gates, while rejecting backend-agnostic profiles such as
///   `core-v0`.
pub(super) fn target_profile_supports_erlang_backend(profile: TargetProfile) -> bool {
    profile.family() == TargetFamily::Beam
}

/// Rejects native package module source on the Erlang backend.
///
/// Inputs:
/// - `path`: source path used for diagnostics.
/// - `source`: Terlan source text to inspect before formal lowering.
///
/// Output:
/// - `Ok(())` when the source does not declare a `std.native.*` module.
/// - `Err(String)` with a stable target-capability diagnostic when a native
///   package module declaration is present.
///
/// Transformation:
/// - Performs a conservative textual boundary check before Erlang emission so
///   native package implementation modules do not compile as ordinary BEAM
///   modules. Native imports are target-profile checked separately because
///   selected native APIs can be reached through SafeNative boundary modules.
pub(super) fn reject_erlang_native_package_source(path: &str, source: &str) -> Result<(), String> {
    if source.contains("module std.native.") {
        return Err(format!(
            "terlc build --target erlang cannot compile native package module `{path}`; `std.native` packages require the Rust/native target capability"
        ));
    }
    Ok(())
}

/// Rejects std imports that the selected target profile cannot execute.
///
/// Inputs:
/// - `path`: source path used for diagnostics.
/// - `source`: Terlan source text to scan for module and import declarations.
/// - `profile`: backend capability profile selected by the build command.
/// - `options`: command-owned validation switches for target-gated imports.
///
/// Output:
/// - `Ok(())` when all discovered target-gated std modules are supported.
/// - `Err(String)` with the first stable target-profile diagnostic.
///
/// Transformation:
/// - Extracts top-level `module`, executable `import`, and `import type`
///   declaration paths from raw source text, then delegates std-family support
///   decisions to target-profile validation. Type-only imports are included
///   because generated platform bindings can still leak target-specific type
///   contracts into incompatible build targets.
pub(super) fn reject_unsupported_target_std_source(
    path: &str,
    source: &str,
    profile: TargetProfile,
    options: TargetProfileCheckOptions,
) -> Result<(), String> {
    let context = format!("source `{path}`");
    for module in source_declared_or_imported_modules(source) {
        if let Some(message) =
            target_profile_std_module_import_error_with_options(profile, options, &context, &module)
        {
            return Err(format!("terlc build target-profile error: {message}"));
        }
    }
    Ok(())
}

/// Extracts declaration module paths from source text for build preflight gates.
///
/// Inputs:
/// - `source`: Terlan source text.
///
/// Output:
/// - Ordered module paths mentioned by top-level `module` and executable
///   `import` declarations.
///
/// Transformation:
/// - Performs a lightweight line-oriented scan and normalizes trailing
///   statement dots or selective-import braces. It handles `import type`
///   explicitly so platform-specific type contracts are checked against the
///   selected build target before the formal parser and interface loader run.
fn source_declared_or_imported_modules(source: &str) -> Vec<String> {
    source
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("module ") {
                source_declaration_path(rest)
            } else if let Some(rest) = trimmed.strip_prefix("import type ") {
                source_declaration_path(rest)
            } else if let Some(rest) = trimmed.strip_prefix("import ") {
                source_declaration_path(rest)
            } else {
                None
            }
        })
        .collect()
}

/// Normalizes one module path fragment from a declaration line.
///
/// Inputs:
/// - `rest`: declaration text after the keyword prefix.
///
/// Output:
/// - `Some(String)` for a non-empty module path candidate.
/// - `None` when the declaration has no path token.
///
/// Transformation:
/// - Takes the first whitespace-delimited token, strips the statement
///   terminator, and removes selective-import suffixes such as `.{println}` so
///   target-family matching sees the declared module identity.
fn source_declaration_path(rest: &str) -> Option<String> {
    let token = rest.split_whitespace().next()?.trim_end_matches('.');
    let module = token
        .split(".{")
        .next()
        .unwrap_or(token)
        .trim_end_matches('.');
    if module.is_empty() {
        None
    } else {
        Some(module.to_string())
    }
}
