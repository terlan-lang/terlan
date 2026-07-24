
/// Resolves source roots for a Terlan VM directory build.
///
/// Inputs:
/// - `dir`: build directory.
///
/// Output:
/// - Source root build units with optional package-root validation metadata.
///
/// Transformation:
/// - Uses project manifest metadata when available and falls back to a plain
///   single-root build otherwise.
fn terlan_vm_source_roots_for_directory(
    dir: &Path,
) -> Result<Vec<source_roots::SourceRootBuildUnit>, String> {
    let manifest_path = project_manifest_path(dir);
    if manifest_path.is_file() {
        let manifest = project_manifest::read_project_manifest(&manifest_path)?;
        if let Some(message) = reserved_project_artifact_build_error(&manifest) {
            return Err(message);
        }
        let roots = resolve_project_build_roots(dir, &manifest)?;
        return Ok(roots
            .source_roots
            .into_iter()
            .map(|root| source_roots::SourceRootBuildUnit {
                path: root.path,
                package_path: Some(root.package_path),
            })
            .collect());
    }

    Ok(vec![source_roots::SourceRootBuildUnit {
        path: dir.to_path_buf(),
        package_path: None,
    }])
}

/// Emits VM artifacts for all files in source roots.
///
/// Inputs:
/// - `source_roots`: resolved source roots with optional package-root checks.
/// - `state`: global CLI state used by VM artifact emission.
///
/// Output:
/// - CLI exit code representing source-root VM artifact emission.
///
/// Transformation:
/// - Recursively discovers `.terl` files, validates manifest package-root
///   layout when applicable, and emits each file through the same VM artifact
///   path used by single-file builds.
fn run_terlan_vm_source_roots_build(
    source_roots: &[source_roots::SourceRootBuildUnit],
    state: &CliState,
    policy: crate::compiler::native_ir::NativeCodegenPolicy,
) -> ExitCode {
    let (files, directory_state) = match prepare_source_roots_build(source_roots, state) {
        Ok(prepared) => prepared,
        Err(status) => return status,
    };
    match vm_artifact::build_vm_application_artifacts(&files, &directory_state, policy) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => error.into_exit_code(),
    }
}

/// Emits Wasm core artifacts for all files in source roots.
///
/// Inputs:
/// - `source_roots`: resolved source roots with optional package-root checks.
/// - `state`: global CLI state used by Wasm artifact emission.
///
/// Output:
/// - CLI exit code representing source-root Wasm artifact emission.
///
/// Transformation:
/// - Reuses the same source discovery, package-root validation, and interface
///   preparation as VM artifact builds, but writes `.wasm` and `.wasm.json`
///   outputs through the Wasm artifact path.
fn run_wasm_core_source_roots_build(
    source_roots: &[source_roots::SourceRootBuildUnit],
    state: &CliState,
) -> ExitCode {
    run_source_roots_build(
        source_roots,
        state,
        wasm_artifact::build_one_wasm_core_artifact,
    )
}

/// Emits artifacts for all files in source roots with a selected builder.
///
/// Inputs:
/// - `source_roots`: resolved source roots with optional package-root checks.
/// - `state`: global CLI state used by artifact emission.
/// - `build_one`: artifact builder for one source file.
///
/// Output:
/// - CLI exit code representing source-root artifact emission.
///
/// Transformation:
/// - Centralizes directory source discovery, package-root validation, interface
///   preparation, and cache-dir setup so backend-specific artifact paths do not
///   duplicate compiler orchestration.
fn run_source_roots_build(
    source_roots: &[source_roots::SourceRootBuildUnit],
    state: &CliState,
    build_one: BuildOneArtifactFn,
) -> ExitCode {
    let (files, directory_state) = match prepare_source_roots_build(source_roots, state) {
        Ok(prepared) => prepared,
        Err(status) => return status,
    };
    for file in files {
        let file = file.to_string_lossy().to_string();
        if let Err(err) = build_one(&file, &directory_state) {
            return err.into_exit_code();
        }
    }
    ExitCode::SUCCESS
}

