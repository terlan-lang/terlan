//! Retained native target names and fresh executable admission for batched coverage.

use crate::cargo_harness_admission::{DeclaredHarness, ManifestAdmission};
use crate::coverage_requests::{failure, Selection};
use crate::executable_binding::ExecutableBinding;
use crate::test_execution::ExpectedTests;
use crate::PhaseFailure;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use terlan_process_owner::ProcessControl;

/// A target's historical coverage plus its exact declaration and completed byte binding.
pub(super) struct Target {
    record: Value,
    names: ExpectedTests,
}

/// Validates native evidence once, sharing target inventories across all batch requests.
pub(super) fn inventory(report: &Value) -> Result<Vec<Target>, PhaseFailure> {
    let phases = report["phases"]
        .as_array()
        .ok_or_else(|| failure("missing suite phases"))?;
    let native = phases
        .iter()
        .filter(|phase| phase["executor"] == "cargo-native-harnesses")
        .collect::<Vec<_>>();
    if native.len() != 1
        || native[0]["outcome"] != "passed"
        || native[0]["test_execution"]["scope"] != "cargo-native-libtest-records-v1"
    {
        return Err(failure("missing completed canonical native phase"));
    }
    let rows = native[0]["test_execution"]["targets"]
        .as_array()
        .ok_or_else(|| failure("missing native targets"))?;
    let mut targets = Vec::new();
    let mut unique = std::collections::BTreeSet::new();
    for row in rows {
        if row["delegated_main"] == true {
            continue;
        }
        let target = &row["target"];
        let key = (&target["package"], &target["kind"], &target["target"]);
        let key = serde_json::to_string(&key).map_err(failure)?;
        if !unique.insert(key) {
            return Err(failure("duplicate native target coverage"));
        }
        let completion = &row["completion"];
        let launches = completion["launches"]
            .as_array()
            .ok_or_else(|| failure("missing native launches"))?;
        crate::workspace_native::validate_completion(
            completion,
            completion["certificate_identity"]
                .as_str()
                .ok_or_else(|| failure("missing target certificate"))?,
            completion["context_identity"]
                .as_str()
                .ok_or_else(|| failure("missing target context"))?,
            launches,
        )?;
        let binding = &row["executable_binding"];
        if binding["verified"] != true || binding["before"] != binding["after"] {
            return Err(failure(
                "native executable did not close with unchanged bytes",
            ));
        }
        targets.push(Target {
            record: row.clone(),
            names: ExpectedTests::native_names(&completion["passed_names"])?,
        });
    }
    Ok(targets)
}

impl Target {
    /// Matches an explicit package and kind without confusing library and integration names.
    pub(super) fn matches(&self, selection: &Selection) -> bool {
        let target = &self.record["target"];
        target["package"] == selection.package
            && target["kind"] == selection.kind
            && selection
                .target
                .as_ref()
                .is_none_or(|name| target["target"] == *name)
    }

    /// Computes selector coverage without executing or querying the retained harness again.
    pub(super) fn coverage(&self, selection: &Selection) -> Result<Value, PhaseFailure> {
        let args = selection
            .selectors
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        let names = if args.contains(&"--ignored") {
            &self.names.ignored
        } else {
            &self.names.passed
        };
        let selected = crate::test_inventory::select(names, &args)?.len();
        Ok(
            json!({"selected":selected,"passed":selected,"uncovered":0,"covered":selected > 0,"reusable":false}),
        )
    }

    /// Keeps the source declaration and byte identities required for current-input admission.
    pub(super) fn record(&self) -> &Value {
        &self.record
    }
}

/// Fresh native inputs, admitted once per requested target, not once per selector.
pub(super) struct Inputs(Vec<(DeclaredHarness, ExecutableBinding)>);

impl Inputs {
    /// Reuses manifest snapshots and applies the existing aggregate executable byte ceiling.
    pub(super) fn admit(
        records: &[Value],
        packages: &BTreeMap<String, PathBuf>,
        control: ProcessControl<'_>,
    ) -> Result<Self, PhaseFailure> {
        let mut manifests = ManifestAdmission::new(Path::new("."))?;
        let mut inputs = Vec::new();
        let mut bytes = 0_u64;
        for record in records {
            let declaration = manifests.restore(&record["target"], control)?;
            crate::cargo_metadata_owner::Handoff::admit_harness(packages, &declaration.json())?;
            let binding = ExecutableBinding::capture_bounded(
                &[("workspace-harness", declaration.executable.clone())],
                8 * 1024 * 1024 * 1024 - bytes,
                control,
            )?;
            if binding.json()["before"] != record["executable_binding"]["before"] {
                return Err(failure(
                    "requested native harness differs from completed suite",
                ));
            }
            bytes = bytes
                .checked_add(
                    binding.json()["before"][0]["bytes"]
                        .as_u64()
                        .ok_or_else(|| failure("missing native executable size"))?,
                )
                .ok_or_else(|| failure("native executable size overflow"))?;
            if bytes > 8 * 1024 * 1024 * 1024 {
                return Err(failure("native executables exceed aggregate byte budget"));
            }
            inputs.push((declaration, binding));
        }
        Ok(Self(inputs))
    }

    /// Rejects declaration or byte changes across the single batch input verification.
    pub(super) fn close(&mut self, control: ProcessControl<'_>) -> Result<(), PhaseFailure> {
        for (declaration, binding) in &mut self.0 {
            declaration.verify(control)?;
            binding.verify(control)?;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "native_coverage_test.rs"]
mod tests;
