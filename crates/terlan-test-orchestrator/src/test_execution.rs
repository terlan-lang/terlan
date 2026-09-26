//! Verify libtest's private result channel independently of nested child stdout.

use crate::execution_environment::ExecutionEnvironment;
use crate::{process_failure, PhaseFailure, TestPhase};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use terlan_process_owner::ProcessControl;

/// Exact runnable and ignored names selected from a byte-admitted harness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ExpectedTests {
    /// Tests which must execute and pass, not simply disappear on exit zero.
    pub(super) passed: BTreeSet<String>,
    /// Selected ignores deliberately owned by other phases or external tiers.
    pub(super) ignored: BTreeSet<String>,
    /// Compiled names excluded by the selectors.
    pub(super) filtered: usize,
}

impl ExpectedTests {
    /// Restores a complete native target's bounded, duplicate-free runnable inventory.
    pub(super) fn native_names(value: &serde_json::Value) -> Result<Self, PhaseFailure> {
        let values = value
            .as_array()
            .ok_or_else(|| failure("missing native test names"))?;
        let mut passed = BTreeSet::new();
        let mut bytes = 0_usize;
        for value in values {
            let name = value
                .as_str()
                .ok_or_else(|| failure("native test name is not a string"))?;
            bytes = bytes.saturating_add(name.len());
            if name.is_empty()
                || name.chars().any(char::is_whitespace)
                || passed.len() >= 100_000
                || bytes > 1024 * 1024
                || !passed.insert(name.to_owned())
            {
                return Err(failure("invalid, duplicate or excessive native test names"));
            }
        }
        Ok(Self {
            passed,
            ignored: BTreeSet::new(),
            filtered: 0,
        })
    }

    /// Shared identity for admitted names and independently verified terminal records.
    pub(super) fn identity(&self) -> String {
        let mut selection = Sha256::new();
        for (status, names) in [
            (b"passed".as_slice(), &self.passed),
            (b"ignored".as_slice(), &self.ignored),
        ] {
            crate::file_identity::field(&mut selection, status);
            for name in names {
                crate::file_identity::field(&mut selection, name.as_bytes());
            }
        }
        crate::file_identity::hex(selection)
    }
}

/// Verified main-harness records or explicitly weaker Cargo stdout observations.
#[derive(Debug)]
pub(super) struct TestEvidence(pub(super) serde_json::Value);

impl TestEvidence {
    /// Does not present Cargo summary presence as a complete per-harness inventory.
    pub(super) fn json(&self) -> serde_json::Value {
        self.0.clone()
    }
}

/// Executes once, keeping main-harness output live and its result log private.
pub(super) fn run(
    phase: &TestPhase,
    executable: &Path,
    environment: &ExecutionEnvironment,
    threads: usize,
    expected: Option<ExpectedTests>,
    control: ProcessControl<'_>,
    launched: &mut dyn FnMut(u32) -> Result<(), String>,
) -> Result<TestEvidence, PhaseFailure> {
    if phase.executor == crate::PhaseExecutor::CargoNative {
        return Err(failure(
            "native Cargo tests require the workspace runner owner",
        ));
    }
    if (phase.executor == crate::PhaseExecutor::TerlanHarness) != expected.is_some() {
        return Err(failure(
            "test phase does not have its required inventory scope",
        ));
    }
    let mut command = environment.test_command(executable);
    let doctests = phase.args.contains(&"--doc");
    command.args(&phase.args).args([
        "--test-threads",
        &threads.to_string(),
        "--format",
        if doctests { "pretty" } else { "terse" },
        "--color",
        "never",
    ]);
    command.envs(phase.environment.iter().map(|(name, value)| (name, value)));
    if let Some(expected) = expected {
        let log = crate::test_result_log::TestResultLog::create(environment)?;
        command.arg("--logfile").arg(log.path());
        control
            .run(&mut command, launched)
            .map_err(process_failure)?;
        let (bytes, identity) = log.read(control)?;
        let evidence = inspect_log(&bytes, &expected, &identity)?;
        log.close()?;
        Ok(evidence)
    } else {
        let captured = control
            .capture_stdout_observed(&mut command, 16 * 1024 * 1024, launched, |bytes| {
                let mut output = std::io::stdout().lock();
                output
                    .write_all(bytes)
                    .and_then(|()| output.flush())
                    .map_err(|error| error.to_string())
            })
            .map_err(process_failure)?;
        captured.outcome.map_err(process_failure)?;
        if doctests {
            crate::doctest_output::inspect(&captured.stdout)
        } else {
            inspect_cargo(&captured.stdout)
        }
    }
}

