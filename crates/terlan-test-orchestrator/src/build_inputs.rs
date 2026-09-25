//! Reusable Cargo configuration and selected executable entry-point identities.
//!
//! Reuses the validation driver's include resolution, environment precedence,
//! Rustup selection, file bounds and mutation checks. This binds configuration
//! and executable bytes, not arbitrary wrapper-internal or SDK dependencies.
use crate::configured_cargo_tools::ConfiguredCargoTools;
use crate::executable_binding::{resolve_program, ExecutableBinding};
use crate::execution_environment::ExecutionEnvironment;
use crate::tool_configuration::ToolConfiguration;
use crate::PhaseFailure;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;
use terlan_process_owner::ProcessControl;

/// A bounded input snapshot, verified again before publishing or reusing outputs.
pub struct CargoInputFiles {
    configuration: ToolConfiguration,
    tools: ConfiguredCargoTools,
    executables: ExecutableBinding,
    digest: String,
    cargo_program: String,
    cargo_search: OsString,
    selected_cargo: PathBuf,
}

impl CargoInputFiles {
    /// Captures inputs from the current directory and effective Cargo environment.
    pub fn capture(cargo: &Path, timeout: Duration) -> io::Result<Self> {
        Self::capture_inner(cargo, ProcessControl::new(timeout)).map_err(input_error)
    }

    fn capture_inner(cargo: &Path, control: ProcessControl<'_>) -> Result<Self, PhaseFailure> {
        let environment = ExecutionEnvironment::capture_direct()?;
        let search = environment.value("PATH").unwrap_or_default();
        let cargo = cargo
            .to_str()
            .ok_or_else(|| failure("Cargo path is not UTF-8"))?;
        let cargo_program = cargo.to_owned();
        let cargo = resolve_program(cargo, &search)?;
        let rustup = resolve_program("rustup", &search).ok();
        let mut paths = vec![("cargo", cargo.clone())];
        if let Some(rustup) = &rustup {
            paths.push(("rustup", rustup.clone()));
        }
        let mut executables = ExecutableBinding::capture(&paths, control)?;
        let proxy = if executables.same_identity("cargo", "rustup") {
            let rustup = rustup
                .as_ref()
                .ok_or_else(|| failure("missing Cargo proxy"))?;
            let native_cargo = crate::rust_toolchain::resolve_installed_tool(
                &mut environment.command(rustup),
                "cargo",
                control,
                &mut |_| Ok(()),
            )?;
            let root = native_cargo
                .parent()
                .and_then(Path::parent)
                .ok_or_else(|| failure("invalid selected Cargo installation"))?
                .to_path_buf();
            let output = control
                .capture_stdout(
                    environment
                        .command(rustup)
                        .args(["show", "active-toolchain"])
                        .env("RUSTUP_AUTO_INSTALL", "0"),
                    64 * 1024,
                    |_| Ok(()),
                )
                .map_err(crate::process_failure)?;
            let home = home::env::rustup_home_with_env(&environment).ok();
            let selection = crate::rustup_selection::parse(&output, &root, home.as_deref())?;
            executables.verify(control)?;
            paths.push(("native-cargo", native_cargo));
            executables = ExecutableBinding::capture(&paths, control)?;
            Some((root, selection.name))
        } else {
            None
        };
        let configuration = ToolConfiguration::capture(&environment, control)?;
        let tools = ConfiguredCargoTools::capture_selected(
            configuration.cargo_settings(),
            &environment,
            proxy
                .as_ref()
                .map(|(root, name)| (root.as_path(), name.as_str())),
            control,
        )?;
        let config = configuration.json();
        let executable_rows = executables.json();
        let selected_tools = tools.json();
        let identity = serde_json::json!({
            "schema":"terlan.cargo-input-files.v1",
            "configuration":config["before"],
            "cargo_load_order":config["cargo_load_order"],
            "executables":executable_rows["before"],
            "selected_tools":selected_tools["tools"]["before"],
            "proxy":proxy.as_ref().map(|(root, name)| (root, name)),
        });
        let mut digest = Sha256::new();
        digest.update(serde_json::to_vec(&identity).map_err(failure)?);
        Ok(Self {
            configuration,
            tools,
            executables,
            digest: digest
                .finalize()
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
            cargo_program,
            cargo_search: search,
            selected_cargo: cargo,
        })
    }

    /// Returns a digest only; configuration values are not exposed to callers.
    pub fn digest(&self) -> &str {
        &self.digest
    }

    /// Rejects changed configurations, tool bytes, symlinks or PATH selections.
    pub fn verify(&mut self, timeout: Duration) -> io::Result<()> {
        let control = ProcessControl::new(timeout);
        if resolve_program(&self.cargo_program, &self.cargo_search).map_err(input_error)?
            != self.selected_cargo
        {
            return Err(io::Error::other("Cargo executable selection changed"));
        }
        self.configuration.verify(control).map_err(input_error)?;
        self.tools.verify(control).map_err(input_error)?;
        self.executables.verify(control).map_err(input_error)
    }
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "cargo-input-binding-failed",
        detail: detail.to_string(),
    }
}

fn input_error(error: PhaseFailure) -> io::Error {
    // Configuration can contain credentials, including malformed tool values.
    io::Error::other(format!("Cargo input admission failed ({})", error.outcome))
}
