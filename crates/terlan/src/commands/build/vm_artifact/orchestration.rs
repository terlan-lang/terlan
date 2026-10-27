use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use crate::compiler::native_ir::NativeCodegenPolicy;
use crate::terlan_typeck::CoreModule;
use crate::CliState;

use super::super::{BuildOneError, BuildTimings};
use super::compile::{compile_vm_module, CompiledVmModule};
use super::parallel_compile::compile_vm_modules;
use super::{native_image, native_reuse, std_source};

#[cfg(test)]
#[path = "orchestration_test.rs"]
mod tests;

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) struct CompiledServeApplication {
    #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
    pub(crate) core: CoreModule,
    pub(crate) interfaces: Vec<crate::terlan_hir::ModuleInterface>,
    pub(crate) image: native_image::CompiledServeNativeImage,
}

/// Builds one Terlan VM artifact from one source file.
/// Inputs:
/// - `path`: Terlan source path.
/// - `state`: global CLI state carrying output and diagnostic settings.
///
/// Output:
/// - `Ok(())` after native-image publication or compiler-interface-only output.
///
/// Transformation:
/// - Runs the formal compiler pipeline with the portable CoreIR v0 profile and
///   emits a compiler-owned native TVM image when the checked module has a
///   supported AOT region. Managed-only libraries retain their `.typi`
///   compiler interface without producing serialized runtime code.
pub(in crate::commands::build) fn build_one_vm_artifact(
    path: &str,
    state: &CliState,
    policy: NativeCodegenPolicy,
) -> Result<(), BuildOneError> {
    let mut timings = BuildTimings::new(state.timings);
    if native_reuse::reuse_dependency_free_native_image(path, state, policy)? {
        timings.mark("vm.native-cache-reuse");
        return Ok(());
    }
    let module = compile_vm_module(path, state)?;
    timings.mark("vm.compile");
    if state.no_emit {
        return Ok(());
    }
    let imported = std_source::compile_imported_std_source_modules(
        &[&module.compiled.core],
        PathBuf::from(path).as_path(),
        state,
    )?;
    let result = if imported.is_empty() {
        write_vm_artifact(module, state, policy)
    } else {
        let mut modules = Vec::with_capacity(imported.len() + 1);
        modules.push(module);
        modules.extend(imported);
        write_vm_application_artifacts(modules, state, policy, None)
    };
    timings.mark("vm.aot-and-artifact");
    result
}

pub(in crate::commands::build) fn build_vm_application_artifacts(
    paths: &[PathBuf],
    state: &CliState,
    policy: NativeCodegenPolicy,
) -> Result<(), BuildOneError> {
    build_vm_application_artifacts_with_optional_entry(paths, state, policy, None)
}

/// Compiles one route module together with its complete packaged application
/// closure into a compiler-free serve image.
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) fn compile_serve_application(
    web_root: &std::path::Path,
    source_path: &std::path::Path,
    expected_module: &str,
) -> Result<CompiledServeApplication, String> {
    let source_root = application_source_root(source_path, expected_module)?;
    if !super::super::project_manifest_path(&source_root).is_file()
        && !web_root
            .ancestors()
            .any(|candidate| source_root.starts_with(candidate) && candidate.parent().is_some())
    {
        return Err(format!(
            "serve source {} and web root {} do not share an application root",
            source_path.display(),
            web_root.display()
        ));
    }
    compile_application_from_root(web_root, &source_root, expected_module)
}

