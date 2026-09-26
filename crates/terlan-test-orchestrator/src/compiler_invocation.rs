//! Frozen compiler-query commands retaining Cargo's wrapper argument contract.

use crate::execution_environment::identity;
use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

/// One distinct wrapper chain, reused for version and default-sysroot queries.
pub(super) struct CompilerInvocation {
    /// Workspace queries include the optional workspace wrapper; dependency queries do not.
    pub(super) workspace: bool,
    programs: Vec<PathBuf>,
    environment: Vec<(OsString, OsString)>,
    directory: PathBuf,
}

impl CompilerInvocation {
    /// Owns the already selected chain and compiler environment, never ambient state.
    pub(super) fn new(workspace: bool, programs: Vec<PathBuf>, environment: &Command) -> Self {
        Self {
            workspace,
            programs,
            environment: environment
                .get_envs()
                .filter_map(|(key, value)| Some((key.to_owned(), value?.to_owned())))
                .collect(),
            directory: environment
                .get_current_dir()
                .expect("compiler environment has frozen cwd")
                .to_owned(),
        }
    }

    /// Preserves bare compiler/wrapper arguments rather than substituting canonical paths.
    pub(super) fn command(&self) -> Command {
        let mut command = Command::new(&self.programs[0]);
        command
            .args(&self.programs[1..])
            .current_dir(&self.directory)
            .env_clear()
            .envs(self.environment.iter().map(|(key, value)| (key, value)));
        command
    }

    /// Records invocation inputs without publishing environment values.
    pub(super) fn json(&self) -> serde_json::Value {
        serde_json::json!({ "workspace": self.workspace, "program_chain": self.programs,
            "environment_identity_sha256": identity(&self.command()), "directory": self.directory })
    }
}
