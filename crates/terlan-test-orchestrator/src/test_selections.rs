//! Bounded exact-name evidence for later gate coverage, not a reusable test receipt.

use crate::file_identity::{hash_file_contents, hex};
use crate::report_file::ReportFile;
use crate::test_execution::ExpectedTests;
use crate::{PhaseExecutor, PhaseFailure, PhaseResult, TestPhase};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::Instant;
use terlan_process_owner::ProcessControl;

const MAX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_NAMES: usize = 100_000;

mod reader;
pub(super) use reader::document as read_document;
pub(super) use reader::inspect_main;
pub(super) use reader::read_completed;

/// One immutable selection document, tied to the current suite and compiled harness.
pub(super) struct TestSelections {
    path: PathBuf,
    digest: String,
    run_id: String,
    selections: BTreeMap<String, ExpectedTests>,
    compiled: BTreeSet<String>,
    original_ignored: BTreeSet<String>,
    covered_names: BTreeSet<String>,
    coverage: Value,
    verified: bool,
}

impl TestSelections {
    /// Publishes already-discovered selections without executing or listing tests again.
    pub(super) fn create(
        path: &Path,
        run_id: &str,
        phases: &[TestPhase],
        plan: &crate::test_inventory::TestPlan,
        harness: &Value,
        control: ProcessControl<'_>,
    ) -> Result<Self, PhaseFailure> {
        let started = Instant::now();
        let mut selections = BTreeMap::new();
        let mut names = BTreeSet::new();
        let mut name_bytes = 0_usize;
        let mut name_count = 0_usize;
        if plan.all.is_empty() || !plan.ignored.is_subset(&plan.all) {
            return Err(failure(
                "missing or inconsistent complete compiled inventory",
            ));
        }
        for name in plan.all.iter().chain(&plan.ignored) {
            name_count += 1;
            name_bytes = name_bytes.saturating_add(name.len());
            if name.is_empty()
                || name.chars().any(char::is_whitespace)
                || name_count > MAX_NAMES
                || name_bytes > MAX_BYTES as usize / 2
            {
                return Err(failure("invalid or excessive compiled inventory"));
            }
        }
        for phase in phases
            .iter()
            .filter(|phase| phase.executor == PhaseExecutor::TerlanHarness)
        {
            control.check(started).map_err(crate::process_failure)?;
            let selected = plan
                .selections
                .get(phase.name)
                .ok_or_else(|| failure("missing admitted phase selection"))?;
            if selected.passed.is_empty() || !selected.passed.is_disjoint(&selected.ignored) {
                return Err(failure("empty or inconsistent phase selection"));
            }
            if !selected.passed.is_subset(&plan.all)
                || !selected.ignored.is_subset(&plan.ignored)
                || selected
                    .passed
                    .len()
                    .checked_add(selected.ignored.len())
                    .and_then(|count| count.checked_add(selected.filtered))
                    != Some(plan.all.len())
                || (phase.args.contains(&"--ignored") && !selected.passed.is_subset(&plan.ignored))
                || (phase.args.contains(&"--ignored") && !selected.ignored.is_empty())
                || (!phase.args.contains(&"--ignored")
                    && !selected.passed.is_disjoint(&plan.ignored))
            {
                return Err(failure(
                    "phase selection differs from the complete compiled inventory",
                ));
            }
            for name in selected.passed.iter().chain(&selected.ignored) {
                name_count += 1;
                name_bytes = name_bytes
                    .checked_add(name.len())
                    .ok_or_else(|| failure("selection name budget overflow"))?;
                if name.is_empty()
                    || name.chars().any(char::is_whitespace)
                    || name_bytes > MAX_BYTES as usize / 2
                    || name_count > MAX_NAMES
                {
                    return Err(failure("invalid or excessive selection name bytes"));
                }
            }
            for name in &selected.passed {
                if names.len() == MAX_NAMES || !names.insert(name.clone()) {
                    return Err(failure(
                        "duplicate test owner or excessive selection inventory",
                    ));
                }
            }
            if selections.len() == 256
                || selections
                    .insert(phase.name.to_owned(), selected.clone())
                    .is_some()
            {
                return Err(failure("duplicate or excessive selection phases"));
            }
        }
        if selections.is_empty()
            || run_id.is_empty()
            || harness["role"] != "terlan-library-harness"
            || harness["identity_sha256"].as_str().is_none_or(|hash| {
                hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        {
            return Err(failure(
                "selection inventory lacks its suite or compiled harness binding",
            ));
        }
        let rows = selections
            .iter()
            .map(|(name, selected)| {
                json!({"phase":name,
            "passed":selected.passed, "ignored":selected.ignored, "filtered":selected.filtered,
            "selection_identity_sha256":selected.identity()})
            })
            .collect::<Vec<_>>();
        let document = json!({"schema":"terlan.rust-test-selections.v2", "suite_run_id":run_id,
            "scope":"admitted-main-harness-selections-v1", "reusable":false,
            "package":"terlan", "target":"lib", "features":crate::VALIDATION_FEATURES,
            "harness":harness, "compiled": {"all":plan.all, "ignored":plan.ignored}, "phases":rows});
        let bytes = serde_json::to_vec(&document).map_err(failure)?;
        let mut owner = ReportFile::open_bounded(path, MAX_BYTES).map_err(failure)?;
        owner.publish(&bytes).map_err(failure)?;
        Ok(Self {
            path: path.to_owned(),
            digest: hex(Sha256::new_with_prefix(&bytes)),
            run_id: run_id.to_owned(),
            selections,
            compiled: plan.all.clone(),
            original_ignored: plan.ignored.clone(),
            covered_names: names,
            coverage: Value::Null,
            verified: false,
        })
    }

    /// Requires every admitted phase to complete with the exact independently verified selection.
    pub(super) fn verify(
        &mut self,
        results: &[PhaseResult],
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        if self.verified {
            return Err(failure("test selection closeout cannot repeat"));
        }
        let direct = results
            .iter()
            .filter(|result| result.executor == "direct-terlan-harness")
            .collect::<Vec<_>>();
        if direct.len() != self.selections.len() {
            return Err(failure("missing or extra main-harness phase completions"));
        }
        let mut seen = BTreeSet::new();
        for result in direct {
            let selected = self
                .selections
                .get(result.name)
                .ok_or_else(|| failure("unadmitted test phase completed"))?;
            let evidence = result
                .test_execution
                .as_ref()
                .ok_or_else(|| failure("test phase has no completion evidence"))?;
            if !seen.insert(result.name) {
                return Err(failure("test completion does not match admitted selection"));
            }
            validate_completion(result.outcome, result.child_pid, evidence, selected)?;
        }
        let mut digest = Sha256::new();
        hash_file_contents(&self.path, &mut digest, MAX_BYTES, control, Instant::now())?;
        if hex(digest) != self.digest {
            return Err(failure("test selection document changed during execution"));
        }
        self.verified = true;
        self.coverage = json!({"normal":self.inspect_coverage(&[])?, "ignored":self.inspect_coverage(&["--ignored"])?});
        Ok(())
    }

    /// Compact companion binding keeps exact names outside the live-report size budget.
    pub(super) fn json(&self) -> Value {
        json!({"scope":"admitted-main-harness-selections-v1", "path":self.path,
            "suite_run_id":self.run_id, "sha256":self.digest, "phases":self.selections.len(),
            "tests":self.selections.values().map(|selection| selection.passed.len()).sum::<usize>(),
            "compiled_tests":self.compiled.len(), "originally_ignored":self.original_ignored.len(),
            "coverage":self.coverage, "verified":self.verified, "reusable":false})
    }

    /// Inspects historical name coverage only; it never authorizes current-input reuse.
    pub(super) fn inspect_coverage(&self, selectors: &[&str]) -> Result<Value, PhaseFailure> {
        if !self.verified {
            return Err(failure("coverage inspection requires completed selections"));
        }
        let names = if selectors.contains(&"--ignored") {
            self.original_ignored.clone()
        } else {
            self.compiled
                .difference(&self.original_ignored)
                .cloned()
                .collect()
        };
        let selected = crate::test_inventory::select(&names, selectors)?;
        let passed = selected
            .iter()
            .filter(|name| self.covered_names.contains(**name))
            .count();
        Ok(
            json!({"scope":"completed-main-harness-name-coverage-v1", "selected":selected.len(), "passed":passed,
            "uncovered":selected.len()-passed, "covered":!selected.is_empty() && selected.len() == passed, "reusable":false}),
        )
    }

    /// Requires every selected test's recorded phase to match the caller's effective inputs.
    pub(super) fn inspect_request(
        &self,
        selectors: &[&str],
        environment: &str,
        phases: &BTreeMap<String, String>,
    ) -> Result<Value, PhaseFailure> {
        let coverage = self.inspect_coverage(selectors)?;
        if coverage["covered"] != true {
            return Err(failure("requested tests are absent or not covered"));
        }
        let names = if selectors.contains(&"--ignored") {
            self.original_ignored.clone()
        } else {
            self.compiled
                .difference(&self.original_ignored)
                .cloned()
                .collect()
        };
        let selected = crate::test_inventory::select(&names, selectors)?;
        for (phase, tests) in &self.selections {
            if selected.iter().any(|name| tests.passed.contains(*name))
                && phases.get(phase).map(String::as_str) != Some(environment)
            {
                return Err(failure(format!(
                    "request environment differs from completed phase {phase}"
                )));
            }
        }
        Ok(coverage)
    }
}

fn validate_completion(
    outcome: &str,
    pid: Option<u32>,
    evidence: &Value,
    selected: &ExpectedTests,
) -> Result<(), PhaseFailure> {
    if outcome != "passed"
        || pid.is_none_or(|pid| pid == 0)
        || evidence["scope"] != "admitted-libtest-records-v1"
        || evidence["selection_identity_sha256"] != selected.identity()
        || evidence["passed"] != selected.passed.len()
        || evidence["ignored"] != selected.ignored.len()
        || evidence["filtered"] != selected.filtered
    {
        return Err(failure("test completion does not match admitted selection"));
    }
    Ok(())
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "test-selections-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "test_selections_test.rs"]
mod tests;
