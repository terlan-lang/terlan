use std::path::Path;

use super::model::{
    ProjectArtifactKind, ProjectWasiProfile, ProjectWasiTarget, ProjectWasmProfile,
    ProjectWasmTarget,
};
use super::strings::parse_string;

/// Finalizes optional `[target.wasm]` metadata.
///
/// Inputs:
/// - Fields collected while parsing a Wasm target section.
///
/// Output:
/// - `Ok(None)` when no Wasm target section is present and the artifact is not
///   Wasm.
/// - `Ok(Some(ProjectWasmTarget))` when the reserved Wasm target metadata is
///   complete.
/// - `Err(String)` when artifact/profile metadata is missing or inconsistent.
///
/// Transformation:
/// - Validates the manifest reservation without enabling Wasm byte emission.
#[allow(clippy::too_many_arguments)]
pub(super) fn finish_wasm_target(
    path: &Path,
    artifact: ProjectArtifactKind,
    seen: bool,
    profile: Option<ProjectWasmProfile>,
    exports: Option<Vec<String>>,
    bridge: Option<String>,
    capabilities: Option<Vec<String>>,
    world: Option<String>,
    validation_engine: Option<String>,
) -> Result<Option<ProjectWasmTarget>, String> {
    if !seen {
        if is_wasm_artifact(artifact) {
            return Err(format!(
                "{}: project manifest [build] artifact `{}` requires [target.wasm]",
                path.display(),
                artifact.as_str()
            ));
        }
        return Ok(None);
    }
    if !is_wasm_artifact(artifact) {
        return Err(format!(
            "{}: project manifest [target.wasm] requires [build] artifact wasm-core, wasm-browser, or wasm-component",
            path.display()
        ));
    }

    let profile = profile.ok_or_else(|| {
        format!(
            "{}: project manifest [target.wasm] requires profile",
            path.display()
        )
    })?;
    validate_wasm_artifact_profile(path, artifact, profile)?;
    let exports = validate_manifest_string_list(path, "[target.wasm] exports", exports)?;
    let capabilities =
        validate_manifest_string_list(path, "[target.wasm] capabilities", capabilities)?;
    let bridge = validate_optional_manifest_string(path, "[target.wasm] bridge", bridge)?;
    let world = validate_optional_manifest_string(path, "[target.wasm] world", world)?;
    let validation_engine = validate_optional_manifest_string(
        path,
        "[target.wasm] validation_engine",
        validation_engine,
    )?;

    Ok(Some(ProjectWasmTarget {
        profile,
        exports,
        bridge,
        capabilities,
        world,
        validation_engine,
    }))
}

/// Finalizes optional `[target.wasi]` metadata.
///
/// Inputs:
/// - Fields collected while parsing a WASI target section.
///
/// Output:
/// - `Ok(None)` when no WASI target section is present and the artifact is not
///   WASI.
/// - `Ok(Some(ProjectWasiTarget))` when the reserved WASI target metadata is
///   complete.
/// - `Err(String)` when artifact/profile metadata is missing or inconsistent.
///
/// Transformation:
/// - Validates the manifest reservation without enabling WASI component
///   emission.
pub(super) fn finish_wasi_target(
    path: &Path,
    artifact: ProjectArtifactKind,
    seen: bool,
    profile: Option<ProjectWasiProfile>,
    world: Option<String>,
    capabilities: Option<Vec<String>>,
    validation_engine: Option<String>,
) -> Result<Option<ProjectWasiTarget>, String> {
    if !seen {
        if is_wasi_artifact(artifact) {
            return Err(format!(
                "{}: project manifest [build] artifact `{}` requires [target.wasi]",
                path.display(),
                artifact.as_str()
            ));
        }
        return Ok(None);
    }
    if !is_wasi_artifact(artifact) {
        return Err(format!(
            "{}: project manifest [target.wasi] requires [build] artifact wasi-cli, wasi-http, or wasi-worker",
            path.display()
        ));
    }

    let profile = profile.ok_or_else(|| {
        format!(
            "{}: project manifest [target.wasi] requires profile",
            path.display()
        )
    })?;
    validate_wasi_artifact_profile(path, artifact, profile)?;
    let world = validate_optional_manifest_string(path, "[target.wasi] world", world)?;
    let capabilities =
        validate_manifest_string_list(path, "[target.wasi] capabilities", capabilities)?;
    let validation_engine = validate_optional_manifest_string(
        path,
        "[target.wasi] validation_engine",
        validation_engine,
    )?;

    Ok(Some(ProjectWasiTarget {
        profile,
        world,
        capabilities,
        validation_engine,
    }))
}

