//! Reconcile normal Rustdoc pretty output without rerunning or merging doctests.
//!
//! These observations cover emitted harnesses, not an independently admitted
//! package inventory. They must not be used as reusable test receipts.

use crate::test_execution::TestEvidence;
use crate::PhaseFailure;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Bound the number of independently summarized Rustdoc harnesses per phase.
const MAX_HARNESSES: usize = 1_024;
/// Bound retained names across all emitted harnesses in the captured phase.
const MAX_TESTS: usize = 100_000;

/// One Rustdoc-emitted harness; names include location and compilation/panic mode.
#[derive(Default)]
struct Harness {
    /// Count declared before the harness starts executing its selected tests.
    declared: usize,
    /// Individual terminal records, kept distinct even when names contain spaces.
    records: BTreeMap<String, bool>,
    /// Serial pretty output prints the name before a slow-test warning arrives.
    pending: Option<String>,
    /// Timeout warnings are diagnostic, but must refer to tests which complete.
    warnings: BTreeSet<String>,
}

/// Requires complete, internally consistent records from every observed harness.
pub(super) fn inspect(output: &[u8]) -> Result<TestEvidence, PhaseFailure> {
    let text = std::str::from_utf8(output).map_err(failure)?;
    if !text.ends_with('\n') {
        return Err(failure("truncated doctest output"));
    }
    let mut active: Option<Harness> = None;
    let mut harnesses = Vec::new();
    let mut total = 0usize;
    for line in text.lines().filter(|line| !line.is_empty()) {
        if let Some(harness) = active.as_mut().filter(|harness| harness.pending.is_some()) {
            let name = harness.pending.take().expect("checked pending name");
            harness.record(&format!("{name} ... {line}"))?;
        } else if let Some(count) = line.strip_prefix("running ") {
            if active.is_some() || harnesses.len() == MAX_HARNESSES {
                return Err(failure("overlapping or excessive doctest harnesses"));
            }
            let (count, noun) = count
                .split_once(' ')
                .ok_or_else(|| failure("malformed doctest start record"))?;
            let count = count.parse::<usize>().map_err(failure)?;
            if noun != if count == 1 { "test" } else { "tests" }
                || count > MAX_TESTS.saturating_sub(total)
            {
                return Err(failure("malformed or excessive doctest count"));
            }
            total += count;
            active = Some(Harness {
                declared: count,
                ..Default::default()
            });
        } else if let Some(summary) = line.strip_prefix("test result: ok. ") {
            let harness = active
                .take()
                .ok_or_else(|| failure("doctest summary has no preceding harness"))?;
            harnesses.push(harness.finish(summary)?);
        } else if let Some(record) = line.strip_prefix("test ") {
            active
                .as_mut()
                .ok_or_else(|| failure("doctest result has no preceding harness"))?
                .record(record)?;
        } else if let Some(times) = line.strip_prefix("all doctests ran in ") {
            let (elapsed, compilation) = times
                .split_once("; merged doctests compilation took ")
                .ok_or_else(|| failure("malformed merged-doctest timing"))?;
            duration(elapsed)?;
            duration(compilation)?;
            if active.is_some() || harnesses.is_empty() {
                return Err(failure("doctest timing precedes harness completion"));
            }
        } else {
            return Err(failure(format!(
                "unexpected doctest output: {}",
                line.chars().take(160).collect::<String>()
            )));
        }
    }
    if active.is_some() || harnesses.is_empty() {
        return Err(failure("doctest harness did not finish"));
    }
    let passed: usize = harnesses.iter().map(|value| value.0).sum();
    let ignored: usize = harnesses.iter().map(|value| value.1).sum();
    let records = harnesses
        .into_iter()
        .map(|(passed, ignored, identity)| {
            serde_json::json!({"passed":passed, "ignored":ignored, "records_identity_sha256":identity})
        })
        .collect::<Vec<_>>();
    Ok(TestEvidence(serde_json::json!({
        "scope":"rustdoc-emitted-harness-records-v1",
        "independent_inventory":false,
        "passed":passed, "ignored":ignored, "harnesses":records,
        "stdout_bytes":output.len(),
        "stdout_sha256":crate::file_identity::hex(Sha256::new_with_prefix(output))
    })))
}

