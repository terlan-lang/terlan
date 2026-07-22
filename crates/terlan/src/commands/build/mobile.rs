use std::fs;
use std::path::Path;
use std::process::ExitCode;

use serde::Serialize;
use serde_json::json;

use crate::mobile::mobile_android_shell::{generate_android_shell_layout, AndroidShellConfig};
use crate::mobile::mobile_bridge::generate_mobile_bridge_metadata;
use crate::mobile::mobile_capability::generate_mobile_native_service_capability_resources;
use crate::mobile::mobile_ios_shell::{generate_ios_shell_layout, IosShellConfig};
use crate::mobile::mobile_route::generate_mobile_route_configuration;
use crate::CliState;

use super::args::MobileBuildTarget;
use super::{write_build_file, BuildArgs};

const MOBILE_BUILD_PLAN_SCHEMA: &str = "terlan-mobile-build-plan-v1";
const MOBILE_PLANNING_STATUS: &str = "planning_metadata_available";
const MOBILE_INCOMPLETE_DIAGNOSTIC_CODE: &str = "mobile_shell_emission_experimental";

/// Mobile shell build plan emitted before native shell tooling runs.
///
/// Inputs:
/// - Source path and selected mobile target.
///
/// Output:
/// - Serializable build plan stored under `_build/mobile/<platform>/plan.json`.
///
/// Transformation:
/// - Records command intent without routing mobile builds through Vm, JS, or
///   native shell build tools.
#[derive(Debug, Serialize)]
struct MobileBuildPlan {
    schema: &'static str,
    target: &'static str,
    platform: &'static str,
    status: &'static str,
    diagnostic_code: &'static str,
    diagnostic_message: String,
    source_path: String,
    shell_generation: &'static str,
    native_shell_project: Option<String>,
    web_output: Option<String>,
    route_manifest: Option<String>,
    bridge_manifest: Option<String>,
    capability_manifest: Option<String>,
    native_shell_config: Option<String>,
    source_identity_metadata: Option<String>,
}

/// Relative metadata paths emitted for one mobile shell plan.
///
/// Inputs:
/// - Created by writing mobile metadata artifacts.
///
/// Output:
/// - Relative paths stored in the mobile build plan.
///
/// Transformation:
/// - Keeps plan serialization separate from metadata file writes.
struct MobileShellMetadataPaths {
    route_manifest: String,
    bridge_manifest: String,
    capability_manifest: String,
    native_shell_config: String,
    source_identity_metadata: String,
}

