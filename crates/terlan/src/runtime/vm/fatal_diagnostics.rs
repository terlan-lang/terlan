use serde::Serialize;
#[cfg(test)]
use std::{
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

use super::native_image_diagnostics::VmNativeImageDiagnosticMetadata;

use super::process::{
    VmExitReason, VmProcessId, VmProcessResumeState, VmProcessSnapshot, VmProcessState,
    VmProcessTable,
};
use super::scheduler::VmScheduler;

const VM_FATAL_DIAGNOSTIC_SCHEMA: &str = "terlan.vm.fatal-diagnostic.v2";
const MAX_CAUSE_CODE_BYTES: usize = 128;

/// Explicit policy controlling bounded fatal-diagnostic capture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmFatalDiagnosticPolicy {
    #[cfg(test)]
    Disabled,
    Enabled {
        max_subjects: usize,
        max_output_bytes: usize,
    },
}

impl VmFatalDiagnosticPolicy {
    /// Creates a bounded enabled policy and rejects non-progressing limits.
    pub(crate) fn enabled(max_subjects: usize, max_output_bytes: usize) -> Result<Self, String> {
        if max_subjects == 0 {
            return Err("fatal diagnostic subject limit must be positive".to_string());
        }
        if max_output_bytes == 0 {
            return Err("fatal diagnostic output limit must be positive".to_string());
        }
        Ok(Self::Enabled {
            max_subjects,
            max_output_bytes,
        })
    }
}

/// Stable source-facing frame retained without a host-local source path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmFatalDiagnosticFrame {
    pub(crate) module: String,
    pub(crate) function: String,
    pub(crate) arity: usize,
    pub(crate) instruction_offset: usize,
}

/// Bounded process record captured in a fatal diagnostic bundle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmFatalProcessSnapshot {
    pub(crate) pid: u64,
    pub(crate) parent: Option<u64>,
    pub(crate) state: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) resume_state: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) exit_kind: Option<&'static str>,
    pub(crate) reductions: u64,
    pub(crate) heap_bytes: usize,
    pub(crate) mailbox_messages: usize,
    pub(crate) cancellation_requested: bool,
    pub(crate) resource_handle_count: usize,
    pub(crate) registered_names: Vec<String>,
    pub(crate) stack: Vec<VmFatalDiagnosticFrame>,
}

/// Scheduler state captured beside fatal process diagnostics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmFatalSchedulerSnapshot {
    pub(crate) tick: u64,
    pub(crate) queued_processes: Vec<u64>,
    pub(crate) total_reductions: u64,
    pub(crate) total_slices: u64,
    pub(crate) preemptions: u64,
    pub(crate) max_queue_depth: usize,
}

/// Complete deterministic fatal support bundle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmFatalDiagnosticBundle {
    pub(crate) schema: &'static str,
    pub(crate) generation: u64,
    pub(crate) cause_code: String,
    /// Admitted native image generation active when the failure was captured.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) native_image: Option<VmNativeImageDiagnosticMetadata>,
    pub(crate) scheduler: VmFatalSchedulerSnapshot,
    pub(crate) processes: Vec<VmFatalProcessSnapshot>,
    pub(crate) missing_processes: Vec<u64>,
    pub(crate) complete: bool,
    #[serde(skip)]
    max_output_bytes: usize,
}

impl VmFatalDiagnosticBundle {
    /// Captures all retained processes and explicitly observed missing identities.
    ///
    /// Disabled policy returns no bundle before validating capture inputs. An
    /// enabled capture is deterministic and mutation-free: it does not stop,
    /// wake, schedule, or otherwise alter any process.
    #[cfg(test)]
    pub(crate) fn capture(
        policy: VmFatalDiagnosticPolicy,
        generation: u64,
        cause_code: &str,
        processes: &VmProcessTable,
        scheduler: &VmScheduler,
        observed_processes: &[VmProcessId],
    ) -> Result<Option<Self>, String> {
        Self::capture_with_native_image(
            policy,
            generation,
            cause_code,
            processes,
            scheduler,
            observed_processes,
            None,
        )
    }

