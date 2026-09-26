//! Validate selections against libtest without executing tests or rebuilding.

use std::collections::{BTreeMap, BTreeSet};

use super::*;
use crate::phase_plan::terlan_library_phase;

/// Complete compiled inventory and its already-validated ownership selections.
#[derive(Debug)]
pub(super) struct TestPlan {
    /// Every compiled main-harness name, including externally owned tests.
    pub(super) all: BTreeSet<String>,
    /// Original libtest ignore classification, independent of selected execution phases.
    pub(super) ignored: BTreeSet<String>,
    /// Exact per-owner selections; external coverage may own a phase absent from this run.
    pub(super) selections: BTreeMap<&'static str, crate::test_execution::ExpectedTests>,
}

/// Lists the compiled harness once per distinct all/ignored inventory query.
///
/// These are observed child launches, not test executions or reusable receipts.
pub(super) fn prepare(
    phases: &[TestPhase],
    coverage_owns_terlc: bool,
    control: ProcessControl<'_>,
    executables: &crate::executable_binding::ExecutableBinding,
    environment: &crate::execution_environment::ExecutionEnvironment,
    ledger: &mut launch_ledger::LaunchLedger,
) -> Result<TestPlan, PhaseFailure> {
    let all = ledger.execute(
        "Terlan compiled test inventory",
        ValidationTier::FastUnit,
        "direct-terlan-inventory",
        |launched| {
            let harness = executables.verify_harness(control)?;
            list(&harness, false, environment, control, launched)
        },
    )?;
    ledger.execute(
        "Terlan compiled ignored-test inventory",
        ValidationTier::FastUnit,
        "direct-terlan-inventory",
        |launched| {
            let harness = executables.verify_harness(control)?;
            let ignored = list(&harness, true, environment, control, launched)?;
            let expected = validate(&all, &ignored, phases, coverage_owns_terlc, TIER_INVENTORY)?;
            println!(
                "[rust-test-suite] validated {} compiled tests ({} explicitly ignored)",
                all.len(),
                ignored.len()
            );
            Ok(TestPlan {
                all: all.clone(),
                ignored,
                selections: expected,
            })
        },
    )
}

fn list(
    harness: &Path,
    ignored: bool,
    environment: &crate::execution_environment::ExecutionEnvironment,
    control: ProcessControl<'_>,
    launched: &mut dyn FnMut(u32) -> Result<(), String>,
) -> Result<BTreeSet<String>, PhaseFailure> {
    let mut command = environment.test_command(harness);
    command.args(["--list", "--format", "terse"]);
    if ignored {
        command.arg("--ignored");
    }
    let output = control
        .capture_stdout(&mut command, 16 * 1024 * 1024, launched)
        .map_err(process_failure)?;
    parse(&output)
}

/// Parses libtest's terse inventory without running any test bodies.
pub(super) fn parse(output: &[u8]) -> Result<BTreeSet<String>, PhaseFailure> {
    let output = std::str::from_utf8(output).map_err(failure)?;
    let mut names = BTreeSet::new();
    for line in output.lines() {
        let name = line
            .strip_suffix(": test")
            .filter(|name| !name.is_empty() && !name.chars().any(char::is_whitespace))
            .ok_or_else(|| failure(format!("invalid libtest inventory line: {line:?}")))?;
        if !names.insert(name.to_owned()) {
            return Err(failure(format!("duplicate compiled test: {name}")));
        }
    }
    Ok(names)
}

fn validate(
    all: &BTreeSet<String>,
    ignored: &BTreeSet<String>,
    phases: &[TestPhase],
    coverage_owns_terlc: bool,
    tiers: &str,
) -> Result<BTreeMap<&'static str, crate::test_execution::ExpectedTests>, PhaseFailure> {
    if all.is_empty() || !ignored.is_subset(all) {
        return Err(failure("empty or inconsistent compiled test inventory"));
    }
    let normal = all.difference(ignored).cloned().collect::<BTreeSet<_>>();
    let mut owners = BTreeMap::new();
    let mut expected = BTreeMap::new();
    let coverage_phase = terlan_library_phase();
    let coverage = coverage_owns_terlc.then_some(&coverage_phase);
    for phase in phases.iter().chain(coverage) {
        if phase.executor != PhaseExecutor::TerlanHarness {
            continue;
        }
        let source = if phase.args.contains(&"--ignored") {
            ignored
        } else {
            &normal
        };
        let selected = select(source, &phase.args)?;
        if selected.is_empty() {
            return Err(failure(format!(
                "phase `{}` selects zero runnable tests",
                phase.name
            )));
        }
        let selected_ignored = if phase.args.contains(&"--ignored") {
            Vec::new()
        } else {
            select(ignored, &phase.args)?
        };
        let total = selected.len() + selected_ignored.len();
        expected.insert(
            phase.name,
            crate::test_execution::ExpectedTests {
                passed: selected.iter().map(|name| (*name).clone()).collect(),
                ignored: selected_ignored.into_iter().cloned().collect(),
                filtered: all.len() - total,
            },
        );
        for name in selected {
            if let Some((previous, _)) = owners.insert(name, (phase.name, phase.tier)) {
                return Err(failure(format!(
                    "test `{name}` has duplicate owners `{previous}` and `{}`",
                    phase.name
                )));
            }
        }
    }
    for name in &normal {
        if !owners.contains_key(name) {
            return Err(failure(format!(
                "compiled test `{name}` has no phase owner"
            )));
        }
    }
    for name in ignored {
        let rows = tiers
            .lines()
            .skip(1)
            .map(|line| line.split('\t').collect::<Vec<_>>())
            .filter(|row| row.first() == Some(&name.as_str()))
            .collect::<Vec<_>>();
        let [row] = rows.as_slice() else {
            return Err(failure(format!(
                "ignored test `{name}` requires exactly one tier row"
            )));
        };
        if row.len() != 4 || (row[2] == "terlan-test-orchestrator") != owners.contains_key(name) {
            return Err(failure(format!(
                "ignored test `{name}` has inconsistent tier ownership"
            )));
        }
        if owners
            .get(name)
            .is_some_and(|(_, tier)| tier.as_str() != row[1])
        {
            return Err(failure(format!(
                "ignored test `{name}` has inconsistent tier classification"
            )));
        }
    }
    Ok(expected)
}

/// Shares libtest selection semantics between execution planning and coverage inspection.
pub(super) fn select<'a>(
    names: &'a BTreeSet<String>,
    args: &[&str],
) -> Result<Vec<&'a String>, PhaseFailure> {
    let mut filters = Vec::new();
    let mut skipped = Vec::new();
    let mut exact = false;
    let mut args = args.iter();
    while let Some(argument) = args.next() {
        match *argument {
            "--ignored" => {}
            "--exact" => exact = true,
            "--skip" => skipped.push(
                *args
                    .next()
                    .ok_or_else(|| failure("missing skip selector"))?,
            ),
            value if value.starts_with('-') => {
                return Err(failure(format!("unsupported selection argument: {value}")))
            }
            value => filters.push(value),
        }
    }
    let matches = |name: &str, filter: &str| {
        if exact {
            name == filter
        } else {
            name.contains(filter)
        }
    };
    Ok(names
        .iter()
        .filter(|name| {
            (filters.is_empty() || filters.iter().any(|filter| matches(name, filter)))
                && !skipped.iter().any(|filter| matches(name, filter))
        })
        .collect())
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "test-inventory-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "test_inventory_test.rs"]
mod tests;