fn prepare_source_roots_build(
    source_roots: &[source_roots::SourceRootBuildUnit],
    state: &CliState,
) -> Result<(Vec<PathBuf>, CliState), ExitCode> {
    let mut files = Vec::new();
    for root in source_roots {
        let root_files = match crate::formal_pipeline::terlan_sources_in_dir(&root.path) {
            Ok(root_files) => root_files,
            Err(message) => {
                eprintln!("{message}");
                return Err(ExitCode::from(1));
            }
        };
        if root_files.is_empty() {
            source_roots::report_empty_source_root(&root.path);
            return Err(ExitCode::from(1));
        }
        if let Some(package_path) = root.package_path.as_deref() {
            for file in &root_files {
                if let Err(message) =
                    validate_project_source_package_root(&root.path, file, package_path)
                {
                    eprintln!("{message}");
                    return Err(ExitCode::from(1));
                }
            }
        }
        files.extend(root_files);
    }

    let mut directory_state = state.clone();
    if directory_state.cache_dir.is_none() {
        directory_state.cache_dir = Some(state.out_dir.join(".terlan"));
    }
    for root in source_roots {
        if let Err(message) =
            source_roots::prepare_source_root_interfaces(&root.path, &directory_state)
        {
            eprintln!("{message}");
            return Err(ExitCode::from(1));
        }
    }
    Ok((files, directory_state))
}

/// Returns a stable diagnostic for project artifact families not owned by the
/// compiler-owned VM build path.
///
/// Inputs:
/// - Parsed project manifest.
///
/// Output:
/// - `Some(String)` when the manifest selected a reserved Wasm/WASI artifact.
/// - `None` for current compiler-owned artifact modes.
///
/// Transformation:
/// - Keeps reserved future target artifacts independent from build execution
///   until replacement dispatch gates are implemented.
/// - terlc build artifact `beam-thin` was removed from the public build path;
///   manifest parsing may keep the spelling only for stable rejection.
fn reserved_project_artifact_build_error(
    manifest: &project_manifest::ProjectManifest,
) -> Option<String> {
    match manifest.artifact {
        project_manifest::ProjectArtifactKind::TerlanVm
        | project_manifest::ProjectArtifactKind::Library
        | project_manifest::ProjectArtifactKind::WasmCore => None,
        project_manifest::ProjectArtifactKind::WasmBrowser
        | project_manifest::ProjectArtifactKind::WasmComponent => Some(format!(
            "terlc build artifact `{}` is reserved for the Wasm target family but is not implemented yet",
            manifest.artifact.as_str()
        )),
        project_manifest::ProjectArtifactKind::WasiCli
        | project_manifest::ProjectArtifactKind::WasiHttp
        | project_manifest::ProjectArtifactKind::WasiWorker => Some(format!(
            "terlc build artifact `{}` is reserved for the WASI target family but is not implemented yet",
            manifest.artifact.as_str()
        )),
    }
}

/// Computes the canonical project manifest path for a build directory.
///
/// Inputs:
/// - `dir`: directory passed to `terlc build`.
///
/// Output:
/// - Path to the package/project manifest candidate inside `dir`.
///
/// Transformation:
/// - Appends the canonical manifest filename without reading or parsing it so
///   directory builds can reject project manifests before silently treating
///   them as plain source roots.
fn project_manifest_path(dir: &Path) -> PathBuf {
    dir.join(TERLAN_PROJECT_MANIFEST_FILE)
}

/// Converts a filesystem path into the portable manifest representation.
///
/// Inputs:
/// - `path`: generated artifact path.
///
/// Output:
/// - Lossy UTF-8 string with `/` separators suitable for JSON manifests.
///
/// Transformation:
/// - Converts the host-native separator to `/` so package metadata remains
///   consumable on every supported host.
fn path_to_manifest_string(path: &Path) -> String {
    path.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

/// Writes one build output file.
///
/// Inputs:
/// - `path`: output path to write.
/// - `bytes`: file contents.
/// - `incremental`: whether unchanged files may be left untouched.
///
/// Output:
/// - `Ok(())` after the file exists with the requested contents.
/// - `Err(message)` when the write fails.
///
/// Transformation:
/// - Delegates to the shared incremental-write helper and wraps errors with the
///   build output path.
fn write_build_file(path: &Path, bytes: &[u8], incremental: bool) -> Result<(), String> {
    crate::support::write_if_changed_or_forced(path, bytes, incremental)
        .map_err(|err| format!("failed to write {}: {err}", path.display()))
}