/// Runs mobile build planning.
///
/// Inputs:
/// - `args`: parsed build command arguments.
/// - `state`: global CLI state used for output paths and incremental writes.
/// - `target`: selected mobile platform.
///
/// Output:
/// - CLI exit code representing plan emission success or failure.
///
/// Transformation:
/// - Emits a deterministic mobile plan artifact and intentionally does not run
///   JS bundling, Android Gradle, or Apple build tooling yet.
pub(super) fn run_mobile_build(
    args: &BuildArgs,
    state: &CliState,
    target: MobileBuildTarget,
) -> ExitCode {
    match write_mobile_build_plan(args, state, target) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

/// Writes the mobile build plan artifact.
///
/// Inputs:
/// - `args`: parsed build command arguments.
/// - `state`: global CLI state.
/// - `target`: selected mobile platform.
///
/// Output:
/// - `Ok(())` after the plan exists, unless `--no-emit` is active.
/// - `Err(message)` when serialization or writing fails.
///
/// Transformation:
/// - Maps platform selection to a stable output path and serializes the first
///   mobile shell planning contract.
fn write_mobile_build_plan(
    args: &BuildArgs,
    state: &CliState,
    target: MobileBuildTarget,
) -> Result<(), String> {
    let (target_name, platform) = mobile_target_metadata(target);
    let plan = MobileBuildPlan {
        schema: MOBILE_BUILD_PLAN_SCHEMA,
        target: target_name,
        platform,
        status: MOBILE_PLANNING_STATUS,
        diagnostic_code: MOBILE_INCOMPLETE_DIAGNOSTIC_CODE,
        diagnostic_message: mobile_planning_diagnostic_message(target_name),
        source_path: Path::new(&args.path).to_string_lossy().into_owned(),
        shell_generation: "planned",
        native_shell_project: None,
        web_output: None,
        route_manifest: None,
        bridge_manifest: None,
        capability_manifest: None,
        native_shell_config: None,
        source_identity_metadata: None,
    };

    if state.no_emit {
        return Ok(());
    }

    let plan_dir = state.out_dir.join("mobile").join(platform);
    fs::create_dir_all(&plan_dir)
        .map_err(|err| format!("failed to create {}: {err}", plan_dir.display()))?;

    let packaged_web_output = package_mobile_web_output(state, platform)?;
    let native_shell_project = emit_mobile_shell_project_layout(&plan_dir, target, state)?;
    let metadata_paths = emit_mobile_shell_metadata(&plan_dir, target_name, platform, state)?;
    let plan = MobileBuildPlan {
        native_shell_project: Some(native_shell_project),
        web_output: packaged_web_output,
        route_manifest: Some(metadata_paths.route_manifest),
        bridge_manifest: Some(metadata_paths.bridge_manifest),
        capability_manifest: Some(metadata_paths.capability_manifest),
        native_shell_config: Some(metadata_paths.native_shell_config),
        source_identity_metadata: Some(metadata_paths.source_identity_metadata),
        ..plan
    };

    let json = serde_json::to_string_pretty(&plan)
        .map_err(|err| format!("failed to serialize mobile build plan: {err}"))?;
    write_build_file(
        &plan_dir.join("plan.json"),
        format!("{json}\n").as_bytes(),
        state.incremental,
    )
}

/// Emits the generated native shell project layout.
///
/// Inputs:
/// - `plan_dir`: platform-specific mobile output directory.
/// - `target`: selected mobile platform.
/// - `state`: global CLI state for incremental writes.
///
/// Output:
/// - Relative shell project directory path stored in the mobile plan.
/// - `Err(message)` when shell layout generation or file writes fail.
///
/// Transformation:
/// - Reuses compiler-owned Android/iOS shell layout generators and writes their
///   template files under `_build/mobile/<platform>/shell`.
fn emit_mobile_shell_project_layout(
    plan_dir: &Path,
    target: MobileBuildTarget,
    state: &CliState,
) -> Result<String, String> {
    let relative_shell = "shell";
    let shell_dir = plan_dir.join(relative_shell);
    if shell_dir.exists() {
        fs::remove_dir_all(&shell_dir)
            .map_err(|err| format!("failed to clean {}: {err}", shell_dir.display()))?;
    }

    match target {
        MobileBuildTarget::Android => {
            let layout = generate_android_shell_layout(&AndroidShellConfig {
                app_name: "TerlanMobile".to_string(),
                application_id: "io.terlan.mobile".to_string(),
                kotlin_package: "io.terlan.mobile".to_string(),
            })
            .map_err(debug_diagnostics)?;
            for file in layout.files {
                write_shell_layout_file(&shell_dir, &file.path, &file.contents, state.incremental)?;
            }
        }
        MobileBuildTarget::Ios => {
            let layout = generate_ios_shell_layout(&IosShellConfig {
                app_name: "TerlanMobile".to_string(),
                bundle_id: "io.terlan.mobile".to_string(),
                swift_module_prefix: "TerlanMobile".to_string(),
            })
            .map_err(debug_diagnostics)?;
            for file in layout.files {
                write_shell_layout_file(&shell_dir, &file.path, &file.contents, state.incremental)?;
            }
        }
    }

    Ok(relative_shell.to_string())
}

/// Writes one generated native shell layout file.
///
/// Inputs:
/// - `shell_dir`: platform shell project root.
/// - `relative_path`: generator-owned relative file path.
/// - `contents`: generated source text.
/// - `incremental`: whether unchanged writes may be skipped.
///
/// Output:
/// - `Ok(())` after the file exists.
/// - `Err(message)` when parent directory creation or writing fails.
///
/// Transformation:
/// - Creates parent directories and delegates file contents to the shared build
///   writer.
fn write_shell_layout_file(
    shell_dir: &Path,
    relative_path: &str,
    contents: &str,
    incremental: bool,
) -> Result<(), String> {
    let path = shell_dir.join(relative_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    write_build_file(&path, contents.as_bytes(), incremental)
}

/// Emits shell metadata artifacts for one mobile target.
///
/// Inputs:
/// - `plan_dir`: platform-specific mobile output directory.
/// - `target_name`: full CLI target spelling.
/// - `platform`: platform path segment.
/// - `state`: global CLI state for incremental writes.
///
/// Output:
/// - Relative metadata paths suitable for the mobile plan.
/// - `Err(message)` when metadata generation, serialization, or writing fails.
///
/// Transformation:
/// - Writes empty-but-typed route, bridge, capability, native shell, and source
///   identity metadata using the existing compiler-owned mobile contracts.
fn emit_mobile_shell_metadata(
    plan_dir: &Path,
    target_name: &str,
    platform: &str,
    state: &CliState,
) -> Result<MobileShellMetadataPaths, String> {
    let metadata_dir = plan_dir.join("metadata");
    fs::create_dir_all(&metadata_dir)
        .map_err(|err| format!("failed to create {}: {err}", metadata_dir.display()))?;

    let routes = generate_mobile_route_configuration(&[]).map_err(debug_diagnostics)?;
    let bridge = generate_mobile_bridge_metadata(&[]).map_err(debug_diagnostics)?;
    let capabilities =
        generate_mobile_native_service_capability_resources(&[]).map_err(debug_diagnostics)?;

    write_json_file(
        &metadata_dir.join("routes.json"),
        &json!({
            "schema_version": routes.schema_version,
            "routes": [],
        }),
        state.incremental,
    )?;
    write_json_file(
        &metadata_dir.join("bridge.json"),
        &json!({
            "schema_version": bridge.schema_version,
            "declarations": [],
        }),
        state.incremental,
    )?;
    write_json_file(
        &metadata_dir.join("capabilities.json"),
        &json!({
            "schema_version": capabilities.schema_version,
            "resources": [],
        }),
        state.incremental,
    )?;
    write_json_file(
        &metadata_dir.join("native-shell.json"),
        &json!({
            "schema": "terlan-mobile-native-shell-config-v1",
            "target": target_name,
            "platform": platform,
            "native_shell_project": "shell",
            "web_input": "shell-inputs/web",
            "route_manifest": "metadata/routes.json",
            "bridge_manifest": "metadata/bridge.json",
            "capability_manifest": "metadata/capabilities.json",
            "source_identity_metadata": "metadata/source-identities.json",
        }),
        state.incremental,
    )?;
    write_json_file(
        &metadata_dir.join("source-identities.json"),
        &json!({
            "schema": "terlan-mobile-source-identities-v1",
            "identities": [],
        }),
        state.incremental,
    )?;

    Ok(MobileShellMetadataPaths {
        route_manifest: "metadata/routes.json".to_string(),
        bridge_manifest: "metadata/bridge.json".to_string(),
        capability_manifest: "metadata/capabilities.json".to_string(),
        native_shell_config: "metadata/native-shell.json".to_string(),
        source_identity_metadata: "metadata/source-identities.json".to_string(),
    })
}

/// Writes a JSON metadata artifact.
///
/// Inputs:
/// - `path`: destination path.
/// - `value`: serializable JSON value.
/// - `incremental`: whether unchanged writes may be skipped.
///
/// Output:
/// - `Ok(())` after the file exists.
/// - `Err(message)` when serialization or writing fails.
///
/// Transformation:
/// - Serializes with stable pretty JSON and delegates file writes to the shared
///   build writer.
fn write_json_file(
    path: &Path,
    value: &serde_json::Value,
    incremental: bool,
) -> Result<(), String> {
    let json = serde_json::to_string_pretty(value)
        .map_err(|err| format!("failed to serialize JSON: {err}"))?;
    write_build_file(path, format!("{json}\n").as_bytes(), incremental)
}

/// Formats compiler diagnostics for build-planner errors.
///
/// Inputs:
/// - Any compiler diagnostic collection with a debug representation.
///
/// Output:
/// - Compact diagnostic string for the mobile build planner.
///
/// Transformation:
/// - Keeps first-slice mobile planning independent from a public diagnostic
///   renderer while preserving useful failure details.
fn debug_diagnostics<T: std::fmt::Debug>(diagnostics: T) -> String {
    format!("{diagnostics:?}")
}

/// Packages browser output for native shell consumption.
///
/// Inputs:
/// - `state`: global CLI state that owns the build output root.
/// - `platform`: selected mobile platform path segment.
///
/// Output:
/// - `Ok(Some(relative_path))` when existing `_build/web` output was copied
///   into the mobile shell inputs directory.
/// - `Ok(None)` when no browser output exists yet.
/// - `Err(message)` when directory creation, cleanup, or copying fails.
///
/// Transformation:
/// - Treats browser output as an already-produced shell input and copies it
///   into `_build/mobile/<platform>/shell-inputs/web`.
fn package_mobile_web_output(state: &CliState, platform: &str) -> Result<Option<String>, String> {
    let web_source = state.out_dir.join("web");
    if !web_source.exists() {
        return Ok(None);
    }
    if !web_source.is_dir() {
        return Err(format!(
            "mobile build expected browser output directory at {}",
            web_source.display()
        ));
    }

    let relative_output = "shell-inputs/web";
    let destination = state
        .out_dir
        .join("mobile")
        .join(platform)
        .join(relative_output);
    if destination.exists() {
        fs::remove_dir_all(&destination)
            .map_err(|err| format!("failed to clean {}: {err}", destination.display()))?;
    }
    copy_directory(&web_source, &destination)?;
    Ok(Some(relative_output.to_string()))
}

/// Recursively copies a directory tree.
///
/// Inputs:
/// - `source`: existing source directory.
/// - `destination`: destination directory to create.
///
/// Output:
/// - `Ok(())` after all regular files and directories are copied.
/// - `Err(message)` when traversal or copying fails.
///
/// Transformation:
/// - Uses standard filesystem operations so mobile packaging can reuse the
///   browser package output without adding a second asset pipeline.
fn copy_directory(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination)
        .map_err(|err| format!("failed to create {}: {err}", destination.display()))?;
    for entry in
        fs::read_dir(source).map_err(|err| format!("failed to read {}: {err}", source.display()))?
    {
        let entry = entry.map_err(|err| format!("failed to read directory entry: {err}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|err| format!("failed to read {} file type: {err}", source_path.display()))?;
        if file_type.is_dir() {
            copy_directory(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            fs::copy(&source_path, &destination_path).map_err(|err| {
                format!(
                    "failed to copy {} to {}: {err}",
                    source_path.display(),
                    destination_path.display()
                )
            })?;
        }
    }
    Ok(())
}

/// Returns stable CLI and path labels for a mobile target.
///
/// Inputs:
/// - `target`: selected mobile platform.
///
/// Output:
/// - `(target_name, platform_path_segment)`.
///
/// Transformation:
/// - Centralizes target spelling for diagnostics and emitted manifest paths.
fn mobile_target_metadata(target: MobileBuildTarget) -> (&'static str, &'static str) {
    match target {
        MobileBuildTarget::Android => ("mobile.android", "android"),
        MobileBuildTarget::Ios => ("mobile.ios", "ios"),
    }
}

/// Returns the stable mobile planning diagnostic for one target.
///
/// Inputs:
/// - `target_name`: CLI target spelling from `mobile_target_metadata`.
///
/// Output:
/// - Human-readable diagnostic stored in the mobile plan.
///
/// Transformation:
/// - Makes successful mobile planning explicit about the current incomplete
///   native shell build boundary.
fn mobile_planning_diagnostic_message(target_name: &str) -> String {
    format!(
        "{target_name} planning metadata exists; full native shell emission remains experimental and incomplete"
    )
}