/// Compiles a source module for tooling without imposing serving output layout.
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) fn compile_source_application(
    workspace: &std::path::Path,
    source_path: &std::path::Path,
    expected_module: &str,
) -> Result<CompiledServeApplication, String> {
    let source_root = application_source_root(source_path, expected_module)?;
    compile_application_from_root(workspace, &source_root, expected_module)
}

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
fn compile_application_from_root(
    web_root: &std::path::Path,
    source_root: &std::path::Path,
    expected_module: &str,
) -> Result<CompiledServeApplication, String> {
    let source_roots = super::super::terlan_vm_source_roots_for_directory(source_root)?;
    let state = CliState {
        out_dir: web_root.join(".terlan").join("serve-build"),
        cache_dir: Some(web_root.join(".terlan").join("serve-compiler")),
        incremental: true,
        native_policy: crate::validation::native_policy::NativePolicy::NativeBoundaryOptional,
        ..CliState::default()
    };
    let (paths, state) = super::super::prepare_source_roots_build(&source_roots, &state)
        .map_err(|status| format!("serve application preparation failed with {status:?}"))?;
    let mut modules =
        super::application_closure::compile_application_closure(&paths, expected_module, &state)
            .map_err(|error| match error {
                BuildOneError::Message(message) => message,
                BuildOneError::Exit(status) => {
                    format!("serve application compilation failed with {status:?}")
                }
            })?;
    if let Some(active_path) = modules
        .first()
        .map(|module| PathBuf::from(&module.source_path))
    {
        let roots = modules
            .iter()
            .map(|module| &module.compiled.core)
            .collect::<Vec<_>>();
        let imported =
            std_source::compile_imported_std_source_modules(&roots, active_path.as_path(), &state)
                .map_err(|error| match error {
                    BuildOneError::Message(message) => message,
                    BuildOneError::Exit(status) => {
                        format!("serve stdlib closure compilation failed with {status:?}")
                    }
                })?;
        let mut present = modules
            .iter()
            .map(|module| module.compiled.core.module.clone())
            .collect::<BTreeSet<_>>();
        modules.extend(
            imported
                .into_iter()
                .filter(|module| present.insert(module.compiled.core.module.clone())),
        );
    }
    let route_index = modules
        .iter()
        .position(|module| module.compiled.core.module == expected_module)
        .ok_or_else(|| format!("serve application is missing route module `{expected_module}`"))?;
    let core = modules[route_index].compiled.core.clone();
    let debug_source_paths = modules
        .iter()
        .map(|module| stable_serve_debug_source_path(source_root, &module.source_path))
        .collect::<Vec<_>>();
    let module_stem = expected_module.replace('.', "_");
    let roots = core
        .functions
        .iter()
        .filter(|function| function.public)
        .map(|function| (core.module.clone(), function.name.clone(), function.arity))
        .collect::<Vec<_>>();
    let (cores, debug_inputs): (Vec<_>, Vec<_>) = modules
        .into_iter()
        .zip(debug_source_paths)
        .map(|(module, source_path)| {
            (
                module.compiled.core,
                module.native_debug.with_source_path(source_path),
            )
        })
        .unzip();
    let core_refs = cores.iter().collect::<Vec<_>>();
    let image = native_image::compile_serve_native_application_image_with_metadata(
        web_root,
        &module_stem,
        &core_refs,
        &roots,
        &debug_inputs,
    )?
    .ok_or_else(|| {
        format!(
            "error[serve.aot.image_required]: application route module `{expected_module}` did not produce a native image"
        )
    })?;
    let interfaces = cores.iter().map(|core| core.interface.clone()).collect();
    Ok(CompiledServeApplication {
        #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
        core,
        image,
        interfaces,
    })
}

