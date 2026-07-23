//! Compiler-owned native image generation for source hot reload.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::runtime::vm::code_server_compiler::stage_compiled_replacement;
use crate::runtime::vm::pure_native::{
    VmNativeGenerationReferenceClass, VmNativeGenerationReferenceSnapshot,
};
use crate::runtime::vm::source_reload::{VmSourceReloadAdapter, VmSourceReloadBatchReport};
use crate::runtime::vm::ReplValue;
use crate::terlan_typeck::CoreModule;
use crate::CliState;

/// Default monotonic-tick budget for one command-driven generation drain.
const DEFAULT_RELOAD_DRAIN_TICKS: u64 = 30_000;

/// Compiler and runtime state retained across watcher generations.
#[derive(Debug)]
pub(super) struct VmNativeSourceReloadService {
    /// Long-lived runtime adapter that owns the active image generation.
    runtime: VmSourceReloadAdapter,
    /// Next compiler generation identity local to this watcher session.
    generation_sequence: u64,
    /// Monotonic origin used by the default watcher-facing deadline policy.
    clock_origin: Instant,
    /// Stable deadline retained while one generation is draining.
    drain_deadline_tick: Option<u64>,
}

/// Inspectable result of one compiled native source-reload generation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct VmNativeSourceReloadReport {
    /// Source classification and code-server publication report.
    pub(super) sources: VmSourceReloadBatchReport,
    /// Native shard generation admitted before source metadata publication.
    pub(super) native_generation: u64,
    /// Content-addressed native image selected by the compiler.
    pub(super) native_image: PathBuf,
    /// Reference proof captured immediately after admission.
    pub(super) references: VmNativeGenerationReferenceSnapshot,
    /// Bounded scheduler evidence ending at the admitted image generation.
    pub(super) replay: crate::runtime::vm::multicore_replay::VmMulticoreReplayEvidence,
}

/// One source file compiled through the formal frontend exactly once.
struct PreparedReloadSource {
    /// Filesystem identity used by diagnostics and source maps.
    source_name: String,
    /// Source text used to derive code-server checksums.
    source: String,
    /// Checked CoreIR used by native application lowering.
    core: CoreModule,
}

impl VmNativeSourceReloadService {
    /// Creates an empty long-lived hot-reload service.
    pub(super) fn new() -> Self {
        Self {
            runtime: VmSourceReloadAdapter::new(),
            generation_sequence: 0,
            clock_origin: Instant::now(),
            drain_deadline_tick: None,
        }
    }

    /// Compiles and admits one watcher batch with the default drain budget.
    pub(super) fn reload(
        &mut self,
        paths: &[PathBuf],
        state: &CliState,
    ) -> Result<VmNativeSourceReloadReport, String> {
        let observed_tick =
            u64::try_from(self.clock_origin.elapsed().as_millis()).unwrap_or(u64::MAX);
        let deadline_tick = match self.drain_deadline_tick {
            Some(deadline) => deadline,
            None => {
                let deadline = observed_tick
                    .checked_add(DEFAULT_RELOAD_DRAIN_TICKS)
                    .unwrap_or(u64::MAX);
                self.drain_deadline_tick = Some(deadline);
                deadline
            }
        };
        let result = self.reload_at(paths, state, observed_tick, deadline_tick);
        if result.is_ok()
            || result
                .as_ref()
                .is_err_and(|error| error.contains("generation_quarantined"))
        {
            self.drain_deadline_tick = None;
        }
        result
    }

