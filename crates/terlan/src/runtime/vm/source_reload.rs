#![allow(dead_code)]

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use super::code_server::{
    VmCodeServer, VmCodeServerEvent, VmCodeServerEventSnapshot, VmStagedModuleArtifact,
};

use super::code_server_compiler::stage_source_replacement;
use super::fixed_scheduler_telemetry::VM_FIXED_SCHEDULER_TRACE_CAPACITY;
use super::multicore_replay::VmMulticoreReplayEvidence;
use super::pure_native::{
    PureNativeExecutionShard, VmNativeGenerationReferenceClass, VmNativeGenerationReferenceSnapshot,
};
use super::ReplValue;

/// VM-owned source reload adapter.
///
/// Inputs:
/// - Source file changes reported by a dev command, watcher, or REPL bridge.
///
/// Output:
/// - New `VmCodeServer` generations for changed Terlan modules.
///
/// Transformation:
/// - Keeps filesystem event handling outside the VM while centralizing the
///   source-to-generation publication step that preserves hot-reload semantics.
#[derive(Debug)]
pub(crate) struct VmSourceReloadAdapter {
    code_server: VmCodeServer,
    native_shard: Option<PureNativeExecutionShard>,
}

/// Native generation admitted by one atomic source-reload publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmNativeReloadPublication {
    /// Newly admitted shard generation.
    pub(crate) generation: u64,
    /// Reference proof captured after publication.
    pub(crate) references: VmNativeGenerationReferenceSnapshot,
    /// Code-server events made visible after native admission.
    pub(crate) events: Vec<VmCodeServerEvent>,
    /// Bounded scheduler evidence ending at this image publication.
    pub(crate) replay: VmMulticoreReplayEvidence,
}

/// Inspectable outcome for one source-reload path batch.
///
/// Inputs:
/// - Watcher, dev-server, or CLI path batch passed to the reload adapter.
///
/// Output:
/// - Counts for source, ignored, and duplicate paths plus publication events.
///
/// Transformation:
/// - Gives VM/debug tooling a stable view of reload work without exposing
///   mutable code-server internals or filesystem watcher implementation
///   details.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmSourceReloadBatchReport {
    pub(crate) changed_paths: usize,
    pub(crate) unique_source_paths: usize,
    pub(crate) ignored_paths: usize,
    pub(crate) duplicate_source_paths: usize,
    pub(crate) events: Vec<VmCodeServerEvent>,
}

impl VmSourceReloadAdapter {
    /// Creates a source reload adapter with an empty code server.
    ///
    /// Inputs:
    /// - None.
    ///
    /// Output:
    /// - Adapter ready to publish changed Terlan source files.
    ///
    /// Transformation:
    /// - Initializes the code-server boundary used by later file publications.
    pub(crate) fn new() -> Self {
        Self {
            code_server: VmCodeServer::default(),
            native_shard: None,
        }
    }

    /// Admits one compiled native generation and then publishes its metadata.
    ///
    /// The old image remains loaded while references drain. If the deadline is
    /// reached, its shard is quarantined and staged code-server metadata stays
    /// invisible, preventing the compiler and runtime generations from
    /// diverging.
    pub(crate) fn publish_native_generation(
        &mut self,
        staged: Vec<VmStagedModuleArtifact>,
        image_path: &Path,
        observed_tick: u64,
        deadline_tick: u64,
    ) -> Result<VmNativeReloadPublication, String> {
        VmCodeServer::validate_staged_batch(&staged)?;
        let generation = match self.native_shard.as_mut() {
            Some(shard) => {
                shard.replace_image_before_deadline(image_path, observed_tick, deadline_tick)?
            }
            None => {
                let shard = PureNativeExecutionShard::load_image(image_path)?;
                let generation = shard.generation()?;
                self.native_shard = Some(shard);
                generation
            }
        };
        let replay = self.multicore_replay_evidence()?;
        let events = self.publish_compiled_sources(staged)?;
        let references = self
            .native_shard
            .as_ref()
            .expect("successful native admission owns a shard")
            .generation_references();
        Ok(VmNativeReloadPublication {
            generation: generation.as_u64(),
            references,
            events,
            replay,
        })
    }

    /// Executes one export through the currently admitted native generation.
    pub(crate) fn call_native(
        &mut self,
        function: &str,
        args: &[ReplValue],
    ) -> Result<ReplValue, String> {
        self.native_shard
            .as_mut()
            .ok_or_else(|| {
                "error[vm.reload.native_generation]: no native generation is admitted".to_string()
            })?
            .call(function, args)
    }

    /// Pins one externally owned reference to the active native generation.
    pub(crate) fn pin_native_generation(
        &mut self,
        class: VmNativeGenerationReferenceClass,
    ) -> Result<(), String> {
        self.native_shard
            .as_mut()
            .ok_or_else(|| {
                "error[vm.reload.native_generation]: no native generation is admitted".to_string()
            })?
            .pin_generation_reference(class)
    }

    /// Releases one externally owned reference to the active generation.
    pub(crate) fn release_native_generation(
        &mut self,
        class: VmNativeGenerationReferenceClass,
    ) -> Result<(), String> {
        self.native_shard
            .as_mut()
            .ok_or_else(|| {
                "error[vm.reload.native_generation]: no native generation is admitted".to_string()
            })?
            .release_generation_reference(class)
    }

