//! One input-admission and closeout sequence for execution and receipt verification.

use crate::execution_environment::ExecutionEnvironment;
use crate::launch_ledger::LaunchLedger;
use crate::{source_inventory, PhaseFailure, TestPhase, ValidationTier};
use std::path::Path;
use terlan_process_owner::ProcessControl;

/// Observes current inputs before any Cargo build or libtest discovery.
pub(super) fn admit(
    ledger: &mut LaunchLedger,
    environment: &ExecutionEnvironment,
    phases: &[TestPhase],
    control: ProcessControl<'_>,
) -> Result<(), PhaseFailure> {
    ledger
        .bind_environment(environment, phases)
        .map_err(failure)?;
    ledger.admit_executables(environment, control)?;
    ledger.admit_configuration(environment, control)?;
    let executables = ledger.executables().clone();
    let source = ledger.execute(
        "working-tree source admission",
        ValidationTier::FastUnit,
        "git-source-inventory",
        |launched| {
            source_inventory::capture_with_git(
                Path::new("."),
                &executables.verify_program("git", control)?,
                environment,
                control,
                launched,
            )
        },
    )?;
    ledger.bind_source(source).map_err(failure)?;
    let toolchain = match crate::rust_toolchain::RustToolchain::admit(
        environment,
        &executables,
        ledger,
        control,
    ) {
        Ok(toolchain) => toolchain,
        Err(error) => {
            ledger.reject_toolchain().map_err(failure)?;
            return Err(error);
        }
    };
    ledger.bind_toolchain(toolchain).map_err(failure)?;
    ledger.admit_cargo_tools(environment, control)?;
    ledger.admit_compiler(control)?;
    ledger.admit_metadata(environment, control)
}

/// Closes exactly the same input owners for execution and read-only verification.
pub(super) fn close(
    ledger: &mut LaunchLedger,
    environment: &ExecutionEnvironment,
    control: ProcessControl<'_>,
) -> Result<(), PhaseFailure> {
    let executables = ledger.executables().clone();
    let source = ledger.execute(
        "working-tree source closeout",
        ValidationTier::FastUnit,
        "git-source-inventory",
        |launched| {
            source_inventory::capture_with_git(
                Path::new("."),
                &executables.verify_program("git", control)?,
                environment,
                control,
                launched,
            )
        },
    )?;
    ledger.verify_source(source).map_err(failure)?;
    ledger.verify_executables(control)?;
    ledger.verify_configuration(control)?;
    ledger.verify_cargo_tools(control)?;
    ledger.verify_toolchain(control)?;
    ledger.verify_compiler(control)?;
    ledger.verify_metadata(control)
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "suite-inputs-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "suite_inputs_test.rs"]
mod tests;

/// Matches every freshly closed input binding, never a handpicked source or commit field.
pub(super) fn compare(
    current: &serde_json::Value,
    previous: &serde_json::Value,
) -> Result<(), PhaseFailure> {
    let bindings = current
        .as_object()
        .filter(|rows| rows.len() == 8)
        .ok_or_else(|| failure("missing current input bindings"))?;
    for (name, current) in bindings {
        if current.is_null() || previous.get(name) != Some(current) {
            return Err(failure(format!(
                "completed suite has different current inputs: {name}"
            )));
        }
    }
    Ok(())
}

/// Compares admitted bytes before launching gates; only closeout observations are pending.
pub(super) fn compare_admission(
    current: &serde_json::Value,
    previous: &serde_json::Value,
) -> Result<(), PhaseFailure> {
    fn admission(value: &serde_json::Value) -> serde_json::Value {
        match value {
            serde_json::Value::Object(fields) => serde_json::Value::Object(
                fields
                    .iter()
                    .filter(|(key, _)| !matches!(key.as_str(), "after" | "verified"))
                    .map(|(key, value)| (key.clone(), admission(value)))
                    .collect(),
            ),
            serde_json::Value::Array(values) => {
                serde_json::Value::Array(values.iter().map(admission).collect())
            }
            value => value.clone(),
        }
    }
    let mut current = admission(current);
    let mut previous = admission(previous);
    // These flags are PATH closeout outcomes, not their admitted path/tool identities.
    for document in [&mut current, &mut previous] {
        for key in ["cargo_tool_binding", "selected_compiler_binding"] {
            if let Some(tools) = document[key].as_object_mut() {
                tools.remove("paths_verified");
            }
        }
    }
    compare(&current, &previous)
}
