use std::fs;
use std::path::Path;

use super::metadata::{
    BuildEntrypoint, BuildModuleArtifact, BuildPackageExecutable, BuildPackageMetadata,
};
use super::{write_build_file, BUILD_PACKAGE_METADATA_FILE};

/// Validates the manifest-backed package executable entrypoint.
///
/// Inputs:
/// - `modules`: build artifacts and CoreIR function summaries for every
///   emitted package module.
/// - `metadata`: manifest-derived package/build metadata.
///
/// Output:
/// - `Ok(BuildEntrypoint)` when `<package_root>.Main.main(): Unit` exists,
///   is public, and has arity zero.
/// - `Err(message)` when the entrypoint module, function, visibility, arity,
///   or return type violates the package executable contract.
///
/// Transformation:
/// - Checks the package-root entrypoint convention against backend-neutral
///   CoreIR summaries before any user-facing executable launcher is written.
pub(super) fn validate_build_entrypoint(
    modules: &[BuildModuleArtifact],
    metadata: &BuildPackageMetadata,
) -> Result<BuildEntrypoint, String> {
    let expected = &metadata
        .executable
        .as_ref()
        .expect("entrypoint validation requires executable metadata")
        .entrypoint;
    let module = modules
        .iter()
        .find(|artifact| artifact.debug_entry.module == expected.module)
        .ok_or_else(|| {
            format!(
                "terlc build package `{}` requires entrypoint `{}.{}(): Unit`; module `{}` was not built",
                metadata.package.name, expected.module, expected.function, expected.module
            )
        })?;

    let matching_arity = module
        .functions
        .iter()
        .find(|function| function.name == expected.function && function.arity == expected.arity);
    let Some(function) = matching_arity else {
        let arities = module
            .functions
            .iter()
            .filter(|function| function.name == expected.function)
            .map(|function| function.arity.to_string())
            .collect::<Vec<_>>();
        if arities.is_empty() {
            return Err(format!(
                "terlc build package `{}` requires entrypoint `{}.{}(): Unit`; function `{}` is missing from module `{}`",
                metadata.package.name,
                expected.module,
                expected.function,
                expected.function,
                expected.module
            ));
        }
        return Err(format!(
            "terlc build package `{}` requires entrypoint `{}.{}(): Unit`; found `{}` with arity {}",
            metadata.package.name,
            expected.module,
            expected.function,
            expected.function,
            arities.join(", ")
        ));
    };

    if !function.public {
        return Err(format!(
            "terlc build package `{}` entrypoint `{}.{}(): Unit` must be declared `pub`",
            metadata.package.name, expected.module, expected.function
        ));
    }

    if function.return_type != "Unit" {
        return Err(format!(
            "terlc build package `{}` entrypoint `{}.{}(): Unit` must return `Unit`, got `{}`",
            metadata.package.name, expected.module, expected.function, function.return_type
        ));
    }

    Ok(BuildEntrypoint {
        module: expected.module.clone(),
        function: expected.function.clone(),
        arity: expected.arity,
        erlang_module: crate::support::erlang_output_stem(&expected.module),
        erlang_function: expected.function.clone(),
    })
}

/// Writes the selected user-facing executable launcher.
///
/// Inputs:
/// - `out_dir`: build output root.
/// - `metadata`: manifest-derived package/build metadata.
/// - `incremental`: whether unchanged files may be left untouched.
///
/// Output:
/// - `Ok(())` after the executable launcher exists.
/// - `Err(message)` when directory creation, writing, or permission updates
///   fail.
///
/// Transformation:
/// - Materializes the current `beam-thin` executable contract as a single
///   launcher file under `bin/` that points Erlang at the generated `ebin`
///   directory. It does not assemble an OTP release or bundle ERTS.
pub(super) fn write_build_executable_launcher(
    out_dir: &Path,
    executable: &BuildPackageExecutable,
    entrypoint: &BuildEntrypoint,
    incremental: bool,
) -> Result<(), String> {
    match executable.mode.as_str() {
        "beam-thin" => write_beam_thin_launcher(out_dir, &executable.path, entrypoint, incremental),
        other => Err(format!(
            "cannot write unsupported executable artifact mode `{other}`"
        )),
    }
}

