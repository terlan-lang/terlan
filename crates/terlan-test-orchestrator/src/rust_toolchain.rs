//! Observed Rustup resolution and installed toolchain input identity.

use crate::executable_binding::ExecutableBinding;
use crate::execution_environment::ExecutionEnvironment;
use crate::launch_ledger::LaunchLedger;
use crate::tool_tree::ToolTree;
use crate::{process_failure, PhaseFailure, ValidationTier};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use terlan_process_owner::ProcessControl;

const PIN: &str = include_str!("../../../rust-toolchain.toml");

/// The selected installation, not proof of arbitrary Cargo compiler-wrapper closure.
#[derive(Default)]
pub(super) struct RustToolchain {
    root: PathBuf,
    proxy_name: Option<crate::rustup_selection::RustupSelection>,
    tools: ExecutableBinding,
    before: Option<ToolTree>,
    after: Option<ToolTree>,
}

impl RustToolchain {
    /// Resolves actual native tools through observed, noninstalling Rustup probes.
    pub(super) fn admit(
        environment: &ExecutionEnvironment,
        executables: &ExecutableBinding,
        ledger: &mut LaunchLedger,
        control: ProcessControl<'_>,
    ) -> Result<Self, PhaseFailure> {
        let control = control.limited_to(Duration::from_secs(30));
        let mut resolved = Vec::new();
        for (name, phase) in [
            ("cargo", "Rustup Cargo resolution"),
            ("rustc", "Rustup compiler resolution"),
        ] {
            let path = ledger.execute(
                phase,
                ValidationTier::FastUnit,
                "rustup-tool-resolution",
                |launched| {
                    let mut command =
                        environment.test_command(&executables.verify_program("rustup", control)?);
                    resolve_installed_tool(&mut command, name, control, launched)
                },
            )?;
            resolved.push(path);
        }
        let root = installation(&resolved[0], &resolved[1])?;
        let selected_cargo =
            fs::canonicalize(executables.verify_program("cargo", control)?).map_err(failure)?;
        let rustup =
            fs::canonicalize(executables.verify_program("rustup", control)?).map_err(failure)?;
        if selected_cargo != rustup
            && selected_cargo != fs::canonicalize(&resolved[0]).map_err(failure)?
            && !executables.same_identity("cargo", "rustup")
        {
            return Err(failure(
                "selected Cargo is neither the admitted Rustup proxy nor its native Cargo",
            ));
        }
        let proxy_name = executables
            .same_identity("cargo", "rustup")
            .then(|| {
                crate::rustup_selection::observe(environment, executables, &root, ledger, control)
            })
            .transpose()?;
        let tools = ExecutableBinding::capture(
            &[
                ("native-cargo", resolved[0].clone()),
                ("native-rustc", resolved[1].clone()),
            ],
            control,
        )?;
        let before = ToolTree::capture(&root, control)?;
        Ok(Self {
            root,
            proxy_name,
            tools,
            before: Some(before),
            after: None,
        })
    }

    /// Rehashes the installation and native entry points before successful closeout.
    pub(super) fn verify(&mut self, control: ProcessControl<'_>) -> Result<(), PhaseFailure> {
        let control = control.limited_to(Duration::from_secs(30));
        if !self.is_bound() || self.after.is_some() {
            return Err(failure(
                "Rust toolchain admission is absent or closeout repeated",
            ));
        }
        self.after = Some(ToolTree::capture(&self.root, control)?);
        self.tools.verify(control)?;
        if !self.verified() {
            return Err(failure("selected Rust toolchain changed during execution"));
        }
        Ok(())
    }

    /// Retains the invocation root, including a Rustup custom-toolchain symlink.
    pub(super) fn root(&self) -> Option<&Path> {
        self.is_bound().then_some(self.root.as_path())
    }

    /// The observed name exported by the selected Cargo proxy, never a guessed alias.
    pub(super) fn proxy_name(&self) -> Option<&str> {
        self.proxy_name
            .as_ref()
            .map(|selection| selection.name.as_str())
    }

    /// Provides the observed proxy context for compiler commands, if Cargo is proxied.
    pub(super) fn proxy(&self) -> Option<(&Path, &crate::rustup_selection::RustupSelection)> {
        self.proxy_name
            .as_ref()
            .map(|selection| (self.root.as_path(), selection))
    }

    /// Returns the native Cargo image used to set Cargo's compiler-probe environment.
    pub(super) fn native_cargo(
        &self,
        control: ProcessControl<'_>,
    ) -> Result<PathBuf, PhaseFailure> {
        self.tools
            .verify_program("native-cargo", control)
            .and_then(|path| fs::canonicalize(path).map_err(failure))
    }

    /// Distinguishes generic ledger tests from an admitted installation.
    pub(super) fn is_bound(&self) -> bool {
        self.before.is_some()
    }