    /// Captures bounded fatal state with an optional admitted native generation.
    pub(crate) fn capture_with_native_image(
        policy: VmFatalDiagnosticPolicy,
        generation: u64,
        cause_code: &str,
        processes: &VmProcessTable,
        scheduler: &VmScheduler,
        observed_processes: &[VmProcessId],
        native_image: Option<VmNativeImageDiagnosticMetadata>,
    ) -> Result<Option<Self>, String> {
        let (max_subjects, max_output_bytes) = match policy {
            #[cfg(test)]
            VmFatalDiagnosticPolicy::Disabled => return Ok(None),
            VmFatalDiagnosticPolicy::Enabled {
                max_subjects,
                max_output_bytes,
            } => (max_subjects, max_output_bytes),
        };
        if generation == 0 {
            return Err("fatal diagnostic generation must be nonzero".to_string());
        }
        validate_cause_code(cause_code)?;

        let snapshots = processes.snapshots();
        let mut missing_processes = observed_processes
            .iter()
            .filter(|pid| processes.get(**pid).is_none())
            .map(|pid| pid.as_u64())
            .collect::<Vec<_>>();
        missing_processes.sort_unstable();
        missing_processes.dedup();
        let subject_count = snapshots
            .len()
            .checked_add(missing_processes.len())
            .ok_or_else(|| "fatal diagnostic subject count overflow".to_string())?;
        if subject_count > max_subjects {
            return Err(format!(
                "fatal diagnostic subject limit exceeded: {subject_count} > {max_subjects}"
            ));
        }

        let metrics = scheduler.metrics();
        let bundle = Self {
            schema: VM_FATAL_DIAGNOSTIC_SCHEMA,
            generation,
            cause_code: cause_code.to_string(),
            native_image,
            scheduler: VmFatalSchedulerSnapshot {
                tick: scheduler.diagnostic_tick(),
                queued_processes: scheduler
                    .diagnostic_queued_processes()
                    .into_iter()
                    .map(VmProcessId::as_u64)
                    .collect(),
                total_reductions: metrics.total_reductions,
                total_slices: metrics.total_slices,
                preemptions: metrics.preemptions,
                max_queue_depth: metrics.max_queue_depth,
            },
            processes: snapshots
                .into_iter()
                .map(VmFatalProcessSnapshot::from)
                .collect(),
            missing_processes,
            complete: true,
            max_output_bytes,
        };
        bundle.serialized_bytes()?;
        Ok(Some(bundle))
    }

    /// Serializes the complete bundle while enforcing its capture byte limit.
    pub(crate) fn serialized_bytes(&self) -> Result<Vec<u8>, String> {
        let mut bytes = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("failed to serialize fatal diagnostic bundle: {error}"))?;
        bytes.push(b'\n');
        if bytes.len() > self.max_output_bytes {
            return Err(format!(
                "fatal diagnostic output limit exceeded: {} > {}",
                bytes.len(),
                self.max_output_bytes
            ));
        }
        Ok(bytes)
    }

    /// Publishes a new complete bundle through a synced same-directory hard link.
    ///
    /// Existing destinations are never overwritten. Any failed write removes
    /// the private partial file and leaves the destination absent.
    #[cfg(test)]
    pub(crate) fn publish_atomic(&self, path: &Path) -> Result<(), String> {
        if path.file_name().is_none() {
            return Err("fatal diagnostic path must name a file".to_string());
        }
        if path.exists() {
            return Err(format!(
                "fatal diagnostic destination already exists: {}",
                path.display()
            ));
        }
        let bytes = self.serialized_bytes()?;
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create fatal diagnostic directory {}: {error}",
                parent.display()
            )
        })?;
        let partial = partial_path(path, self.generation)?;
        let publication = (|| -> Result<(), String> {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&partial)
                .map_err(|error| {
                    format!(
                        "failed to create fatal diagnostic partial file {}: {error}",
                        partial.display()
                    )
                })?;
            file.write_all(&bytes).map_err(|error| {
                format!(
                    "failed to write fatal diagnostic partial file {}: {error}",
                    partial.display()
                )
            })?;
            file.sync_all().map_err(|error| {
                format!(
                    "failed to sync fatal diagnostic partial file {}: {error}",
                    partial.display()
                )
            })?;
            fs::hard_link(&partial, path).map_err(|error| {
                format!(
                    "failed to publish fatal diagnostic {}: {error}",
                    path.display()
                )
            })?;
            fs::remove_file(&partial).map_err(|error| {
                format!(
                    "published fatal diagnostic but failed to remove partial link {}: {error}",
                    partial.display()
                )
            })
        })();
        if publication.is_err() {
            let _ = fs::remove_file(&partial);
        }
        publication
    }
}