    /// Publishes one changed source file into the VM code server.
    ///
    /// Inputs:
    /// - `path`: changed filesystem path from a watcher or dev command.
    ///
    /// Output:
    /// - `Some(event)` for `.terl` files that compile and publish.
    /// - `None` for non-Terlan paths that the VM reload path ignores.
    /// - Error text for unreadable or invalid Terlan source.
    ///
    /// Transformation:
    /// - Reads Terlan source text from disk and delegates to
    ///   `VmCodeServer::publish_source`, leaving generation retention,
    ///   rollback safety, and event inspection owned by the code server.
    pub(crate) fn publish_changed_file(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<Option<VmCodeServerEvent>, String> {
        let path = path.as_ref();
        if !is_terlan_source_path(path) {
            return Ok(None);
        }

        let source = fs::read_to_string(path).map_err(|err| {
            format!(
                "failed to read changed Terlan source `{}`: {err}",
                path.display()
            )
        })?;
        self.code_server
            .publish_source(&path.display().to_string(), &source)
            .map(Some)
    }

    /// Publishes a watcher-style batch of changed paths.
    ///
    /// Inputs:
    /// - `paths`: source or asset paths reported by a filesystem watcher.
    ///
    /// Output:
    /// - Ordered publication events for unique Terlan source paths only.
    /// - Error text when any readable Terlan source fails to compile/publish.
    ///
    /// Transformation:
    /// - Reads and compiles every Terlan source path before publishing any
    ///   generation. This gives future `terlc dev` and watcher integrations an
    ///   atomic batch boundary: noisy asset events are ignored, duplicate
    ///   source events are collapsed, and invalid source batches cannot leave
    ///   the VM partially reloaded.
    pub(crate) fn publish_changed_files(
        &mut self,
        paths: &[impl AsRef<Path>],
    ) -> Result<Vec<VmCodeServerEvent>, String> {
        Ok(self.publish_changed_files_with_report(paths)?.events)
    }

    /// Publishes a watcher-style path batch and returns inspectable diagnostics.
    ///
    /// Inputs:
    /// - `paths`: source or asset paths reported by a filesystem watcher.
    ///
    /// Output:
    /// - A batch report containing path-classification counts and publication
    ///   events.
    /// - Error text when any readable Terlan source fails to compile/publish.
    ///
    /// Transformation:
    /// - Preserves the atomic compile-before-publish transaction while making
    ///   reload work visible to VM CLI/debug tooling and future HTTP dev-server
    ///   diagnostics.
    pub(crate) fn publish_changed_files_with_report(
        &mut self,
        paths: &[impl AsRef<Path>],
    ) -> Result<VmSourceReloadBatchReport, String> {
        let mut compiled = Vec::new();
        let mut seen_sources = BTreeSet::new();
        let mut report = VmSourceReloadBatchReport {
            changed_paths: paths.len(),
            unique_source_paths: 0,
            ignored_paths: 0,
            duplicate_source_paths: 0,
            events: Vec::new(),
        };
        for path in paths {
            let path = path.as_ref();
            if !is_terlan_source_path(path) {
                report.ignored_paths = report.ignored_paths.saturating_add(1);
                continue;
            }
            let source_key = path.to_path_buf();
            if !seen_sources.insert(source_key) {
                report.duplicate_source_paths = report.duplicate_source_paths.saturating_add(1);
                continue;
            }
            report.unique_source_paths = report.unique_source_paths.saturating_add(1);
            let source = fs::read_to_string(path).map_err(|err| {
                format!(
                    "failed to read changed Terlan source `{}`: {err}",
                    path.display()
                )
            })?;
            compiled.push(stage_source_replacement(
                &path.display().to_string(),
                &source,
            )?);
        }
        report.events = self.publish_compiled_sources(compiled)?;
        Ok(report)
    }

    /// Publishes precompiled source artifacts into the code server.
    fn publish_compiled_sources(
        &mut self,
        compiled: Vec<VmStagedModuleArtifact>,
    ) -> Result<Vec<VmCodeServerEvent>, String> {
        self.code_server.publish_staged_batch(compiled)
    }

    /// Returns the ordered reload event inspection stream.
    ///
    /// Inputs:
    /// - None.
    ///
    /// Output:
    /// - Event snapshots recorded by the owned code server.
    ///
    /// Transformation:
    /// - Exposes source-facing reload history without giving callers mutable
    ///   access to code-server generation tables.
    pub(crate) fn event_snapshots(&self) -> Vec<VmCodeServerEventSnapshot> {
        self.code_server.event_snapshots()
    }

    /// Captures bounded reload evidence for the currently admitted generation.
    pub(crate) fn multicore_replay_evidence(&self) -> Result<VmMulticoreReplayEvidence, String> {
        let shard = self.native_shard.as_ref().ok_or_else(|| {
            "error[vm.reload.native_generation]: no native generation is admitted".to_string()
        })?;
        let generation = shard.generation()?.as_u64();
        VmMulticoreReplayEvidence::new(
            generation,
            1,
            VM_FIXED_SCHEDULER_TRACE_CAPACITY,
            vec![shard.lifecycle_replay_capture()?],
        )
        .map_err(|error| format!("error[vm.reload.replay]: {error}"))
    }
}

impl Default for VmSourceReloadAdapter {
    /// Creates the canonical bounded recorder used by source reload.
    fn default() -> Self {
        Self::new()
    }
}

/// Returns whether a changed path is a Terlan source file.
///
/// Inputs:
/// - `path`: changed filesystem path.
///
/// Output:
/// - `true` for `.terl` source files.
///
/// Transformation:
/// - Keeps extension filtering shared by watcher adapters and tests without
///   coupling it to any filesystem-watch crate.
fn is_terlan_source_path(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension == "terl")
}

#[cfg(test)]
#[path = "source_reload_test.rs"]
mod source_reload_test;
