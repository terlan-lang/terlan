//! Bounded execution-point inventory for native compilation and linking.

use std::fs::{OpenOptions, TryLockError};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::super::BuildOneError;

const MAX_LOG_BYTES: u64 = 16 * 1024 * 1024;
static NEXT_ATTEMPT: AtomicU64 = AtomicU64::new(0);

/// Native work categories distinguish in-process emission from child launches.
#[derive(Clone, Copy)]
pub(super) enum Operation {
    ModuleObject,
    ApplicationObject,
    NativeLink,
}

impl Operation {
    fn name(self) -> &'static str {
        match self {
            Self::ModuleObject => "module-object",
            Self::ApplicationObject => "application-object",
            Self::NativeLink => "native-link",
        }
    }
}

struct ActiveActivity {
    path: PathBuf,
    operation: Operation,
    input: String,
    attempt: String,
    started: Instant,
    child_pid: Option<u32>,
    terminal: bool,
}

/// Records actual attempts without equating them with reusable cache checkpoints.
pub(super) struct Activity(Option<ActiveActivity>);

impl Activity {
    /// Enables inventory only when the caller supplies an explicit cycle log.
    pub(super) fn begin(operation: Operation, input: &str) -> Result<Self, BuildOneError> {
        Self::begin_at(
            std::env::var_os("TERLAN_BUILD_ACTIVITY_LOG").map(PathBuf::from),
            operation,
            input,
        )
    }

    fn begin_at(
        path: Option<PathBuf>,
        operation: Operation,
        input: &str,
    ) -> Result<Self, BuildOneError> {
        let Some(path) = path else {
            return Ok(Self(None));
        };
        if path.as_os_str().is_empty()
            || input.len() != 64
            || !input
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(failure("invalid cycle path or native input digest"));
        }
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(failure)?
            .as_nanos();
        let activity = ActiveActivity {
            path,
            operation,
            input: input.to_owned(),
            attempt: format!(
                "{}-{timestamp}-{}",
                std::process::id(),
                NEXT_ATTEMPT.fetch_add(1, Ordering::Relaxed)
            ),
            started: Instant::now(),
            child_pid: None,
            terminal: false,
        };
        activity.record("started", None)?;
        Ok(Self(Some(activity)))
    }

    /// Records a child only after the OS has successfully returned its PID.
    pub(super) fn spawned(&mut self, pid: u32) -> Result<(), String> {
        if let Some(activity) = &mut self.0 {
            activity.child_pid = Some(pid);
            activity
                .record("spawned", None)
                .map_err(|error| format!("{error:?}"))?;
        }
        Ok(())
    }

    /// Seals an attempt outcome; an inventory failure also rejects the build.
    pub(super) fn finish(mut self, success: bool, bytes: Option<u64>) -> Result<(), BuildOneError> {
        if let Some(activity) = &mut self.0 {
            activity.record(if success { "completed" } else { "failed" }, bytes)?;
            activity.terminal = true;
        }
        Ok(())
    }
}

impl ActiveActivity {
    fn record(&self, state: &str, bytes: Option<u64>) -> Result<(), BuildOneError> {
        let mut record = serde_json::to_vec(&serde_json::json!({
            "schema": "terlan.native-build-activity.v1",
            "attempt": self.attempt,
            "operation": self.operation.name(),
            "input_sha256": self.input,
            "compiler_pid": std::process::id(),
            "child_pid": self.child_pid,
            "state": state,
            "elapsed_micros": self.started.elapsed().as_micros() as u64,
            "artifact_bytes": bytes,
        }))
        .map_err(failure)?;
        record.push(b'\n');
        let mut file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&self.path)
            .map_err(failure)?;
        let waiting = Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => break,
                Err(TryLockError::WouldBlock) if waiting.elapsed() < Duration::from_secs(5) => {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(error) => return Err(failure(error)),
            }
        }
        if file
            .metadata()
            .map_err(failure)?
            .len()
            .saturating_add(record.len() as u64)
            > MAX_LOG_BYTES
        {
            return Err(failure("cycle inventory exceeds its 16 MiB budget"));
        }
        file.write_all(&record)
            .and_then(|()| file.sync_data())
            .map_err(failure)
    }
}

impl Drop for Activity {
    fn drop(&mut self) {
        if let Some(activity) = &self.0 {
            if !activity.terminal {
                let _ = activity.record("abandoned", None);
            }
        }
    }
}

fn failure(error: impl std::fmt::Display) -> BuildOneError {
    BuildOneError::Message(format!("error[tvm.build_activity]: {error}"))
}

#[cfg(test)]
#[path = "build_activity_test.rs"]
mod tests;
