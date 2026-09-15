//! Admit bounded Cargo JSON records while the compilation owner is still running.

use crate::cargo_harness_admission::{DeclaredHarness, ManifestAdmission};
use crate::PhaseFailure;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use terlan_process_owner::ProcessControl;

const MAX_RECORD_BYTES: usize = 1024 * 1024;
const MAX_STREAM_BYTES: usize = 16 * 1024 * 1024;
const MAX_HARNESSES: usize = 1024;

/// Owns framing, package snapshots, and unique native test artifact observations.
pub(super) struct CargoArtifactStream {
    pending: Vec<u8>,
    bytes: usize,
    manifests: ManifestAdmission,
    selected: fn(&Value) -> bool,
    targets: BTreeSet<String>,
    executables: BTreeSet<PathBuf>,
    harnesses: Vec<DeclaredHarness>,
    build_finished: bool,
    capture_started: bool,
    failed: bool,
    require_fresh_main: bool,
}

impl CargoArtifactStream {
    /// Selects the one library target owned by the union-feature Terlan build.
    pub(super) fn terlan_library(root: &Path) -> Result<Self, PhaseFailure> {
        Self::new(root, |message| {
            message["target"]["name"] == "terlan"
                && message["target"]["kind"] == serde_json::json!(["lib"])
                && message["profile"]["test"] == true
        })
    }

    /// Uses the same source-root and manifest admission as the direct Terlan owner.
    pub(super) fn new(root: &Path, selected: fn(&Value) -> bool) -> Result<Self, PhaseFailure> {
        Ok(Self {
            pending: Vec::new(),
            bytes: 0,
            manifests: ManifestAdmission::new(root)?,
            selected,
            targets: BTreeSet::new(),
            executables: BTreeSet::new(),
            harnesses: Vec::new(),
            build_finished: false,
            capture_started: false,
            failed: false,
            require_fresh_main: false,
        })
    }

    /// The workspace execution phase must reuse the already-built main library unit.
    pub(super) fn require_fresh_main(&mut self) {
        self.require_fresh_main = true;
    }

    /// Runs the existing Cargo owner once and retains typed admission failures.
    pub(super) fn capture(
        &mut self,
        command: &mut Command,
        control: ProcessControl<'_>,
        launched: &mut dyn FnMut(u32) -> Result<(), String>,
        mut admitted: impl FnMut(&DeclaredHarness) -> Result<(), PhaseFailure>,
    ) -> Result<(), PhaseFailure> {
        if self.failed || self.capture_started || self.bytes != 0 || self.build_finished {
            self.failed = true;
            return Err(failure(
                "Cargo artifact capture is repeated or already observed",
            ));
        }
        self.capture_started = true;
        let mut admission_error = None;
        let captured =
            control.capture_stdout_observed(command, MAX_STREAM_BYTES, launched, |bytes| {
                self.observe(bytes, control, &mut admitted)
                    .map_err(|error| {
                        let detail = error.detail.clone();
                        admission_error = Some(error);
                        detail
                    })
            });
        let result = captured
            .map_err(|error| admission_error.unwrap_or_else(|| crate::process_failure(error)))
            .and_then(|output| output.outcome.map_err(crate::process_failure));
        if result.is_err() {
            self.failed = true;
        }
        result
    }

    /// Accepts arbitrary byte chunks, notifying only after a complete declaration is admitted.
    /// Notifications are provisional: Cargo exit and stream closeout must still succeed.
    pub(super) fn observe(
        &mut self,
        bytes: &[u8],
        control: ProcessControl<'_>,
        mut admitted: impl FnMut(&DeclaredHarness) -> Result<(), PhaseFailure>,
    ) -> Result<(), PhaseFailure> {
        if self.failed {
            return Err(failure("Cargo artifact stream already failed"));
        }
        // Poison before callbacks too: catching an unwind must not revive this stream.
        self.failed = true;
        let result = self.observe_chunk(bytes, control, &mut admitted);
        self.failed = result.is_err();
        result
    }

