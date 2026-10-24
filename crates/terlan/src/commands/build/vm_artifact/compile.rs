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
    if state.incremental {
        if let Some(compiled) =
            checked_cache::load_checked_implementation(path, &source_text, state)?
        {
            return Ok(compact_compiled_module(
                path.to_string(),
                source_text,
                compiled,
                true,
            ));
        }
    }
    let compiled =
        crate::formal_pipeline::compile_syntax_module_through_phases_with_profile_options(
            path,
            &source_text,
            state.diagnostic_format,
            state.cache_dir.as_deref(),
            state.native_policy,
            TargetProfile::Vm,
            TargetProfileCheckOptions {
                allow_asset_imports: false,
                allow_rust_backed_std_modules: state.native_policy != NativePolicy::Pure,
            },
        )
        .map_err(BuildOneError::Exit)?;
    checked_cache::publish_checked_implementation(path, &source_text, &compiled, state)?;
    Ok(compact_compiled_module(
        path.to_string(),
        source_text,
        compiled,
        false,
    ))
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