/// Removes the build output root from source identities embedded in packaged
/// serve images. Release bundles may be assembled in arbitrary directories;
/// those directories are not compiler inputs and must not change image bytes.
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
fn stable_serve_debug_source_path(source_root: &std::path::Path, source_path: &str) -> String {
    std::path::Path::new(source_path)
        .strip_prefix(source_root)
        .ok()
        .map(|relative| {
            relative
                .components()
                .filter_map(|component| match component {
                    std::path::Component::Normal(part) => Some(part.to_string_lossy()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("/")
        })
        .filter(|relative| !relative.is_empty())
        .unwrap_or_else(|| source_path.to_string())
}

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
fn application_source_root(
    source_path: &std::path::Path,
    expected_module: &str,
) -> Result<PathBuf, String> {
    if let Some(project_root) = source_path
        .ancestors()
        .find(|candidate| super::super::project_manifest_path(candidate).is_file())
    {
        return Ok(project_root.to_path_buf());
    }

    let module_parts = expected_module.split('.').collect::<Vec<_>>();
    let mut source_root = source_path.parent().ok_or_else(|| {
        format!(
            "serve source path has no parent directory: {}",
            source_path.display()
        )
    })?;
    for _ in 1..module_parts.len() {
        source_root = source_root.parent().ok_or_else(|| {
            format!(
                "serve source {} is too shallow for module `{expected_module}`",
                source_path.display()
            )
        })?;
    }
    let mut expected_relative = PathBuf::new();
    for part in module_parts {
        expected_relative.push(part);
    }
    expected_relative.set_extension(
        source_path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("terl"),
    );
    if source_path.strip_prefix(source_root).ok() != Some(expected_relative.as_path()) {
        return Err(format!(
            "serve source {} does not match module `{expected_module}`",
            source_path.display()
        ));
    }
    Ok(source_root.to_path_buf())
}

/// Builds a native application rooted at one exact compiler module identity.
///
/// Script builds use this entry-aware form so an owning project module ending
/// in `.Main` can never displace the selected `.terls` synthetic `main/0`.
pub(in crate::commands::build) fn build_vm_application_artifacts_with_entry(
    paths: &[PathBuf],
    state: &CliState,
    policy: NativeCodegenPolicy,
    entry_module: &str,
) -> Result<(), BuildOneError> {
    build_vm_application_artifacts_with_optional_entry(paths, state, policy, Some(entry_module))
}

fn build_vm_application_artifacts_with_optional_entry(
    paths: &[PathBuf],
    state: &CliState,
    policy: NativeCodegenPolicy,
    entry_module: Option<&str>,
) -> Result<(), BuildOneError> {
    if std::env::var_os("TERLAN_BUILD_LOW_MEMORY").is_some() {
        return super::low_memory::build_vm_application_artifacts(
            paths,
            state,
            policy,
            entry_module,
        );
    }
    let mut timings = BuildTimings::new(state.timings);
    let mut modules = compile_vm_modules(paths, state)?;
    timings.mark("vm.application-compile");
    if state.no_emit {
        return Ok(());
    }
    if let Some(active_path) = modules
        .first()
        .map(|module| PathBuf::from(&module.source_path))
    {
        let roots = modules
            .iter()
            .map(|module| &module.compiled.core)
            .collect::<Vec<_>>();
        let imported =
            std_source::compile_imported_std_source_modules(&roots, active_path.as_path(), state)?;
        let mut present = modules
            .iter()
            .map(|module| module.compiled.core.module.clone())
            .collect::<BTreeSet<_>>();
        modules.extend(
            imported
                .into_iter()
                .filter(|module| present.insert(module.compiled.core.module.clone())),
        );
    }
    timings.mark("vm.application-std-closure");
    let result = write_vm_application_artifacts(modules, state, policy, entry_module);
    timings.mark("vm.application-aot-and-artifact");
    result
}

/// Writes one descriptor-bearing native image when the module is AOT-capable.
fn write_vm_artifact(
    module: CompiledVmModule,
    state: &CliState,
    policy: NativeCodegenPolicy,
) -> Result<(), BuildOneError> {
    let vm_dir = state.out_dir.join("vm");
    fs::create_dir_all(&vm_dir).map_err(|err| {
        BuildOneError::Message(format!("cannot create VM artifact directory: {err}"))
    })?;
    let module_stem = module_file_stem(&module.compiled.core);
    let native_cache_root = state
        .cache_dir
        .clone()
        .unwrap_or_else(|| state.out_dir.join(".terlan"))
        .join("native-aot");
    let CompiledVmModule {
        source_path,
        compiled,
        native_debug,
        ..
    } = module;
    let native_image = native_image::compile_native_application_image(
        &vm_dir,
        &native_cache_root,
        &module_stem,
        &[&compiled.core],
        std::slice::from_ref(&native_debug),
        policy,
        state.incremental,
    )?;
    if let Some(native_image) = native_image.as_ref() {
        native_reuse::write_native_reuse_stamp(
            &source_path,
            native_debug.source_text(),
            state,
            native_image,
            policy,
        )?;
    }
    Ok(())
}

fn write_vm_application_artifacts(
    modules: Vec<CompiledVmModule>,
    state: &CliState,
    policy: NativeCodegenPolicy,
    entry_module: Option<&str>,
) -> Result<(), BuildOneError> {
    let vm_dir = state.out_dir.join("vm");
    fs::create_dir_all(&vm_dir).map_err(|error| {
        BuildOneError::Message(format!("cannot create VM artifact directory: {error}"))
    })?;
    let entry = if let Some(entry_module) = entry_module {
        modules
            .iter()
            .find(|module| module.compiled.core.module == entry_module)
            .ok_or_else(|| {
                BuildOneError::Message(format!(
                    "VM application is missing selected entry module `{entry_module}`"
                ))
            })?
    } else {
        modules
            .iter()
            .find(|module| module.compiled.core.module.ends_with(".Main"))
            .or_else(|| modules.first())
            .ok_or_else(|| BuildOneError::Message("VM application has no modules".to_string()))?
    };
    let image_stem = module_file_stem(&entry.compiled.core);
    let native_cache_root = state
        .cache_dir
        .clone()
        .unwrap_or_else(|| state.out_dir.join(".terlan"))
        .join("native-aot");
    let roots = if entry
        .compiled
        .core
        .functions
        .iter()
        .any(|function| function.name == "main" && function.arity == 0)
    {
        vec![(entry.compiled.core.module.clone(), "main".to_string(), 0)]
    } else {
        entry
            .compiled
            .core
            .exports
            .iter()
            .filter_map(|export| match export.kind {
                crate::terlan_typeck::CoreExportKind::Function { arity } => Some((
                    entry.compiled.core.module.clone(),
                    export.name.clone(),
                    arity,
                )),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let (cores, debug_inputs): (Vec<_>, Vec<_>) = modules
        .into_iter()
        .map(|module| (module.compiled.core, module.native_debug))
        .unzip();
    native_image::compile_owned_rooted_native_application_image(
        &vm_dir,
        &native_cache_root,
        &image_stem,
        cores,
        native_image::RootedNativeApplicationInput {
            roots: &roots,
            debug_inputs: &debug_inputs,
            policy,
            incremental: state.incremental,
        },
    )?;
    Ok(())
}

/// Returns the filesystem stem for one VM application module.
fn module_file_stem(core: &CoreModule) -> String {
    core.module.replace('.', "_")
}
