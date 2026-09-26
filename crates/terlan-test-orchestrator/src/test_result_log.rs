//! Private disposable libtest result files, separate from child-controlled stdout.

use crate::execution_environment::ExecutionEnvironment;
use crate::PhaseFailure;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use terlan_process_owner::ProcessControl;

static SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Owns only one freshly reserved directory and one known result file.
pub(super) struct TestResultLog {
    directory: Option<PathBuf>,
}

impl TestResultLog {
    /// Reserves under the frozen worktree, not a subsequently changed ambient TMPDIR.
    pub(super) fn create(environment: &ExecutionEnvironment) -> Result<Self, PhaseFailure> {
        Ok(Self {
            directory: Some(reserve_directory(environment, "rust-test-log")?),
        })
    }

    /// The fixed leaf is created by the admitted libtest harness.
    pub(super) fn path(&self) -> PathBuf {
        self.directory
            .as_ref()
            .expect("owned test log")
            .join("libtest.log")
    }

    /// Reads and hashes exactly the bounded bytes checked by the result parser.
    pub(super) fn read(
        &self,
        control: ProcessControl<'_>,
    ) -> Result<(Vec<u8>, String), PhaseFailure> {
        let mut digest = Sha256::new();
        let bytes = crate::file_identity::read_hashed_file(
            &self.path(),
            &mut digest,
            16 * 1024 * 1024,
            control,
            Instant::now(),
        )?;
        Ok((bytes, crate::file_identity::hex(digest)))
    }

    /// Successful execution cannot hide a cleanup failure or delete unexpected siblings.
    pub(super) fn close(mut self) -> Result<(), PhaseFailure> {
        remove(self.directory.as_deref().expect("owned test log"))?;
        self.directory = None;
        Ok(())
    }
}

/// Reserves an exclusive private namespace; callers own only explicitly registered leaves.
pub(super) fn reserve_directory(
    environment: &ExecutionEnvironment,
    prefix: &str,
) -> Result<PathBuf, PhaseFailure> {
    if prefix.is_empty()
        || !prefix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err(failure("invalid private namespace prefix"));
    }
    let parent = home::env::Env::current_dir(environment)
        .map_err(failure)?
        .join("target/quality");
    fs::create_dir_all(&parent).map_err(failure)?;
    let time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(failure)?
        .as_nanos();
    for _ in 0..64 {
        let directory = parent.join(format!(
            "{prefix}-{}-{time}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let builder = fs::DirBuilder::new();
        #[cfg(unix)]
        let builder = {
            use std::os::unix::fs::DirBuilderExt;
            let mut builder = builder;
            builder.mode(0o700);
            builder
        };
        match builder.create(&directory) {
            Ok(()) => return Ok(directory),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(failure(error)),
        }
    }
    Err(failure("cannot reserve private test result namespace"))
}

impl Drop for TestResultLog {
    fn drop(&mut self) {
        if let Some(path) = &self.directory {
            if let Err(error) = remove(path) {
                eprintln!(
                    "[rust-test-suite] result log cleanup failed: {}",
                    error.detail
                );
            }
        }
    }
}

fn remove(directory: &Path) -> Result<(), PhaseFailure> {
    match fs::remove_file(directory.join("libtest.log")) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(failure(error)),
    }
    fs::remove_dir(directory).map_err(failure)
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "test-result-log-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "test_result_log_test.rs"]
mod tests;