/// Parses a reserved Wasm target profile.
pub(super) fn parse_wasm_profile(
    value: &str,
    path: &Path,
    line_no: usize,
) -> Result<ProjectWasmProfile, String> {
    let parsed = parse_string(value, path, line_no)?;
    match parsed.as_str() {
        "core" => Ok(ProjectWasmProfile::Core),
        "browser" => Ok(ProjectWasmProfile::Browser),
        "component" => Ok(ProjectWasmProfile::Component),
        other => Err(format!(
            "{}:{}: unsupported [target.wasm] profile `{}`; supported profiles: core, browser, component",
            path.display(),
            line_no,
            other
        )),
    }
}

/// Parses a reserved WASI target profile.
pub(super) fn parse_wasi_profile(
    value: &str,
    path: &Path,
    line_no: usize,
) -> Result<ProjectWasiProfile, String> {
    let parsed = parse_string(value, path, line_no)?;
    match parsed.as_str() {
        "cli" => Ok(ProjectWasiProfile::Cli),
        "http" => Ok(ProjectWasiProfile::Http),
        "worker" => Ok(ProjectWasiProfile::Worker),
        other => Err(format!(
            "{}:{}: unsupported [target.wasi] profile `{}`; supported profiles: cli, http, worker",
            path.display(),
            line_no,
            other
        )),
    }
}

/// Returns whether the artifact belongs to the reserved Wasm family.
fn is_wasm_artifact(artifact: ProjectArtifactKind) -> bool {
    matches!(
        artifact,
        ProjectArtifactKind::WasmCore
            | ProjectArtifactKind::WasmBrowser
            | ProjectArtifactKind::WasmComponent
    )
}

/// Returns whether the artifact belongs to the reserved WASI family.
fn is_wasi_artifact(artifact: ProjectArtifactKind) -> bool {
    matches!(
        artifact,
        ProjectArtifactKind::WasiCli
            | ProjectArtifactKind::WasiHttp
            | ProjectArtifactKind::WasiWorker
    )
}

/// Validates that a reserved Wasm artifact matches its target profile.
fn validate_wasm_artifact_profile(
    path: &Path,
    artifact: ProjectArtifactKind,
    profile: ProjectWasmProfile,
) -> Result<(), String> {
    let matches = matches!(
        (artifact, profile),
        (ProjectArtifactKind::WasmCore, ProjectWasmProfile::Core)
            | (
                ProjectArtifactKind::WasmBrowser,
                ProjectWasmProfile::Browser
            )
            | (
                ProjectArtifactKind::WasmComponent,
                ProjectWasmProfile::Component
            )
    );
    if matches {
        Ok(())
    } else {
        Err(format!(
            "{}: project manifest [build] artifact `{}` does not match [target.wasm] profile `{}`",
            path.display(),
            artifact.as_str(),
            profile.as_str()
        ))
    }
}

/// Validates that a reserved WASI artifact matches its target profile.
fn validate_wasi_artifact_profile(
    path: &Path,
    artifact: ProjectArtifactKind,
    profile: ProjectWasiProfile,
) -> Result<(), String> {
    let matches = matches!(
        (artifact, profile),
        (ProjectArtifactKind::WasiCli, ProjectWasiProfile::Cli)
            | (ProjectArtifactKind::WasiHttp, ProjectWasiProfile::Http)
            | (ProjectArtifactKind::WasiWorker, ProjectWasiProfile::Worker)
    );
    if matches {
        Ok(())
    } else {
        Err(format!(
            "{}: project manifest [build] artifact `{}` does not match [target.wasi] profile `{}`",
            path.display(),
            artifact.as_str(),
            profile.as_str()
        ))
    }
}

/// Validates an optional manifest string field.
fn validate_optional_manifest_string(
    path: &Path,
    field: &str,
    value: Option<String>,
) -> Result<Option<String>, String> {
    if let Some(value) = value {
        if value.trim().is_empty() {
            return Err(format!(
                "{}: project manifest {} cannot be empty",
                path.display(),
                field
            ));
        }
        Ok(Some(value))
    } else {
        Ok(None)
    }
}

/// Validates an optional manifest string-list field.
fn validate_manifest_string_list(
    path: &Path,
    field: &str,
    values: Option<Vec<String>>,
) -> Result<Vec<String>, String> {
    let values = values.unwrap_or_default();
    if values.iter().any(|value| value.trim().is_empty()) {
        return Err(format!(
            "{}: project manifest {} cannot contain empty entries",
            path.display(),
            field
        ));
    }
    Ok(values)
}
