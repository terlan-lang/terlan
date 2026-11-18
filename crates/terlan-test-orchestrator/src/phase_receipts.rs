//! Exact-input receipts for completed main-harness phases.

use crate::execution_environment::ExecutionEnvironment;
use crate::test_execution::{ExpectedTests, TestEvidence};
use crate::{file_identity, report_file::ReportFile, PhaseFailure, TestPhase};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;
use terlan_process_owner::ProcessControl;

const DIRECTORY_ENV: &str = "TERLAN_RUST_PHASE_RECEIPT_DIR";
const FORCE_ENV: &str = "TERLAN_TEST_FORCE_RERUN";
const SCHEMA: &str = "terlan.rust-phase-receipt.v1";
const MAX_BYTES: u64 = 64 * 1024;

/// Retains new completions in memory until suite inputs close unchanged.
pub(super) struct Store {
    root: Option<PathBuf>,
    input_identity: String,
    force: bool,
    pending: BTreeMap<String, serde_json::Value>,
}

impl Store {
    pub(super) fn new(
        environment: &ExecutionEnvironment,
        input_identity: String,
    ) -> Result<Self, PhaseFailure> {
        let root = environment
            .value(DIRECTORY_ENV)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from);
        if let Some(root) = &root {
            validate_root(root, true)?;
        }
        Ok(Self {
            root,
            input_identity,
            force: environment.value(FORCE_ENV).as_deref() == Some("1".as_ref()),
            pending: BTreeMap::new(),
        })
    }

    pub(super) fn restore(
        &self,
        phase: &TestPhase,
        threads: usize,
        expected: &ExpectedTests,
        control: ProcessControl<'_>,
    ) -> Result<Option<TestEvidence>, PhaseFailure> {
        let Some(root) = &self.root else {
            return Ok(None);
        };
        if self.force {
            return Ok(None);
        }
        validate_root(root, false)?;
        let key = self.key(phase, threads, expected)?;
        let path = root.join(format!("{key}.json"));
        let metadata = match std::fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(failure(error)),
        };
        if !metadata.is_file() || metadata.len() > MAX_BYTES {
            return Err(failure("phase receipt is not a bounded regular file"));
        }
        let bytes = file_identity::read_hashed_file(
            &path,
            &mut Sha256::new(),
            MAX_BYTES,
            control,
            Instant::now(),
        )?;
        let document: serde_json::Value = serde_json::from_slice(&bytes).map_err(failure)?;
        let evidence = &document["evidence"];
        if document.as_object().is_none_or(|fields| fields.len() != 7)
            || document["schema"] != SCHEMA
            || document["key"] != key
            || document["phase"] != phase.name
            || document["input_identity_sha256"] != self.input_identity
            || document["test_threads"] != threads
            || document["selection_identity_sha256"] != expected.identity()
            || !expected.validate_evidence(evidence)
        {
            return Err(failure("phase receipt does not match admitted inputs"));
        }
        Ok(Some(TestEvidence(evidence.clone())))
    }

    pub(super) fn remember(
        &mut self,
        phase: &TestPhase,
        threads: usize,
        expected: &ExpectedTests,
        evidence: &TestEvidence,
    ) -> Result<(), PhaseFailure> {
        if self.root.is_none() {
            return Ok(());
        }
        if !expected.validate_evidence(&evidence.0) {
            return Err(failure("cannot retain incomplete phase evidence"));
        }
        let key = self.key(phase, threads, expected)?;
        let document = serde_json::json!({
            "schema":SCHEMA, "key":key, "phase":phase.name,
            "input_identity_sha256":self.input_identity, "test_threads":threads,
            "selection_identity_sha256":expected.identity(), "evidence":evidence.0,
        });
        if self.pending.insert(key, document).is_some() {
            return Err(failure("duplicate phase receipt completion"));
        }
        Ok(())
    }

    /// Publishes only after normal or failure closeout verifies stable inputs.
    pub(super) fn seal(&mut self) -> Result<(), PhaseFailure> {
        let Some(root) = &self.root else {
            self.pending.clear();
            return Ok(());
        };
        std::fs::create_dir_all(root).map_err(failure)?;
        validate_root(root, false)?;
        for (key, document) in &self.pending {
            let bytes = serde_json::to_vec(document).map_err(failure)?;
            let mut report = ReportFile::open_bounded(&root.join(format!("{key}.json")), MAX_BYTES)
                .map_err(failure)?;
            report.publish(&bytes).map_err(failure)?;
        }
        self.pending.clear();
        Ok(())
    }

    fn key(
        &self,
        phase: &TestPhase,
        threads: usize,
        expected: &ExpectedTests,
    ) -> Result<String, PhaseFailure> {
        let semantic_environment = phase
            .environment
            .iter()
            .filter(|(key, _)| {
                !crate::execution_environment::is_validation_routing_env(key.as_ref())
            })
            .collect::<Vec<_>>();
        let descriptor = serde_json::json!({
            "schema":SCHEMA, "inputs":self.input_identity, "name":phase.name,
            "tier":phase.tier.as_str(), "executor":phase.executor.as_str(),
            "args":phase.args, "environment":semantic_environment,
            "shard":phase.shard.map(|shard| [shard.index, shard.total]),
            "test_threads":threads, "selection":expected.identity(),
        });
        let bytes = serde_json::to_vec(&descriptor).map_err(failure)?;
        Ok(file_identity::hex(Sha256::new_with_prefix(bytes)))
    }
}

fn validate_root(root: &Path, allow_absent: bool) -> Result<(), PhaseFailure> {
    match std::fs::symlink_metadata(root) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => Ok(()),
        Ok(_) => Err(failure("phase receipt root is not a real directory")),
        Err(error) if allow_absent && error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(failure(error)),
    }
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "phase-receipt-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "phase_receipts_test.rs"]
mod tests;