fn inspect_log(
    output: &[u8],
    expected: &ExpectedTests,
    identity: &str,
) -> Result<TestEvidence, PhaseFailure> {
    if expected.passed.is_empty() {
        return Err(failure("Terlan test phase has no runnable tests"));
    }
    inspect_workspace_log(output, expected, identity)
}

/// Exact records for a declared native target, including legitimately empty harnesses.
pub(super) fn inspect_workspace_log(
    output: &[u8],
    expected: &ExpectedTests,
    identity: &str,
) -> Result<TestEvidence, PhaseFailure> {
    let text = std::str::from_utf8(output).map_err(failure)?;
    let mut passed = BTreeSet::new();
    let mut ignored = BTreeSet::new();
    let mut ignore_message = false;
    for line in text.lines() {
        let (name, is_ignored) = if ignore_message || line.starts_with("ignored: ") {
            let candidate = line.rsplit_once(' ').map(|(_, name)| name).unwrap_or(line);
            if !expected.ignored.contains(candidate) {
                ignore_message = true;
                continue;
            }
            ignore_message = false;
            (candidate, true)
        } else if let Some(name) = line.strip_prefix("ignored ") {
            (name, true)
        } else if let Some(name) = line.strip_prefix("ok ") {
            (name, false)
        } else {
            return Err(failure(
                "libtest result log contains a failed or malformed record",
            ));
        };
        let (expected, actual) = if is_ignored {
            (&expected.ignored, &mut ignored)
        } else {
            (&expected.passed, &mut passed)
        };
        if !expected.contains(name) || !actual.insert(name.to_owned()) {
            return Err(failure(
                "libtest result has an unexpected, duplicate, or wrongly ignored name",
            ));
        }
    }
    if ignore_message || passed != expected.passed || ignored != expected.ignored {
        return Err(failure(
            "libtest did not complete every admitted runnable and ignored test",
        ));
    }
    Ok(TestEvidence(
        serde_json::json!({"scope": "admitted-libtest-records-v1", "passed": passed.len(), "ignored": ignored.len(), "filtered": expected.filtered,
        "selection_identity_sha256": expected.identity(), "result_log_bytes": output.len(), "result_log_identity_sha256": identity}),
    ))
}

fn inspect_cargo(output: &[u8]) -> Result<TestEvidence, PhaseFailure> {
    let text = std::str::from_utf8(output).map_err(failure)?;
    let summaries = text
        .lines()
        .filter(|line| line.starts_with("test result: "))
        .collect::<Vec<_>>();
    let last = summaries
        .last()
        .ok_or_else(|| failure("Cargo exited without any test summary"))?;
    let parse = |line: &str| -> Option<(usize, usize)> {
        let body = line.strip_prefix("test result: ok. ")?;
        let (passed, body) = body.split_once(" passed; ")?;
        let (failed, body) = body.split_once(" failed; ")?;
        let mut parts = body.split("; ");
        for suffix in [" ignored", " measured", " filtered out"] {
            parts.next()?.strip_suffix(suffix)?.parse::<usize>().ok()?;
        }
        let duration = parts
            .next()?
            .strip_prefix("finished in ")?
            .strip_suffix('s')?
            .parse::<f64>()
            .ok()?;
        if !duration.is_finite() || duration < 0.0 || parts.next().is_some() {
            return None;
        }
        Some((passed.parse().ok()?, failed.parse().ok()?))
    };
    if parse(last).is_none_or(|(_, failed)| failed != 0)
        || !summaries
            .iter()
            .any(|line| parse(line).is_some_and(|(passed, failed)| passed > 0 && failed == 0))
    {
        return Err(failure(
            "Cargo has no passing nonempty test observation or its last summary failed",
        ));
    }
    Ok(TestEvidence(
        serde_json::json!({"scope": "cargo-summary-presence-v1", "last_summary": last,
        "stdout_bytes": output.len(), "stdout_sha256": crate::file_identity::hex(Sha256::new_with_prefix(output))}),
    ))
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "test-completion-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "test_execution_test.rs"]
mod tests;
