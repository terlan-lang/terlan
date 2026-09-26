//! Cargo 1.96 default-tool dispatch before subprocess environment overrides.

use crate::execution_environment::ExecutionEnvironment;
use crate::PhaseFailure;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

/// Frozen selection context; filesystem-dependent dispatch is checked again at closeout.
pub(super) struct CargoDefaultTools {
    search: Vec<PathBuf>,
    home: Option<PathBuf>,
    toolchain: Option<OsString>,
}

impl CargoDefaultTools {
    /// Captures Cargo's own PATH, not the PATH its env table gives subprocesses.
    pub(super) fn capture(
        environment: &ExecutionEnvironment,
        proxy: Option<(&Path, &str)>,
    ) -> Result<Self, PhaseFailure> {
        let path = match proxy {
            Some((root, _)) => crate::cargo_proxy_path::capture(environment, root)?,
            None => environment.test_path().to_owned(),
        };
        let directory = home::env::Env::current_dir(environment).map_err(failure)?;
        Ok(Self {
            search: std::env::split_paths(&path)
                .map(|path| directory.join(path))
                .collect(),
            home: home::env::rustup_home_with_env(environment).ok(),
            toolchain: proxy
                .map(|(_, name)| name.into())
                .or_else(|| environment.value("RUSTUP_TOOLCHAIN")),
        })
    }

    /// Matches Cargo's size heuristic, including nonexecutable PATH files.
    pub(super) fn program(&self, tool: &str) -> PathBuf {
        self.native(tool).unwrap_or_else(|| tool.into())
    }

    fn native(&self, tool: &str) -> Option<PathBuf> {
        let name = self.toolchain.as_ref()?.to_str()?;
        if name.contains(['/', '\\']) {
            return None;
        }
        let tool_on_path = self.lookup(tool)?;
        let rustup_on_path = self.lookup("rustup")?;
        if fs::metadata(tool_on_path).ok()?.len() != fs::metadata(rustup_on_path).ok()?.len() {
            return None;
        }
        let native = self
            .home
            .as_ref()?
            .join("toolchains")
            .join(name)
            .join("bin")
            .join(Path::new(tool).with_extension(std::env::consts::EXE_EXTENSION));
        native.exists().then_some(native)
    }

    fn lookup(&self, tool: &str) -> Option<PathBuf> {
        // Cargo's resolve_executable checks is_file, not execute permission. On
        // Windows it checks the unsuffixed candidate before the .exe candidate.
        self.search
            .iter()
            .flat_map(|directory| {
                let candidate = directory.join(tool);
                let suffixed = (!std::env::consts::EXE_EXTENSION.is_empty())
                    .then(|| candidate.with_extension(std::env::consts::EXE_EXTENSION));
                std::iter::once(candidate).chain(suffixed)
            })
            .find(|candidate| candidate.is_file())
    }
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "cargo-default-tool-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "cargo_default_tools_test.rs"]
mod tests;
