//! Compiler-dependent staging for the shard-local VM code registry.

use crate::commands::artifacts::fingerprint;
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::{ColorChoice, DiagnosticFormat};

use crate::runtime::vm::code_server::{
    VmCodeServer, VmCodeServerEvent, VmModuleArtifact, VmStagedModuleArtifact,
};

/// Compiles one source replacement before it becomes visible to actors.
///
/// This module-level boundary is used by watcher/REPL source replacement so
/// compiler ownership remains explicit instead of hiding behind an unrelated
/// inherent method call.
pub(crate) fn stage_source_replacement(
    source_name: &str,
    source: &str,
) -> Result<VmStagedModuleArtifact, String> {
    VmCodeServer::stage_source(source_name, source)
}

/// Stages compiler-checked CoreIR without repeating frontend compilation.
pub(crate) fn stage_compiled_replacement(
    source_name: &str,
    source: &str,
    core: &crate::terlan_typeck::CoreModule,
) -> VmStagedModuleArtifact {
    let (module, artifact) = VmCodeServer::artifact_from_compiled_source(source_name, source, core);
    VmStagedModuleArtifact { module, artifact }
}

/// Publishes one fully staged source replacement as an atomic generation.
pub(crate) fn publish_staged_replacement(
    code_server: &mut VmCodeServer,
    staged: VmStagedModuleArtifact,
) -> VmCodeServerEvent {
    code_server.publish_staged(staged)
}

impl VmCodeServer {
    /// Compiles source into an artifact that remains invisible until publish.
    pub(crate) fn stage_source(
        source_name: &str,
        source: &str,
    ) -> Result<VmStagedModuleArtifact, String> {
        let (module, artifact) = Self::compile_source_artifact(source_name, source)?;
        Ok(VmStagedModuleArtifact { module, artifact })
    }

    /// Compiles Terlan source into VM generation metadata.
    pub(crate) fn compile_source_artifact(
        source_name: &str,
        source: &str,
    ) -> Result<(String, VmModuleArtifact), String> {
        let artifacts = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
            source_name,
            source,
            DiagnosticFormat::Text {
                color: ColorChoice::Never,
            },
            None,
            NativePolicy::NativeBoundaryOptional,
            TargetProfile::default(),
        )
        .map_err(|code| {
            format!(
                "source hot reload compile failed for `{source_name}` with exit code {:?}",
                code
            )
        })?;
        Ok(Self::artifact_from_compiled_source(
            source_name,
            source,
            &artifacts.core,
        ))
    }

    /// Publishes already checked CoreIR without compiling the source again.
    pub(crate) fn publish_compiled_source(
        &mut self,
        source_name: &str,
        source: &str,
        core: &crate::terlan_typeck::CoreModule,
    ) -> VmCodeServerEvent {
        let (module, artifact) = Self::artifact_from_compiled_source(source_name, source, core);
        self.publish(module, artifact)
    }

    /// Converts checked CoreIR into canonical code-server publication metadata.
    fn artifact_from_compiled_source(
        source_name: &str,
        source: &str,
        core: &crate::terlan_typeck::CoreModule,
    ) -> (String, VmModuleArtifact) {
        let exported_functions = core.exports.iter().filter_map(|export| {
            let crate::terlan_typeck::CoreExportKind::Function { arity } = &export.kind else {
                return None;
            };
            Some((export.name.clone(), *arity))
        });
        let module = core.module.clone();
        let checksum = format!(
            "source-fnv1a64:{:016x}",
            fingerprint(format!("{module}\n{source}").as_bytes())
        );
        let source_map_id = format!("{source_name}:{checksum}");
        let artifact = VmModuleArtifact::new(checksum, source_map_id)
            .with_exported_functions(exported_functions);
        (module, artifact)
    }

    /// Compiles Terlan source and publishes its newest module generation.
    pub(crate) fn publish_source(
        &mut self,
        source_name: &str,
        source: &str,
    ) -> Result<VmCodeServerEvent, String> {
        let staged = stage_source_replacement(source_name, source)?;
        Ok(publish_staged_replacement(self, staged))
    }

    /// Publishes one previously compiled artifact as an atomic visibility step.
    pub(crate) fn publish_staged(&mut self, staged: VmStagedModuleArtifact) -> VmCodeServerEvent {
        self.publish(staged.module, staged.artifact)
    }
}
