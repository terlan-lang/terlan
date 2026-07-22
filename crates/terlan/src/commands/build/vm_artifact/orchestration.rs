use std::fs;
use std::path::PathBuf;

use crate::compiler::native_ir::NativeCodegenPolicy;
use crate::formal_pipeline::CheckedSyntaxModuleArtifacts;
use crate::terlan_typeck::CoreModule;
use crate::CliState;

use super::super::{BuildOneError, BuildTimings};
use super::compile::{compile_vm_module, CompiledVmModule};
use super::native_debug::NativeDebugInput;
use super::parallel_compile::compile_vm_modules;
use super::{native_image, native_reuse};

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
    let result = write_vm_artifact(
        &module.source_path,
        &module.source_text,
        &module.compiled,
        state,
        policy,
    );
    timings.mark("vm.aot-and-artifact");
    result
}

pub(in crate::commands::build) fn build_vm_application_artifacts(
    paths: &[PathBuf],
    state: &CliState,
    policy: NativeCodegenPolicy,
) -> Result<(), BuildOneError> {
    let mut timings = BuildTimings::new(state.timings);
    let modules = compile_vm_modules(paths, state)?;
    timings.mark("vm.application-compile");
    if state.no_emit {
        return Ok(());
    }
    let result = write_vm_application_artifacts(&modules, state, policy);
    timings.mark("vm.application-aot-and-artifact");
    result
}

/// Writes one descriptor-bearing native image when the module is AOT-capable.
fn write_vm_artifact(
    source_path: &str,
    source_text: &str,
    compiled: &CheckedSyntaxModuleArtifacts,
    state: &CliState,
    policy: NativeCodegenPolicy,
) -> Result<(), BuildOneError> {
    let vm_dir = state.out_dir.join("vm");
    fs::create_dir_all(&vm_dir).map_err(|err| {
        BuildOneError::Message(format!("cannot create VM artifact directory: {err}"))
    })?;
    let module_stem = module_file_stem(&compiled.core);
    let native_cache_root = state
        .cache_dir
        .clone()
        .unwrap_or_else(|| state.out_dir.join(".terlan"))
        .join("native-aot");
    let native_image = native_image::compile_native_application_image(
        &vm_dir,
        &native_cache_root,
        &module_stem,
        &[&compiled.core],
        &[NativeDebugInput {
            source_path,
            source_text,
            core: &compiled.core,
            syntax: &compiled.syntax_output,
        }],
        policy,
        state.incremental,
    )?;
    if let Some(native_image) = native_image.as_ref() {
        native_reuse::write_native_reuse_stamp(
            source_path,
            source_text,
            state,
            native_image,
            policy,
        )?;
    }
    Ok(())
}

fn write_vm_application_artifacts(
    modules: &[CompiledVmModule],
    state: &CliState,
    policy: NativeCodegenPolicy,
) -> Result<(), BuildOneError> {
    let vm_dir = state.out_dir.join("vm");
    fs::create_dir_all(&vm_dir).map_err(|error| {
        BuildOneError::Message(format!("cannot create VM artifact directory: {error}"))
    })?;
    let image_stem = modules
        .iter()
        .find(|module| module.compiled.core.module.ends_with(".Main"))
        .or_else(|| modules.first())
        .map(|module| module_file_stem(&module.compiled.core))
        .ok_or_else(|| BuildOneError::Message("VM application has no modules".to_string()))?;
    let native_cache_root = state
        .cache_dir
        .clone()
        .unwrap_or_else(|| state.out_dir.join(".terlan"))
        .join("native-aot");
    let cores = modules
        .iter()
        .map(|module| &module.compiled.core)
        .collect::<Vec<_>>();
    let debug_inputs = modules
        .iter()
        .map(|module| NativeDebugInput {
            source_path: &module.source_path,
            source_text: &module.source_text,
            core: &module.compiled.core,
            syntax: &module.compiled.syntax_output,
        })
        .collect::<Vec<_>>();
    native_image::compile_native_application_image(
        &vm_dir,
        &native_cache_root,
        &image_stem,
        &cores,
        &debug_inputs,
        policy,
        state.incremental,
    )?;
    Ok(())
}

/// Returns the filesystem stem for one VM application module.
fn module_file_stem(core: &CoreModule) -> String {
    core.module.replace('.', "_")
}
