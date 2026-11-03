//! Formal-pipeline entry for native VM artifact compilation.

use crate::formal_pipeline::CheckedSyntaxModuleArtifacts;
use crate::terlan_typeck::CoreModule;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::{TargetProfile, TargetProfileCheckOptions};
use crate::CliState;

use super::super::BuildOneError;
use super::checked_cache;
use super::native_debug::NativeDebugInput;

#[cfg(all(target_os = "linux", target_env = "gnu"))]
unsafe extern "C" {
    fn malloc_trim(pad: usize) -> i32;
}

/// Checked compiler output retained by VM application orchestration.
pub(super) struct CompiledVmModule {
    pub(super) source_path: String,
    pub(super) compiled: CompiledVmArtifacts,
    pub(super) native_debug: NativeDebugInput,
    /// Whether all checked implementation data came from the verified cache.
    #[cfg(test)]
    pub(super) checked_cache_reused: bool,
}

/// Backend state retained after frontend checking and cache publication.
pub(super) struct CompiledVmArtifacts {
    pub(super) core: CoreModule,
}

pub(super) fn compile_vm_module(
    path: &str,
    state: &CliState,
) -> Result<CompiledVmModule, BuildOneError> {
    let source_text = crate::support::read_file(path)
        .map_err(|error| BuildOneError::Message(error.to_string()))?;
    let target_profile_options = TargetProfileCheckOptions {
        allow_asset_imports: false,
        allow_rust_backed_std_modules: state.native_policy != NativePolicy::Pure,
    };
    let (compiled, checked_cache_reused) = load_or_compile_checked_vm_artifacts(
        path,
        &source_text,
        state,
        target_profile_options,
        None,
    )?;
    Ok(compact_compiled_module(
        path.to_string(),
        source_text,
        compiled,
        checked_cache_reused,
    ))
}

/// Loads or compiles checked VM artifacts for test and build commands.
pub(crate) fn compile_checked_vm_artifacts(
    path: &str,
    source: &str,
    state: &CliState,
    target_profile_options: TargetProfileCheckOptions,
) -> Result<CheckedSyntaxModuleArtifacts, std::process::ExitCode> {
    compile_checked_vm_artifacts_with_interfaces(path, source, state, target_profile_options, None)
}

/// Loads or compiles checked VM artifacts against an authoritative source graph.
pub(crate) fn compile_checked_vm_artifacts_with_interfaces(
    path: &str,
    source: &str,
    state: &CliState,
    target_profile_options: TargetProfileCheckOptions,
    source_graph: Option<&crate::formal_pipeline::interface_graph::SourceInterfaceGraph>,
) -> Result<CheckedSyntaxModuleArtifacts, std::process::ExitCode> {
    match load_or_compile_checked_vm_artifacts(
        path,
        source,
        state,
        target_profile_options,
        source_graph,
    ) {
        Ok((compiled, _)) => Ok(compiled),
        Err(BuildOneError::Message(message)) => {
            eprintln!("{message}");
            Err(std::process::ExitCode::from(1))
        }
        Err(BuildOneError::Exit(exit_code)) => Err(exit_code),
    }
}

/// Publishes already-checked VM artifacts for reuse by later commands.
pub(crate) fn publish_checked_vm_artifacts(
    path: &str,
    source: &str,
    syntax_output: &crate::terlan_syntax::SyntaxModuleOutput,
    interfaces: &std::collections::HashMap<String, crate::terlan_hir::ModuleInterface>,
    core: &CoreModule,
    state: &CliState,
) -> Result<(), std::process::ExitCode> {
    checked_cache::publish_checked_implementation(
        path,
        source,
        syntax_output,
        interfaces,
        core,
        state,
    )
    .map_err(|error| {
        eprintln!("{error}");
        std::process::ExitCode::from(1)
    })
}

fn load_or_compile_checked_vm_artifacts(
    path: &str,
    source_text: &str,
    state: &CliState,
    target_profile_options: TargetProfileCheckOptions,
    source_graph: Option<&crate::formal_pipeline::interface_graph::SourceInterfaceGraph>,
) -> Result<(CheckedSyntaxModuleArtifacts, bool), BuildOneError> {
    if state.incremental {
        if let Some(compiled) = checked_cache::load_checked_implementation(
            path,
            source_text,
            state,
            target_profile_options,
            source_graph,
        )? {
            checked_cache::publish_checked_dependency_manifest(
                path,
                source_text,
                &compiled.syntax_output,
                &compiled.interfaces,
                &compiled.core,
                state,
            )?;
            return Ok((compiled, true));
        }
    }
    let compiled = crate::formal_pipeline::compile_syntax_module_with_interface_graph(
        path,
        source_text,
        state.diagnostic_format,
        crate::formal_pipeline::InterfaceInputs {
            cache_dir: state.cache_dir.as_deref(),
            source_graph,
        },
        state.native_policy,
        TargetProfile::Vm,
        target_profile_options,
    )
    .map_err(BuildOneError::Exit)?;
    checked_cache::publish_checked_implementation(
        path,
        source_text,
        &compiled.syntax_output,
        &compiled.interfaces,
        &compiled.core,
        state,
    )?;
    Ok((compiled, false))
}

/// Projects checked frontend output into the state required by native linking.
///
/// Compiler-sized package builds must not retain parsed syntax or a complete
/// interface graph for every module. Debug metadata is projected while syntax
/// is present, then only backend-neutral CoreIR and that compact projection are
/// carried into application linking.
fn compact_compiled_module(
    source_path: String,
    source_text: String,
    compiled: CheckedSyntaxModuleArtifacts,
    _checked_cache_reused: bool,
) -> CompiledVmModule {
    let CheckedSyntaxModuleArtifacts {
        syntax_output,
        interfaces,
        core,
    } = compiled;
    let native_debug =
        NativeDebugInput::capture_owned(source_path.clone(), source_text, &core, &syntax_output);
    drop(syntax_output);
    drop(interfaces);
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    if std::env::var_os("TERLAN_BUILD_LOW_MEMORY").is_some() {
        // SAFETY: `malloc_trim` is a process-wide, thread-safe glibc allocator
        // operation. It neither receives nor exposes Rust allocation pointers.
        unsafe {
            malloc_trim(0);
        }
    }
    CompiledVmModule {
        source_path,
        compiled: CompiledVmArtifacts { core },
        native_debug,
        #[cfg(test)]
        checked_cache_reused: _checked_cache_reused,
    }
}