impl From<VmProcessSnapshot> for VmFatalProcessSnapshot {
    fn from(snapshot: VmProcessSnapshot) -> Self {
        let (state, resume_state, exit_kind) = process_state_labels(&snapshot.state);
        Self {
            pid: snapshot.pid.as_u64(),
            parent: snapshot.parent.map(VmProcessId::as_u64),
            state,
            resume_state,
            exit_kind,
            reductions: snapshot.reductions,
            heap_bytes: snapshot.heap_bytes,
            mailbox_messages: snapshot.mailbox_messages,
            cancellation_requested: snapshot.cancellation_requested,
            resource_handle_count: snapshot.resource_handles.len(),
            registered_names: snapshot.registered_names,
            stack: snapshot
                .current_stacktrace
                .into_iter()
                .map(|location| VmFatalDiagnosticFrame {
                    module: location.source.module,
                    function: location.source.function,
                    arity: location.source.arity,
                    instruction_offset: location.instruction_offset,
                })
                .collect(),
        }
    }
}

fn process_state_labels(
    state: &VmProcessState,
) -> (&'static str, Option<&'static str>, Option<&'static str>) {
    match state {
        VmProcessState::Runnable => ("runnable", None, None),
        VmProcessState::Blocked => ("blocked", None, None),
        VmProcessState::Hibernated => ("hibernated", None, None),
        VmProcessState::Suspended(VmProcessResumeState::Runnable) => {
            ("suspended", Some("runnable"), None)
        }
        VmProcessState::Suspended(VmProcessResumeState::Blocked) => {
            ("suspended", Some("blocked"), None)
        }
        VmProcessState::Suspended(VmProcessResumeState::Hibernated) => {
            ("suspended", Some("hibernated"), None)
        }
        VmProcessState::Exited(reason) => ("exited", None, Some(exit_reason_kind(reason))),
    }
}

fn exit_reason_kind(reason: &VmExitReason) -> &'static str {
    match reason {
        VmExitReason::Normal => "normal",
        VmExitReason::Error(_) => "error",
        VmExitReason::Killed => "killed",
        VmExitReason::ShutdownTimeout { .. } => "shutdown-timeout",
        VmExitReason::MemoryLimitExceeded { .. } => "memory-limit-exceeded",
    }
}

fn validate_cause_code(cause_code: &str) -> Result<(), String> {
    if cause_code.is_empty() || cause_code.len() > MAX_CAUSE_CODE_BYTES {
        return Err("fatal diagnostic cause code must contain 1..=128 bytes".to_string());
    }
    if !cause_code
        .bytes()
        .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"._-".contains(&byte))
    {
        return Err("fatal diagnostic cause code contains unsupported characters".to_string());
    }
    Ok(())
}

#[cfg(test)]
fn partial_path(path: &Path, generation: u64) -> Result<PathBuf, String> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "fatal diagnostic file name must be valid UTF-8".to_string())?;
    Ok(path.with_file_name(format!(
        ".{file_name}.{}.{}.partial",
        std::process::id(),
        generation
    )))
}

#[cfg(test)]
#[path = "fatal_diagnostics_test.rs"]
#[cfg(test)]
mod fatal_diagnostics_test;

#[cfg(test)]
#[path = "fatal_diagnostics_ignore_cores_parity_test.rs"]
#[cfg(test)]
mod fatal_diagnostics_ignore_cores_parity_test;
