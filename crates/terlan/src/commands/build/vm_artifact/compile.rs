//! Formal-pipeline entry for native VM artifact compilation.

use crate::formal_pipeline::CheckedSyntaxModuleArtifacts;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::{TargetProfile, TargetProfileCheckOptions};
use crate::CliState;

use super::super::BuildOneError;
use super::checked_cache;

/// Checked compiler output retained by VM application orchestration.
pub(super) struct CompiledVmModule {
    pub(super) source_path: String,
    pub(super) source_text: String,
    pub(super) compiled: CheckedSyntaxModuleArtifacts,
    /// Whether all checked implementation data came from the verified cache.
    #[cfg(test)]
    pub(super) checked_cache_reused: bool,
}

pub(super) fn compile_vm_module(
    path: &str,
    state: &CliState,
) -> Result<CompiledVmModule, BuildOneError> {
    let source_text = crate::support::read_file(path)
        .map_err(|error| BuildOneError::Message(error.to_string()))?;
    if state.incremental {
        if let Some(compiled) =
            checked_cache::load_checked_implementation(path, &source_text, state)
        {
            return Ok(CompiledVmModule {
                source_path: path.to_string(),
                source_text,
                compiled,
                #[cfg(test)]
                checked_cache_reused: true,
            });
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
    Ok(CompiledVmModule {
        source_path: path.to_string(),
        source_text,
        compiled,
        #[cfg(test)]
        checked_cache_reused: false,
    })
}
