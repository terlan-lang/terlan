//! Explicitly scoped input observations shared by the live suite report.

use std::path::Path;

/// Scoped execution inputs, not complete reusable successful-test receipts.
#[derive(Default)]
pub(super) struct ValidationInputs {
    /// Git-listed working files observed at admission and closeout.
    pub(super) source: crate::source_inventory::SourceBinding,
    /// Driver-selected executable files, including the compiled library harness.
    pub(super) executables: crate::executable_binding::ExecutableBinding,
    /// Digest-only frozen environments and declared phase overrides.
    pub(super) environment: serde_json::Value,
    /// Cargo/Rustup configuration, including files outside the source inventory.
    pub(super) configuration: crate::tool_configuration::ToolConfiguration,
    /// Explicit Cargo compiler/wrapper entry points and their subprocess settings.
    pub(super) cargo_tools: crate::configured_cargo_tools::ConfiguredCargoTools,
    /// Installed Rustup-selected bin/lib bytes, not transitive SDK provenance.
    pub(super) toolchain: crate::rust_toolchain::RustToolchain,
    /// Actual selected wrapper-chain versions and byte-bound default sysroots.
    pub(super) compiler: crate::selected_compiler::SelectedCompiler,
    /// Fresh shared metadata generation, independently compared with suite inputs.
    pub(super) metadata: Option<crate::cargo_metadata_owner::Handoff>,
    /// Exact owned test names, independently reconciled with terminal phase evidence.
    pub(super) test_selections: Option<crate::test_selections::TestSelections>,
}

impl ValidationInputs {
    /// Exact execution inputs, excluding timings, test outcomes and observation-file names.
    pub(super) fn snapshot(&self) -> serde_json::Value {
        serde_json::json!({
            "source_binding":self.source.json(), "executable_binding":self.executables.json(),
            "environment_binding":self.environment, "tool_configuration_binding":self.configuration.json(),
            "cargo_tool_binding":self.cargo_tools.json(), "rust_toolchain_binding":self.toolchain.json(),
            "selected_compiler_binding":self.compiler.json(),
            "cargo_metadata_binding":self.metadata.as_ref().map(crate::cargo_metadata_owner::Handoff::json),
        })
    }

    /// Requires complete closed owners before considering a historical input comparison.
    pub(super) fn fully_verified(&self) -> bool {
        self.source.verified()
            && self.executables.verified()
            && self.configuration.verified()
            && self.cargo_tools.verified()
            && self.toolchain.verified()
            && self.compiler.verified()
            && !self.environment.is_null()
            && self.metadata.as_ref().is_some_and(|metadata| {
                let evidence = metadata.json();
                evidence["resolver_cache"]["verified"] == true
                    && evidence["package_sources"]["verified"] == true
            })
    }

    /// Hashes admitted execution identities while excluding run IDs and
    /// closeout-only observations that necessarily differ between retries.
    pub(super) fn phase_receipt_identity(&self) -> Result<String, crate::PhaseFailure> {
        use sha2::{Digest, Sha256};

        let checkout = std::fs::canonicalize(".").map_err(|error| crate::PhaseFailure {
            outcome: "phase-receipt-failed",
            detail: error.to_string(),
        })?;
        let mut snapshot = self.snapshot();
        snapshot["source_binding"] = self.source.phase_receipt_json();
        let bytes =
            serde_json::to_vec(&semantic_admission(snapshot, &checkout)).map_err(|error| {
                crate::PhaseFailure {
                    outcome: "phase-receipt-failed",
                    detail: error.to_string(),
                }
            })?;
        Ok(crate::file_identity::hex(Sha256::new_with_prefix(bytes)))
    }
}

/// Removes observations and checkout placement while preserving relative names,
/// contents, executable identities, configuration, and test selection inputs.
fn semantic_admission(value: serde_json::Value, checkout: &Path) -> serde_json::Value {
    match value {
        serde_json::Value::Object(fields) => serde_json::Value::Object(
            fields
                .into_iter()
                .filter(|(name, _)| {
                    !matches!(
                        name.as_str(),
                        "after" | "verified" | "paths_verified" | "run_id" | "reusable"
                    )
                })
                .map(|(name, value)| (name, semantic_admission(value, checkout)))
                .collect(),
        ),
        serde_json::Value::Array(values) => serde_json::Value::Array(
            values
                .into_iter()
                .map(|value| semantic_admission(value, checkout))
                .collect(),
        ),
        serde_json::Value::String(value) => {
            let path = Path::new(&value);
            match path.strip_prefix(checkout) {
                Ok(relative) if relative.as_os_str().is_empty() => "$CHECKOUT".into(),
                Ok(relative) => format!("$CHECKOUT/{}", relative.display()).into(),
                Err(_) => value.into(),
            }
        }
        value => value,
    }
}

#[cfg(test)]
#[path = "validation_inputs_test.rs"]
mod tests;
