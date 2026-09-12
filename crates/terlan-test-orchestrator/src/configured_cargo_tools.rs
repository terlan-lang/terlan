//! Byte binding for Cargo tool selections, not wrapper-internal closure.

use crate::cargo_default_tools::CargoDefaultTools;
use crate::cargo_tool_settings::CargoToolSettings;
use crate::executable_binding::{resolve_program, ExecutableBinding};
use crate::execution_environment::{command_value, identity, ExecutionEnvironment};
use crate::PhaseFailure;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use terlan_process_owner::ProcessControl;

/// Records declared compiler/wrapper entry points and their effective search path.
#[derive(Default)]
pub(super) struct ConfiguredCargoTools {
    programs: Vec<(&'static str, PathBuf)>,
    resolved: Vec<(&'static str, PathBuf)>,
    search: OsString,
    environment_identity: String,
    tools: ExecutableBinding,
    closed: bool,
    paths_match: bool,
    rustup_proxy: bool,
    defaults: Option<CargoDefaultTools>,
    default_roles: Vec<&'static str>,
    compiler_environment: Option<Command>,
}

impl ConfiguredCargoTools {
    /// Shares the already-admitted Rustdoc selection with its observation owner.
    pub(super) fn rustdoc(&self, control: ProcessControl<'_>) -> Result<PathBuf, PhaseFailure> {
        if self.resolve()? != self.resolved {
            return Err(failure("Rustdoc selection changed before execution"));
        }
        self.tools.verify_program("RUSTDOC", control)
    }

    /// Binds explicit and default entry points before any production Cargo launch.
    pub(super) fn capture_selected(
        settings: &CargoToolSettings,
        environment: &ExecutionEnvironment,
        proxy: Option<(&Path, &str)>,
        control: ProcessControl<'_>,
    ) -> Result<Self, PhaseFailure> {
        let defaults = CargoDefaultTools::capture(environment, proxy)?;
        let command = match proxy {
            Some((root, _)) => {
                let mut command =
                    environment.test_command(Path::new("cargo-subprocess-observation"));
                command.env("PATH", crate::cargo_proxy_path::capture(environment, root)?);
                settings.subprocess_from_command(command)?
            }
            None => settings.subprocess_command(environment)?,
        };
        Self::capture_command(
            settings,
            environment,
            command,
            proxy.is_some(),
            Some(defaults),
            control,
        )
    }

    /// Binds explicit overrides without replacing Cargo's selections or launching it.
    #[cfg(test)]
    pub(super) fn capture(
        settings: &CargoToolSettings,
        environment: &ExecutionEnvironment,
        control: ProcessControl<'_>,
    ) -> Result<Self, PhaseFailure> {
        control
            .check(std::time::Instant::now())
            .map_err(crate::process_failure)?;
        let command = settings.subprocess_command(environment)?;
        Self::capture_command(settings, environment, command, false, None, control)
    }

    /// Includes Rustup's PATH adjustments before applying Cargo's env table.
    #[cfg(test)]
    pub(super) fn capture_for_proxy(
        settings: &CargoToolSettings,
        environment: &ExecutionEnvironment,
        toolchain: &Path,
        control: ProcessControl<'_>,
    ) -> Result<Self, PhaseFailure> {
        let mut command = environment.test_command(Path::new("cargo-subprocess-observation"));
        command.env(
            "PATH",
            crate::cargo_proxy_path::capture(environment, toolchain)?,
        );
        let command = settings.subprocess_from_command(command)?;
        Self::capture_command(settings, environment, command, true, None, control)
    }

    fn capture_command(
        settings: &CargoToolSettings,
        environment: &ExecutionEnvironment,
        command: Command,
        rustup_proxy: bool,
        defaults: Option<CargoDefaultTools>,
        control: ProcessControl<'_>,
    ) -> Result<Self, PhaseFailure> {
        control
            .check(std::time::Instant::now())
            .map_err(crate::process_failure)?;
        let directory = home::env::Env::current_dir(environment).map_err(failure)?;
        let search = command_value(&command, "PATH").unwrap_or_default();
        let search = std::env::join_paths(
            std::env::split_paths(&search)
                .filter(|path| !cfg!(windows) || !path.as_os_str().is_empty())
                .map(|path| directory.join(path)),
        )
        .map_err(|_| failure("cannot resolve Cargo subprocess search path"))?;
        let mut programs = settings.programs(environment)?;
        let mut default_roles = Vec::new();
        if defaults.is_some() {
            for (role, tool) in [("RUSTC", "rustc"), ("RUSTDOC", "rustdoc")] {
                if !programs.iter().any(|(selected, _)| *selected == role) {
                    programs.push((role, tool.into()));
                    default_roles.push(role);
                }
            }
        }
        let mut result = Self {
            programs,
            search,
            environment_identity: identity(&command),
            rustup_proxy,
            defaults,
            default_roles,
            ..Self::default()
        };
        result.resolved = result.resolve()?;
        if !result.resolved.is_empty() {
            result.tools = ExecutableBinding::capture(&result.resolved, control)?;
        }
        Ok(result)
    }

    /// Detects changed bytes, symlink retargeting, and a new earlier PATH candidate.
    pub(super) fn verify(&mut self, control: ProcessControl<'_>) -> Result<(), PhaseFailure> {
        control
            .check(std::time::Instant::now())
            .map_err(crate::process_failure)?;
        if !self.is_bound() || self.closed {
            return Err(failure(
                "configured Cargo tool admission is missing or closeout repeated",
            ));
        }
        self.closed = true;
        self.paths_match = self.resolve()? == self.resolved;
        if !self.paths_match {
            return Err(failure(
                "Cargo subprocess executable search changed after admission",
            ));
        }
        if self.tools.is_bound() {
            self.tools.verify(control)?;
        }
        Ok(())
    }

    /// Completes the compiler-query environment before the ledger records admission.
    pub(super) fn prepare_compiler_environment(
        &mut self,
        settings: &CargoToolSettings,
        environment: &ExecutionEnvironment,
        toolchain: &crate::rust_toolchain::RustToolchain,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        if self.compiler_environment.is_some() || self.closed {
            return Err(failure(
                "compiler environment cannot be replaced after admission",
            ));
        }
        let native = toolchain.native_cargo(control)?;
        let command = crate::cargo_compiler_environment::capture(
            settings,
            environment,
            toolchain.proxy(),
            &native,
        )?;
        self.environment_identity = identity(&command);
        self.compiler_environment = Some(command);
        Ok(())
    }

    /// Verifies selected paths and yields only distinct workspace/dependency chains.
    pub(super) fn compiler_invocations(
        &self,
        control: ProcessControl<'_>,
    ) -> Result<Vec<crate::compiler_invocation::CompilerInvocation>, PhaseFailure> {
        let environment = self
            .compiler_environment
            .as_ref()
            .ok_or_else(|| failure("compiler query environment was not admitted"))?;
        if self.resolve()? != self.resolved {
            return Err(failure("compiler selection changed before its probe"));
        }
        for (role, _) in &self.resolved {
            self.tools.verify_program(role, control)?;
        }
        let selected = self
            .programs
            .iter()
            .map(|(role, program)| (*role, self.selected_program(role, program)))
            .collect::<Vec<_>>();
        let has_workspace = selected
            .iter()
            .any(|(role, _)| *role == "RUSTC_WORKSPACE_WRAPPER");
        let mut invocations = Vec::new();
        for workspace in [true, false] {
            if !workspace && !has_workspace {
                continue;
            }
            let roles = [
                "RUSTC_WRAPPER",
                if workspace {
                    "RUSTC_WORKSPACE_WRAPPER"
                } else {
                    ""
                },
                "RUSTC",
            ];
            let programs = roles
                .iter()
                .filter_map(|role| {
                    selected
                        .iter()
                        .find(|(selected, _)| selected == role)
                        .map(|(_, program)| program.clone())
                })
                .collect();
            invocations.push(crate::compiler_invocation::CompilerInvocation::new(
                workspace,
                programs,
                environment,
            ));
        }
        Ok(invocations)
    }

    /// Even an explicit-only test observation with no selections requires closeout.
    pub(super) fn is_bound(&self) -> bool {
        !self.environment_identity.is_empty()
    }

    /// Requires closeout of the search choices as well as their executable bytes.
    pub(super) fn verified(&self) -> bool {
        self.is_bound()
            && self.closed
            && self.paths_match
            && (!self.tools.is_bound() || self.tools.verified())
    }

    /// Identifies entry-point scope without claiming wrapper-internal or sysroot closure.
    pub(super) fn json(&self) -> serde_json::Value {
        let scope = if self.defaults.is_some() {
            "selected-cargo-tool-entrypoints-v2"
        } else {
            "explicit-cargo-tool-entrypoints-v1"
        };
        serde_json::json!({ "scope": scope,
            "default_roles": self.default_roles,
            "rustup_proxy_path": self.rustup_proxy,
            "environment_observation_scope": if self.compiler_environment.is_some() { "cargo-compiler-query-environment-v1" } else { "frozen-input-plus-cargo-env-and-selected-proxy-path" },
            "subprocess_environment_identity_sha256": self.environment_identity,
            "tools": self.tools.json(), "paths_verified": self.paths_match, "verified": self.verified() })
    }

    fn resolve(&self) -> Result<Vec<(&'static str, PathBuf)>, PhaseFailure> {
        self.programs
            .iter()
            .map(|(role, program)| {
                let program = self.selected_program(role, program);
                let program = program
                    .to_str()
                    .ok_or_else(|| failure("Cargo tool path is not UTF-8"))?;
                Ok((*role, resolve_program(program, &self.search)?))
            })
            .collect()
    }

    fn selected_program(&self, role: &str, program: &Path) -> PathBuf {
        if self.default_roles.contains(&role) {
            if let Some(defaults) = &self.defaults {
                return defaults.program(
                    program
                        .to_str()
                        .expect("default tool names are static UTF-8"),
                );
            }
        }
        program.to_owned()
    }
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "cargo-tool-binding-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "configured_cargo_tools_test.rs"]
mod tests;
