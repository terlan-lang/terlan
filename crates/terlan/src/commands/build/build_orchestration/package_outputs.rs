use super::super::metadata::ProjectBuildRoots;
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

/// Writes one executable and build metadata for a native application package.
pub(in crate::commands::build) fn write_terlan_vm_executable_package_outputs(
    project_dir: &Path,
    manifest: &project_manifest::ProjectManifest,
    roots: &ProjectBuildRoots,
    vm_service: bool,
    artifact_state: &CliState,
    state: &CliState,
) -> Result<(), String> {
    let executable_name = package_executable_name(&manifest.package.name);
    let executable_relative_path = PathBuf::from("bin").join(&executable_name);
    let executable_path = state.out_dir.join(&executable_relative_path);
    let artifact_relative_path =
        PathBuf::from("vm").join(format!("{}.tvm", executable_vm_artifact_stem(manifest)));
    let artifact_path = artifact_state.out_dir.join(&artifact_relative_path);
    let legacy = super::super::single_executable::LegacyOutputs::capture(
        &state.out_dir,
        &manifest.package.name,
        &artifact_relative_path,
    );
    if !vm_service && !artifact_path.is_file() {
        return Err(format!(
            "terlc build executable package `{}` requires entry artifact `{}`; define `{}.Main.main/0` or set [build] artifact = \"library\"",
            manifest.package.name,
            artifact_path.display(),
            source_package_path(&manifest.package).join(".")
        ));
    }
    let entry_module = format!("{}.Main", source_package_path(&manifest.package).join("."));
    if !vm_service && !vm_image_has_main_entrypoint(&artifact_path, &entry_module)? {
        return Err(format!(
            "terlc build executable package `{}` requires public `main/0` in `{}`; define `{}.Main.main/0` or set [build] artifact = \"library\"",
            manifest.package.name,
            artifact_path.display(),
            source_package_path(&manifest.package).join(".")
        ));
    }
    let entry = if vm_service {
        crate::runtime::executable::ApplicationEntry::Service {
            web_root: "web".into(),
        }
    } else {
        crate::runtime::executable::ApplicationEntry::Cli {
            image: path_to_manifest_string(&artifact_relative_path),
            function: format!("{entry_module}.main"),
        }
    };
    super::super::single_executable::write(
        &executable_path,
        &artifact_state.out_dir,
        entry,
        &roots.native_artifact_environment,
        state.incremental,
    )?;

    let mut metadata = build_package_metadata_with_artifacts(
        project_dir,
        manifest,
        &roots.native_rust_dependencies,
        &roots.native_artifact_environment,
        roots.accelerator_closure.as_ref(),
    );
    metadata.executable = Some(BuildPackageExecutable {
        path: path_to_manifest_string(&executable_relative_path),
        embedded: true,
        image: None,
        runtime: path_to_manifest_string(&executable_relative_path),
        native_worker: path_to_manifest_string(&executable_relative_path),
        service_runtime: vm_service.then(|| path_to_manifest_string(&executable_relative_path)),
        web_root: None,
    });
    legacy.remove(&state.out_dir, &executable_relative_path)?;
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