    /// Compiles and admits one watcher batch at an explicit drain deadline.
    pub(super) fn reload_at(
        &mut self,
        paths: &[PathBuf],
        state: &CliState,
        observed_tick: u64,
        deadline_tick: u64,
    ) -> Result<VmNativeSourceReloadReport, String> {
        let (prepared, mut source_report) = prepare_sources(paths, state)?;
        if prepared.is_empty() {
            return Err("terlc vm reload did not receive any .terl source files".to_string());
        }
        self.generation_sequence = self.generation_sequence.checked_add(1).ok_or_else(|| {
            "error[vm.reload.generation]: compiler generation identity exhausted".to_string()
        })?;
        let workspace = state.out_dir.join("vm-reload-aot");
        let native_cache_root = workspace.join("native-aot");
        let generation_stem = format!("reload_generation_{}", self.generation_sequence);
        let cores = prepared
            .iter()
            .map(|source| &source.core)
            .collect::<Vec<_>>();
        let image = crate::commands::build::vm_artifact::native_image::compile_reload_native_image(
            &workspace,
            &native_cache_root,
            &generation_stem,
            &cores,
        )?
        .ok_or_else(|| {
            "error[vm.reload.aot_required]: source batch did not produce a native image".to_string()
        })?;
        let staged = prepared
            .iter()
            .map(|source| {
                stage_compiled_replacement(&source.source_name, &source.source, &source.core)
            })
            .collect();
        let publication =
            self.runtime
                .publish_native_generation(staged, &image, observed_tick, deadline_tick)?;
        source_report.events = publication.events;
        Ok(VmNativeSourceReloadReport {
            sources: source_report,
            native_generation: publication.generation,
            native_image: image,
            references: publication.references,
            replay: publication.replay,
        })
    }

    /// Executes one export through the active compiled generation.
    #[allow(dead_code)] // Long-lived watcher integrations consume this entry.
    pub(super) fn call(&mut self, function: &str, args: &[ReplValue]) -> Result<ReplValue, String> {
        self.runtime.call_native(function, args)
    }

    /// Pins one debugger or crash-metadata reference to the active generation.
    #[allow(dead_code)] // Long-lived debugger integration consumes this entry.
    pub(super) fn pin_generation(
        &mut self,
        class: VmNativeGenerationReferenceClass,
    ) -> Result<(), String> {
        self.runtime.pin_native_generation(class)
    }

    /// Releases one externally owned generation pin.
    #[allow(dead_code)] // Paired with the long-lived generation pin entry.
    pub(super) fn release_generation(
        &mut self,
        class: VmNativeGenerationReferenceClass,
    ) -> Result<(), String> {
        self.runtime.release_native_generation(class)
    }

    /// Returns bounded image-publication evidence for focused lifecycle tests.
    #[cfg(test)]
    pub(super) fn replay_evidence(
        &self,
    ) -> Result<crate::runtime::vm::multicore_replay::VmMulticoreReplayEvidence, String> {
        self.runtime.multicore_replay_evidence()
    }
}

impl Default for VmNativeSourceReloadService {
    /// Creates the default long-lived reload service and monotonic clock.
    fn default() -> Self {
        Self::new()
    }
}

/// Reads, classifies, and compiles every unique Terlan source before mutation.
fn prepare_sources(
    paths: &[PathBuf],
    state: &CliState,
) -> Result<(Vec<PreparedReloadSource>, VmSourceReloadBatchReport), String> {
    let mut seen = BTreeSet::new();
    let mut prepared = Vec::new();
    let mut report = VmSourceReloadBatchReport {
        changed_paths: paths.len(),
        unique_source_paths: 0,
        ignored_paths: 0,
        duplicate_source_paths: 0,
        events: Vec::new(),
    };
    for path in paths {
        if !is_terlan_source(path) {
            report.ignored_paths = report.ignored_paths.saturating_add(1);
            continue;
        }
        if !seen.insert(path.clone()) {
            report.duplicate_source_paths = report.duplicate_source_paths.saturating_add(1);
            continue;
        }
        report.unique_source_paths = report.unique_source_paths.saturating_add(1);
        let source = fs::read_to_string(path).map_err(|error| {
            format!(
                "failed to read changed Terlan source `{}`: {error}",
                path.display()
            )
        })?;
        let source_name = path.display().to_string();
        let artifacts = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
            &source_name,
            &source,
            state.diagnostic_format,
            state.cache_dir.as_deref(),
            state.native_policy,
            state.target_profile,
        )
        .map_err(|code| {
            format!("source hot reload compile failed for `{source_name}` with exit code {code:?}")
        })?;
        prepared.push(PreparedReloadSource {
            source_name,
            source,
            core: artifacts.core,
        });
    }
    Ok((prepared, report))
}

/// Returns whether one watcher path is a Terlan implementation source.
fn is_terlan_source(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "terl")
}