/// Writes package/build metadata into the build output directory.
///
/// Inputs:
/// - `out_dir`: build output root.
/// - `metadata`: manifest-derived package/build metadata.
/// - `incremental`: whether unchanged files may be left untouched.
///
/// Output:
/// - `Ok(())` after package metadata exists.
/// - `Err(message)` when serialization or writing fails.
///
/// Transformation:
/// - Serializes deterministic package metadata as stable JSON at
///   `terlan-package-build.json`.
pub(super) fn write_build_package_metadata(
    out_dir: &Path,
    metadata: BuildPackageMetadata,
    incremental: bool,
) -> Result<(), String> {
    let json = serde_json::to_string_pretty(&metadata)
        .map_err(|err| format!("failed to serialize build package metadata: {err}"))?;
    write_build_file(
        &out_dir.join(BUILD_PACKAGE_METADATA_FILE),
        format!("{json}\n").as_bytes(),
        incremental,
    )
}

/// Writes a thin BEAM launcher script.
///
/// Inputs:
/// - `out_dir`: build output root.
/// - `relative_path`: metadata-relative executable path, such as `bin/demo`.
/// - `incremental`: whether unchanged files may be left untouched.
///
/// Output:
/// - `Ok(())` after the launcher exists and is executable on Unix.
/// - `Err(message)` when directory creation, writing, or permission updates
///   fail.
///
/// Transformation:
/// - Emits a portable POSIX shell launcher that resolves its own build root and
///   starts `erl` with the generated `ebin` directory on the BEAM code path.
fn write_beam_thin_launcher(
    out_dir: &Path,
    relative_path: &str,
    entrypoint: &BuildEntrypoint,
    incremental: bool,
) -> Result<(), String> {
    let executable_path = out_dir.join(relative_path);
    let parent = executable_path.parent().ok_or_else(|| {
        format!(
            "cannot resolve parent directory for executable artifact {}",
            executable_path.display()
        )
    })?;
    fs::create_dir_all(parent)
        .map_err(|err| format!("cannot create build executable directory: {err}"))?;

    let script = format!(
        "#!/usr/bin/env sh\nset -eu\nSCRIPT_DIR=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\nROOT_DIR=$(CDPATH= cd -- \"$SCRIPT_DIR/..\" && pwd)\nexec erl -noshell -pa \"$ROOT_DIR/ebin\" -eval \"case catch {module}:{function}() of {{'EXIT', Reason}} -> io:format(standard_error, \\\"terlan entrypoint {source_module}.{source_function}/{arity} failed: ~p~n\\\", [Reason]), halt(1); _ -> halt(0) end.\" \"$@\"\n",
        module = entrypoint.erlang_module,
        function = entrypoint.erlang_function,
        source_module = entrypoint.module,
        source_function = entrypoint.function,
        arity = entrypoint.arity,
    );
    write_build_file(&executable_path, script.as_bytes(), incremental)?;
    mark_build_file_executable(&executable_path)
}

/// Marks a generated build file executable when the platform supports Unix
/// mode bits.
///
/// Inputs:
/// - `path`: generated build file path.
///
/// Output:
/// - `Ok(())` after permissions are updated or when the platform has no Unix
///   mode bits.
/// - `Err(message)` when permission reads or writes fail.
///
/// Transformation:
/// - Adds user/group/other execute bits to the generated launcher on Unix and
///   leaves non-Unix platforms to their native execution policy.
#[cfg(unix)]
fn mark_build_file_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(path).map_err(|err| {
        format!(
            "cannot read executable permissions {}: {err}",
            path.display()
        )
    })?;
    let mut permissions = metadata.permissions();
    permissions.set_mode(permissions.mode() | 0o111);
    fs::set_permissions(path, permissions)
        .map_err(|err| format!("cannot mark executable {}: {err}", path.display()))
}

/// Marks a generated build file executable when the platform supports Unix
/// mode bits.
///
/// Inputs:
/// - `path`: generated build file path.
///
/// Output:
/// - Always `Ok(())` on non-Unix platforms.
///
/// Transformation:
/// - Keeps the call site cross-platform while non-Unix executable semantics
///   remain owned by downstream target packaging.
#[cfg(not(unix))]
fn mark_build_file_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}