impl Harness {
    /// Accepts one unique terminal record, without parsing the test name as words.
    fn record(&mut self, record: &str) -> Result<(), PhaseFailure> {
        if let Some((names, duration)) = record.rsplit_once(" has been running for over ") {
            return self.warning(names, duration);
        }
        let (name, status) = record
            .rsplit_once(" ... ")
            .ok_or_else(|| failure("unfinished doctest result"))?;
        let passed = match status {
            "ok" => true,
            "ignored" => false,
            _ => return Err(failure("failed or malformed doctest result")),
        };
        if name.is_empty()
            || name.chars().any(char::is_control)
            || self.records.len() >= self.declared
            || self.records.insert(name.to_owned(), passed).is_some()
        {
            return Err(failure("duplicate, excessive, or malformed doctest name"));
        }
        self.warnings.remove(bare_name(name));
        Ok(())
    }

    /// Keeps libtest's slow-test warning from turning an otherwise passing run red.
    fn warning(&mut self, names: &str, duration: &str) -> Result<(), PhaseFailure> {
        duration
            .strip_suffix(" seconds")
            .and_then(|value| value.parse::<u64>().ok())
            .filter(|seconds| *seconds > 0)
            .ok_or_else(|| failure("malformed doctest timeout warning"))?;
        let name = if let Some((display, name)) = names.split_once(" ... test ") {
            if bare_name(display) != name {
                return Err(failure("doctest timeout warning changes pending test"));
            }
            self.pending = Some(display.to_owned());
            name
        } else {
            names
        };
        if name.is_empty()
            || name.chars().any(char::is_control)
            || self.warnings.len() >= self.declared
            || !self.warnings.insert(name.to_owned())
        {
            return Err(failure("duplicate or malformed doctest timeout warning"));
        }
        Ok(())
    }

    /// Matches all summary counts to the individual records and declared count.
    fn finish(self, summary: &str) -> Result<(usize, usize, String), PhaseFailure> {
        let mut fields = summary.split("; ");
        let mut counts = Vec::new();
        for suffix in [
            " passed",
            " failed",
            " ignored",
            " measured",
            " filtered out",
        ] {
            let field = fields
                .next()
                .and_then(|field| field.strip_suffix(suffix))
                .ok_or_else(|| failure("malformed doctest summary"))?;
            counts.push(field.parse::<usize>().map_err(failure)?);
        }
        duration(
            fields
                .next()
                .and_then(|field| field.strip_prefix("finished in "))
                .ok_or_else(|| failure("missing doctest duration"))?,
        )?;
        let passed = self.records.values().filter(|passed| **passed).count();
        let ignored = self.records.len() - passed;
        if fields.next().is_some()
            || counts != [passed, 0, ignored, 0, 0]
            || self.records.len() != self.declared
            || self.pending.is_some()
            || !self.warnings.is_empty()
        {
            return Err(failure(
                "doctest results do not match declared or summary counts",
            ));
        }
        let mut hash = Sha256::new();
        for (name, passed) in self.records {
            crate::file_identity::field(&mut hash, name.as_bytes());
            crate::file_identity::field(&mut hash, if passed { b"ok" } else { b"ignored" });
        }
        Ok((passed, ignored, crate::file_identity::hex(hash)))
    }
}

/// Rust's warning uses the raw name while its result includes the execution mode.
fn bare_name(display: &str) -> &str {
    [" - compile fail", " - compile", " - should panic"]
        .into_iter()
        .find_map(|suffix| display.strip_suffix(suffix))
        .unwrap_or(display)
}

/// Rejects non-finite, negative, and malformed durations instead of trusting text.
fn duration(value: &str) -> Result<(), PhaseFailure> {
    let value = value
        .strip_suffix('s')
        .ok_or_else(|| failure("malformed doctest duration"))?
        .parse::<f64>()
        .map_err(failure)?;
    if !value.is_finite() || value < 0.0 {
        return Err(failure("invalid doctest duration"));
    }
    Ok(())
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "test-completion-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "doctest_output_test.rs"]
mod tests;