    fn observe_chunk(
        &mut self,
        bytes: &[u8],
        control: ProcessControl<'_>,
        admitted: &mut dyn FnMut(&DeclaredHarness) -> Result<(), PhaseFailure>,
    ) -> Result<(), PhaseFailure> {
        if bytes.len() > MAX_STREAM_BYTES.saturating_sub(self.bytes) {
            return Err(failure("Cargo artifact stream exceeds its byte budget"));
        }
        self.bytes += bytes.len();
        let started = Instant::now();
        for fragment in bytes.split_inclusive(|byte| *byte == b'\n') {
            control.check(started).map_err(crate::process_failure)?;
            let complete = fragment.last() == Some(&b'\n');
            let content = if complete {
                &fragment[..fragment.len() - 1]
            } else {
                fragment
            };
            if content.len() > MAX_RECORD_BYTES.saturating_sub(self.pending.len()) {
                return Err(failure("Cargo artifact record exceeds its byte budget"));
            }
            self.pending.extend_from_slice(content);
            if complete {
                let record = std::mem::take(&mut self.pending);
                self.record(&record, control, admitted)?;
            }
        }
        Ok(())
    }

    fn record(
        &mut self,
        bytes: &[u8],
        control: ProcessControl<'_>,
        admitted: &mut dyn FnMut(&DeclaredHarness) -> Result<(), PhaseFailure>,
    ) -> Result<(), PhaseFailure> {
        if self.build_finished {
            return Err(failure("Cargo emitted a record after build completion"));
        }
        let message: Value = serde_json::from_slice(bytes).map_err(failure)?;
        match message["reason"].as_str() {
            Some("build-finished") => {
                if message["success"] != true {
                    return Err(failure("Cargo did not report a successful build"));
                }
                self.build_finished = true;
            }
            Some("compiler-message" | "build-script-executed") => {}
            Some("compiler-artifact") => {
                if !(self.selected)(&message) {
                    return Ok(());
                }
                if message["profile"]["test"] != true {
                    return Err(failure("selected Cargo artifact is not a test target"));
                }
                let package = message["package_id"]
                    .as_str()
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| failure("Cargo artifact has no package identity"))?;
                let target = serde_json::json!([
                    package,
                    message["target"]["name"],
                    message["target"]["kind"],
                ])
                .to_string();
                if self.harnesses.len() >= MAX_HARNESSES || self.targets.contains(&target) {
                    return Err(failure(
                        "duplicate Cargo test target or artifact count budget exceeded",
                    ));
                }
                let harness = self.manifests.admit(&message, control)?;
                let declaration = harness.json();
                if self.require_fresh_main
                    && declaration["package"] == "terlan"
                    && declaration["kind"] == "lib"
                    && message["fresh"] != true
                {
                    return Err(failure(
                        "workspace execution rebuilt the already-owned main library unit",
                    ));
                }
                let executable = std::fs::canonicalize(&harness.executable).map_err(failure)?;
                if !self.executables.insert(executable) {
                    return Err(failure("Cargo test targets share an executable"));
                }
                self.targets.insert(target);
                admitted(&harness)?;
                self.harnesses.push(harness);
            }
            _ => return Err(failure("unrecognized Cargo JSON record")),
        }
        Ok(())
    }

    /// Requires complete framing and successful Cargo build termination; not a test receipt.
    pub(super) fn finish(self) -> Result<Vec<DeclaredHarness>, PhaseFailure> {
        if self.failed || !self.pending.is_empty() || !self.build_finished {
            return Err(failure(
                "Cargo artifact stream is failed, truncated, or incomplete",
            ));
        }
        Ok(self.harnesses)
    }

    /// Closes the stream and requires the actual Terlan package declaration.
    pub(super) fn finish_terlan_library(self) -> Result<DeclaredHarness, PhaseFailure> {
        let mut harnesses = self.finish()?;
        if harnesses.len() != 1 || harnesses[0].json()["package"] != "terlan" {
            return Err(PhaseFailure {
                outcome: "artifact-ambiguous",
                detail: "Cargo must produce exactly one declared Terlan library harness".into(),
            });
        }
        Ok(harnesses.remove(0))
    }
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "cargo-artifact-stream-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "cargo_artifact_stream_test.rs"]
mod tests;
