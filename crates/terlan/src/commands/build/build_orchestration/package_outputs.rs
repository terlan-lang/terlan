use super::*;

/// Discovers route-owning source modules in the root executable package.
pub(super) fn root_vm_service_route_sources(
    project_dir: &Path,
    manifest: &project_manifest::ProjectManifest,
) -> Result<Vec<js_browser::WebRouteSourceArtifact>, String> {
    let mut routes = Vec::new();
    for source_root in &manifest.source_roots {
        let root = project_dir.join(source_root);
        for source in crate::formal_pipeline::terlan_sources_in_dir(&root)? {
            let Some(mut route) =
                js_source_classification::web_route_source_artifact_from_file(&source)?
            else {
                continue;
            };
            let relative = source.strip_prefix(project_dir).map_err(|_| {
                format!(
                    "VM service route source {} escapes project root {}",
                    source.display(),
                    project_dir.display()
                )
            })?;
            if relative.components().any(|component| {
                matches!(
                    component,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            }) {
                return Err(format!(
                    "VM service route source path is unsafe: {}",
                    relative.display()
                ));
            }
            route.manifest_path = Some(path_to_manifest_string(relative));
            routes.push(route);
        }
    }
    routes.sort_by(|left, right| left.module.cmp(&right.module));
    Ok(routes)
}

/// Writes launcher and metadata outputs for a VM executable package.
pub(in crate::commands::build) fn write_terlan_vm_executable_package_outputs(
    project_dir: &Path,
    manifest: &project_manifest::ProjectManifest,
    native_rust_dependencies: &[ProjectNativeRustDependency],
    native_artifact_environment: &[(String, PathBuf)],
    accelerator_closure: Option<&crate::compiler::accelerator::AcceleratorDependencyClosure>,
    vm_service: bool,
    state: &CliState,
) -> Result<(), String> {
    let executable_name = package_executable_name(&manifest.package.name);
    let executable_relative_path = PathBuf::from("bin").join(&executable_name);
    let executable_path = state.out_dir.join(&executable_relative_path);
    let artifact_relative_path =
        PathBuf::from("vm").join(format!("{}.tvm", executable_vm_artifact_stem(manifest)));
    let artifact_path = state.out_dir.join(&artifact_relative_path);
    if !artifact_path.is_file() {
        return Err(format!(
            "terlc build executable package `{}` requires entry artifact `{}`; define `{}.Main.main/0` or set [build] artifact = \"library\"",
            manifest.package.name,
            artifact_path.display(),
            source_package_path(&manifest.package).join(".")
        ));
    }
    let entry_module = format!("{}.Main", source_package_path(&manifest.package).join("."));
    if !vm_image_has_main_entrypoint(&artifact_path, &entry_module)? {
        return Err(format!(
            "terlc build executable package `{}` requires public `main/0` in `{}`; define `{}.Main.main/0` or set [build] artifact = \"library\"",
            manifest.package.name,
            artifact_path.display(),
            source_package_path(&manifest.package).join(".")
        ));
    }
    let bundled_runner_path = state.out_dir.join("bin").join(terlan_vm_runner_name());
    copy_bundled_terlan_vm_runner(&bundled_runner_path)?;
    let bundled_worker_path = state.out_dir.join("bin").join(terlan_native_worker_name());
    copy_bundled_terlan_native_worker(&bundled_worker_path)?;
    let service_runtime_relative =
        vm_service.then(|| PathBuf::from("bin").join(terlan_serve_runtime_name()));
    if let Some(relative) = &service_runtime_relative {
        copy_bundled_terlan_serve_runtime(&state.out_dir.join(relative))?;
        write_vm_service_launcher(&executable_path, state.incremental)?;
    } else {
        write_vm_launcher(&executable_path, &artifact_relative_path, state.incremental)?;
    }

    let mut metadata = build_package_metadata_with_artifacts(
        project_dir,
        manifest,
        native_rust_dependencies,
        native_artifact_environment,
        accelerator_closure,
    );
    metadata.executable = Some(BuildPackageExecutable {
        path: path_to_manifest_string(&executable_relative_path),
        image: path_to_manifest_string(&artifact_relative_path),
        runtime: path_to_manifest_string(&PathBuf::from("bin").join(terlan_vm_runner_name())),
        native_worker: path_to_manifest_string(
            &PathBuf::from("bin").join(terlan_native_worker_name()),
        ),
        service_runtime: service_runtime_relative
            .as_ref()
            .map(|path| path_to_manifest_string(path)),
        web_root: vm_service.then(|| "web".to_string()),
    });
    write_package_metadata(&metadata, state)
}

/// Writes normalized package metadata for executable and library artifacts.
pub(super) fn write_package_metadata(
    metadata: &BuildPackageMetadata,
    state: &CliState,
) -> Result<(), String> {
    let json = serde_json::to_string_pretty(metadata)
        .map_err(|err| format!("failed to serialize package build metadata: {err}"))?;
    write_build_file(
        &state.out_dir.join(BUILD_PACKAGE_METADATA_FILE),
        format!("{json}\n").as_bytes(),
        state.incremental,
    )
}
