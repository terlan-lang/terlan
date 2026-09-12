//! Observed compiler versions and default sysroots through Cargo's wrapper chains.

use crate::compiler_invocation::CompilerInvocation;
use crate::launch_ledger::LaunchLedger;
use crate::tool_tree::ToolTree;
use crate::{process_failure, PhaseFailure, ValidationTier};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use terlan_process_owner::ProcessControl;

/// Compiler-query observations, not closure of tools hidden inside arbitrary wrappers.
#[derive(Default)]
pub(super) struct SelectedCompiler {
    queries: Vec<serde_json::Value>,
    reported: Vec<(PathBuf, usize)>,
    sysroots: Vec<Sysroot>,
    closed: bool,
    paths_match: bool,
}

struct Sysroot {
    before: ToolTree,
    after: Option<ToolTree>,
    reuse_toolchain: bool,
}

impl SelectedCompiler {
    /// Runs only distinct wrapper contexts; installed sysroot bytes reuse their admitted owner.
    pub(super) fn admit(
        invocations: Vec<CompilerInvocation>,
        known: Option<&ToolTree>,
        ledger: &mut LaunchLedger,
        control: ProcessControl<'_>,
    ) -> Result<Self, PhaseFailure> {
        let control = control.limited_to(Duration::from_secs(30));
        if invocations.is_empty() || invocations.len() > 2 {
            return Err(failure(
                "compiler admission requires one or two distinct chains",
            ));
        }
        let mut result = Self::default();
        for invocation in invocations {
            let (version_phase, sysroot_phase) = if invocation.workspace {
                (
                    "Workspace compiler version admission",
                    "Workspace compiler sysroot admission",
                )
            } else {
                (
                    "Dependency compiler version admission",
                    "Dependency compiler sysroot admission",
                )
            };
            let version = ledger.execute(
                version_phase,
                ValidationTier::FastUnit,
                "rust-selected-version",
                |launched| {
                    let output = control
                        .capture_stdout(invocation.command().arg("-vV"), 64 * 1024, launched)
                        .map_err(process_failure)?;
                    crate::rust_toolchain::checked_version(&output)
                },
            )?;
            let root = ledger.execute(
                sysroot_phase,
                ValidationTier::FastUnit,
                "rust-selected-sysroot",
                |launched| {
                    let output = control
                        .capture_stdout(
                            invocation.command().args(["--print", "sysroot"]),
                            64 * 1024,
                            launched,
                        )
                        .map_err(process_failure)?;
                    sysroot_path(&output)
                },
            )?;
            let canonical = fs::canonicalize(&root).map_err(failure)?;
            let index = match result
                .sysroots
                .iter()
                .position(|sysroot| sysroot.before.root() == canonical)
            {
                Some(index) => index,
                None => {
                    let reuse = known.filter(|tree| tree.root() == canonical);
                    let before = match reuse {
                        Some(tree) => tree.clone(),
                        None => ToolTree::capture_sysroot(&root, control)?,
                    };
                    let index = result.sysroots.len();
                    result.sysroots.push(Sysroot {
                        before,
                        after: None,
                        reuse_toolchain: reuse.is_some(),
                    });
                    index
                }
            };
            result.queries.push(serde_json::json!({"invocation": invocation.json(), "release": version, "default_sysroot": root, "sysroot_identity_index": index}));
            result.reported.push((root, index));
        }
        Ok(result)
    }

    /// Rechecks every reported alias and hashes each independently owned sysroot once.
    pub(super) fn verify(
        &mut self,
        known: Option<&ToolTree>,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        let control = control.limited_to(Duration::from_secs(30));
        control
            .check(std::time::Instant::now())
            .map_err(process_failure)?;
        if !self.is_bound() || self.closed {
            return Err(failure("compiler admission is absent or closeout repeated"));
        }
        self.closed = true;
        self.paths_match = self.reported.iter().all(|(path, index)| {
            fs::canonicalize(path).is_ok_and(|root| root == self.sysroots[*index].before.root())
        });
        if !self.paths_match {
            return Err(failure("reported compiler sysroot path changed"));
        }
        for sysroot in &mut self.sysroots {
            sysroot.after = Some(if sysroot.reuse_toolchain {
                known
                    .filter(|tree| tree.root() == sysroot.before.root())
                    .ok_or_else(|| failure("reused Rustup sysroot owner has not passed closeout"))?
                    .clone()
            } else {
                ToolTree::capture_sysroot(sysroot.before.root(), control)?
            });
        }
        if !self.verified() {
            return Err(failure(
                "selected compiler sysroot changed during execution",
            ));
        }
        Ok(())
    }

    /// Distinguishes a complete admission from an unconfigured test ledger.
    pub(super) fn is_bound(&self) -> bool {
        !self.queries.is_empty()
    }

    /// Requires all aliases and shared/independent sysroot owners to close successfully.
    pub(super) fn verified(&self) -> bool {
        self.is_bound()
            && self.closed
            && self.paths_match
            && self
                .sysroots
                .iter()
                .all(|sysroot| sysroot.after.as_ref() == Some(&sysroot.before))
    }

    /// Explicitly excludes target/rustflag sysroot overrides and wrapper-internal tool closure.
    pub(super) fn json(&self) -> serde_json::Value {
        serde_json::json!({"scope": "cargo-wrapper-compiler-default-sysroots-v1", "queries": self.queries,
            "sysroots": self.sysroots.iter().map(|sysroot| serde_json::json!({"identity_owner": if sysroot.reuse_toolchain { "rustup-toolchain" } else { "selected-compiler" }, "before": sysroot.before.json(), "after": sysroot.after.as_ref().map(ToolTree::json)})).collect::<Vec<_>>(),
            "paths_verified": self.paths_match, "verified": self.verified()})
    }
}

fn sysroot_path(output: &[u8]) -> Result<PathBuf, PhaseFailure> {
    let output = std::str::from_utf8(output).map_err(failure)?;
    let output = output.strip_suffix('\n').unwrap_or(output);
    let output = output.strip_suffix('\r').unwrap_or(output);
    if output.contains(['\n', '\r', '\0']) || !std::path::Path::new(output).is_absolute() {
        return Err(failure(
            "compiler must report one absolute default sysroot path",
        ));
    }
    Ok(PathBuf::from(output))
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "selected-compiler-admission-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "selected_compiler_test.rs"]
mod tests;