    /// Shares an existing bin/lib observation when a compiler reports this same sysroot.
    pub(super) fn admitted_tree(&self) -> Option<&ToolTree> {
        self.before.as_ref()
    }

    /// A reused sysroot cannot close until its original tree owner has verified it.
    pub(super) fn verified_tree(&self) -> Option<&ToolTree> {
        self.after.as_ref().filter(|_| self.verified())
    }

    /// Requires matching complete bin/lib observations and native entry points.
    pub(super) fn verified(&self) -> bool {
        self.is_bound() && self.before == self.after && self.tools.verified()
    }

    /// Explicitly identifies this installation scope rather than all build inputs.
    pub(super) fn json(&self) -> serde_json::Value {
        serde_json::json!({
            "scope": "rustup-active-bin-lib-v1",
            "cargo_proxy_toolchain": self.proxy_name(),
            "cargo_proxy_source": self.proxy_name.as_ref().map(|selection| selection.source),
            "probe_environment_overrides": {"RUSTUP_AUTO_INSTALL": "0"},
            "native_tools": self.tools.json(),
            "before": self.before.as_ref().map(ToolTree::json),
            "after": self.after.as_ref().map(ToolTree::json), "verified": self.verified(),
        })
    }
}

/// Shares bounded Rustup path admission while the caller owns command environment
/// and process accounting. This query never installs a missing toolchain.
pub(super) fn resolve_installed_tool(
    command: &mut std::process::Command,
    name: &str,
    control: ProcessControl<'_>,
    launched: &mut dyn FnMut(u32) -> Result<(), String>,
) -> Result<PathBuf, PhaseFailure> {
    command
        .args(["which", name])
        .env("RUSTUP_AUTO_INSTALL", "0");
    let output = control
        .capture_stdout(command, 64 * 1024, launched)
        .map_err(process_failure)?;
    resolved_path(&output)
}

fn resolved_path(output: &[u8]) -> Result<PathBuf, PhaseFailure> {
    let output = std::str::from_utf8(output).map_err(failure)?;
    let output = output.strip_suffix('\n').unwrap_or(output);
    let output = output.strip_suffix('\r').unwrap_or(output);
    if output.contains(['\n', '\r', '\0']) || !Path::new(output).is_absolute() {
        return Err(failure(
            "Rustup must return one absolute installed-tool path",
        ));
    }
    // Validate existence without discarding an alias that Cargo can follow later.
    fs::canonicalize(output).map_err(failure)?;
    Ok(PathBuf::from(output))
}

fn installation(cargo: &Path, rustc: &Path) -> Result<PathBuf, PhaseFailure> {
    let bin = cargo
        .parent()
        .ok_or_else(|| failure("native Cargo has no parent"))?;
    if rustc.parent() != Some(bin)
        || bin.file_name() != Some(std::ffi::OsStr::new("bin"))
        || cargo.file_name()
            != Some(std::ffi::OsStr::new(&format!(
                "cargo{}",
                std::env::consts::EXE_SUFFIX
            )))
        || rustc.file_name()
            != Some(std::ffi::OsStr::new(&format!(
                "rustc{}",
                std::env::consts::EXE_SUFFIX
            )))
    {
        return Err(failure(
            "native Cargo and rustc must belong to one installed toolchain",
        ));
    }
    bin.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| failure("Rust toolchain has no root"))
}

/// Checks the selected compiler's reported release against the repository pin.
pub(super) fn checked_version(output: &[u8]) -> Result<String, PhaseFailure> {
    checked_version_with_pin(output, PIN)
}

fn checked_version_with_pin(output: &[u8], pin: &str) -> Result<String, PhaseFailure> {
    let pin: toml::Value = toml::from_str(pin).map_err(failure)?;
    let expected = pin
        .get("toolchain")
        .and_then(|toolchain| toolchain.get("channel"))
        .and_then(toml::Value::as_str)
        .ok_or_else(|| failure("Rust pin has no channel"))?;
    let output = std::str::from_utf8(output).map_err(failure)?;
    let releases = output
        .lines()
        .filter_map(|line| line.strip_prefix("release: "))
        .collect::<Vec<_>>();
    if releases != [expected] {
        return Err(failure(
            "selected rustc release does not match the repository toolchain pin",
        ));
    }
    let hosts = output
        .lines()
        .filter_map(|line| line.strip_prefix("host: "))
        .collect::<Vec<_>>();
    if hosts.len() != 1
        || hosts[0].is_empty()
        || !hosts[0]
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(failure(
            "selected rustc version must report one valid host triple",
        ));
    }
    Ok(expected.to_owned())
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "rust-toolchain-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "rust_toolchain_test.rs"]
mod rust_toolchain_test;
