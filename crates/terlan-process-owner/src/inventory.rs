//! Opt-in, bounded observations of owned OS launches and direct-child reaping.
//!
//! The declaration digest identifies explicit command-builder inputs, not source,
//! executable bytes, inherited environment, or reusable build work. Raw arguments,
//! paths, environment values and child output never enter the inventory.

use std::ffi::OsStr;
use std::fs::{OpenOptions, TryLockError};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

const VARIABLE: &str = "TERLAN_PROCESS_ACTIVITY_LOG";
const MAX_LOG_BYTES: u64 = 16 * 1024 * 1024;
static NEXT_ATTEMPT: AtomicU64 = AtomicU64::new(0);

#[path = "inventory_scope.rs"]
mod scope;

pub(crate) struct Inventory(Option<Attempt>);

struct Attempt {
    paths: Vec<PathBuf>,
    attempt: String,
    declaration: String,
    program_kind: &'static str,
    started: Instant,
    child: Option<u32>,
    terminal: bool,
}

impl Inventory {
    /// Nested destinations add a scope without redirecting the enclosing caller.
    pub(crate) fn begin(command: &mut Command) -> io::Result<Self> {
        let declared = command
            .get_envs()
            .find(|(key, _)| *key == VARIABLE)
            .and_then(|(_, value)| value.map(OsStr::to_os_string));
        let paths = scope::collect(enclosing_path().into_iter().chain(declared))?;
        if paths.is_empty() {
            return Ok(Self(None));
        }
        let encoded = scope::encode(&paths)?;
        let inventory = Self::begin_scopes(command, paths)?;
        // A sanitized environment must not silently detach subsequent
        // OwnedChild launches from the enclosing observation scope.
        command.env(VARIABLE, encoded);
        Ok(inventory)
    }

    #[cfg(test)]
    fn begin_at(command: &Command, path: Option<PathBuf>) -> io::Result<Self> {
        let Some(path) = path else {
            return Ok(Self(None));
        };
        if path.as_os_str().is_empty() {
            return Err(io::Error::other("process inventory path is empty"));
        }
        Self::begin_scopes(command, vec![path])
    }

    fn begin_scopes(command: &Command, paths: Vec<PathBuf>) -> io::Result<Self> {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(io::Error::other)?
            .as_nanos();
        let attempt = Attempt {
            paths,
            attempt: format!(
                "{}-{timestamp}-{}",
                std::process::id(),
                NEXT_ATTEMPT.fetch_add(1, Ordering::Relaxed)
            ),
            declaration: declaration(command),
            program_kind: program_kind(command.get_program()),
            started: Instant::now(),
            child: None,
            terminal: false,
        };
        attempt.record("started", None, None)?;
        Ok(Self(Some(attempt)))
    }

    pub(crate) fn spawned(&mut self, pid: u32) -> io::Result<()> {
        if let Some(attempt) = &mut self.0 {
            attempt.child = Some(pid);
            attempt.record("spawned", None, None)?;
        }
        Ok(())
    }

    pub(crate) fn spawn_failed(&mut self, error: &io::Error) -> io::Result<()> {
        if let Some(attempt) = &mut self.0 {
            attempt.record("spawn_failed", None, Some(format!("{:?}", error.kind())))?;
            attempt.terminal = true;
        }
        Ok(())
    }

    /// A nonzero exit is an observed outcome, not proof that the caller failed.
    pub(crate) fn reaped(&mut self, status: ExitStatus) -> io::Result<()> {
        if let Some(attempt) = &mut self.0 {
            attempt.record("reaped", Some(status), None)?;
            attempt.terminal = true;
        }
        Ok(())
    }
}

fn enclosing_path() -> Option<std::ffi::OsString> {
    #[cfg(test)]
    if tests::isolated_fixture_scope() {
        return None;
    }
    std::env::var_os(VARIABLE)
}

impl Drop for Inventory {
    fn drop(&mut self) {
        if let Some(attempt) = &self.0 {
            if !attempt.terminal {
                let _ = attempt.record("abandoned", None, None);
            }
        }
    }
}

impl Attempt {
    fn record(
        &self,
        state: &str,
        status: Option<ExitStatus>,
        error_kind: Option<String>,
    ) -> io::Result<()> {
        let mut bytes = serde_json::to_vec(&serde_json::json!({
            "schema": "terlan.process-activity.v1",
            "attempt": self.attempt,
            "declaration_sha256": self.declaration,
            "program_kind": self.program_kind,
            "owner_pid": std::process::id(),
            "child_pid": self.child,
            "state": state,
            "elapsed_micros": self.started.elapsed().as_micros() as u64,
            "exit_code": status.and_then(|value| value.code()),
            "exit_success": status.map(|value| value.success()),
            "error_kind": error_kind,
        }))?;
        bytes.push(b'\n');
        for path in &self.paths {
            append(path, &bytes)?;
        }
        Ok(())
    }
}

fn append(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(path)?;
    let waiting = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => break,
            Err(TryLockError::WouldBlock) if waiting.elapsed() < Duration::from_secs(5) => {
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(error) => return Err(io::Error::other(error)),
        }
    }
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len().saturating_add(bytes.len() as u64) > MAX_LOG_BYTES {
        return Err(io::Error::other(
            "process inventory exceeds its regular-file byte budget",
        ));
    }
    file.write_all(bytes)?;
    file.sync_data()
}

fn field(digest: &mut Sha256, value: &OsStr) {
    let bytes = value.as_encoded_bytes();
    digest.update((bytes.len() as u64).to_le_bytes());
    digest.update(bytes);
}

fn declaration(command: &Command) -> String {
    let mut digest = Sha256::new();
    field(&mut digest, OsStr::new("terlan.process-declaration.v1"));
    field(&mut digest, command.get_program());
    for argument in command.get_args() {
        field(&mut digest, OsStr::new("argument"));
        field(&mut digest, argument);
    }
    field(&mut digest, OsStr::new("cwd"));
    field(
        &mut digest,
        command
            .get_current_dir()
            .map_or(OsStr::new(""), Path::as_os_str),
    );
    for (key, value) in command.get_envs().filter(|(key, _)| *key != VARIABLE) {
        field(
            &mut digest,
            OsStr::new(if value.is_some() { "set" } else { "remove" }),
        );
        field(&mut digest, key);
        if let Some(value) = value {
            field(&mut digest, value);
        }
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn program_kind(program: &OsStr) -> &'static str {
    let name = Path::new(program)
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    match name.strip_suffix(".exe").unwrap_or(&name) {
        "cargo" => "cargo",
        "rustc" => "rustc",
        "terlc" => "terlc",
        "terlan-vm" => "terlan-vm",
        "git" => "git",
        "make" => "make",
        "sh" | "bash" => "shell",
        "ld" | "ld.lld" | "lld" | "clang" | "cc" | "gcc" => "native-tool",
        _ => "other",
    }
}

#[cfg(test)]
#[path = "inventory_test.rs"]
mod tests;
