//! Source admission shared by receipt-backed Cargo producers.

use crate::executable_binding::{resolve_program, ExecutableBinding};
use crate::execution_environment::ExecutionEnvironment;
use crate::source_inventory::{capture_with_git, SourceSnapshot};
use crate::PhaseFailure;
use std::io;
use std::path::{Path, PathBuf};
use std::time::Duration;
use terlan_process_owner::ProcessControl;

/// Binds a clean candidate revision to its actual working-source bytes.
///
/// Reuses the validation driver's bounded inventory and content hashing. This
/// detects changed boundary observations, not adversarial transient mutations
/// restored between observations; the checkout must remain exclusively owned.
pub struct SourceInputs {
    root: PathBuf,
    revision: String,
    environment: ExecutionEnvironment,
    git: PathBuf,
    executable: ExecutableBinding,
    before: SourceSnapshot,
}

impl SourceInputs {
    /// Admits actual Git-listed inputs for an expected, clean committed revision.
    pub fn capture(root: &Path, revision: &str, timeout: Duration) -> io::Result<Self> {
        Self::capture_inner(root, revision, timeout).map_err(input_error)
    }

    fn capture_inner(root: &Path, revision: &str, timeout: Duration) -> Result<Self, PhaseFailure> {
        if !matches!(revision.len(), 40 | 64)
            || !revision.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(failure("invalid candidate revision"));
        }
        let environment = ExecutionEnvironment::capture_direct()?;
        let search = environment.value("PATH").unwrap_or_default();
        let git = resolve_program("git", &search)?;
        let control = ProcessControl::new(timeout);
        let executable = ExecutableBinding::capture(&[("git", git.clone())], control)?;
        let before = snapshot(root, revision, &git, &environment, control)?;
        Ok(Self {
            root: root.to_owned(),
            revision: revision.to_owned(),
            environment,
            git,
            executable,
            before,
        })
    }

    /// Returns the content identity without disclosing source paths or contents.
    pub fn digest(&self) -> &str {
        &self.before.sha256
    }

    /// Rejects revision, source, status or Git executable changes before reuse/seal.
    pub fn verify(&mut self, timeout: Duration) -> io::Result<()> {
        let control = ProcessControl::new(timeout);
        let after = snapshot(
            &self.root,
            &self.revision,
            &self.git,
            &self.environment,
            control,
        )
        .map_err(input_error)?;
        self.executable.verify(control).map_err(input_error)?;
        if self.before != after {
            return Err(io::Error::other("working source changed during execution"));
        }
        Ok(())
    }
}

fn snapshot(
    root: &Path,
    revision: &str,
    git: &Path,
    environment: &ExecutionEnvironment,
    control: ProcessControl<'_>,
) -> Result<SourceSnapshot, PhaseFailure> {
    require_candidate(root, revision, git, environment, control)?;
    let source = capture_with_git(root, git, environment, control, &mut |_| Ok(()))?;
    require_candidate(root, revision, git, environment, control)?;
    Ok(source)
}

fn require_candidate(
    root: &Path,
    revision: &str,
    git: &Path,
    environment: &ExecutionEnvironment,
    control: ProcessControl<'_>,
) -> Result<(), PhaseFailure> {
    let current = control
        .capture_stdout(
            environment
                .command(git)
                .arg("-C")
                .arg(root)
                .args(["rev-parse", "--verify", "HEAD"]),
            128,
            |_| Ok(()),
        )
        .map_err(crate::process_failure)?;
    if current != format!("{revision}\n").as_bytes() {
        return Err(failure("candidate revision changed"));
    }
    let status = control
        .capture_stdout(
            environment.command(git).arg("-C").arg(root).args([
                "status",
                "--porcelain=v1",
                "--untracked-files=all",
                "-z",
            ]),
            1024 * 1024,
            |_| Ok(()),
        )
        .map_err(crate::process_failure)?;
    if !status.is_empty() {
        return Err(failure("candidate is not clean"));
    }
    Ok(())
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "source-input-binding-failed",
        detail: detail.to_string(),
    }
}

fn input_error(error: PhaseFailure) -> io::Error {
    io::Error::other(format!("source input admission failed ({})", error.outcome))
}
