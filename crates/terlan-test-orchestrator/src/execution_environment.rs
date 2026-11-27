//! Frozen direct-child environments with digest-only report observations.

use crate::file_identity::{field, hex};
use crate::{PhaseFailure, TestPhase};
use sha2::{Digest, Sha256};
use std::env;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

const MAX_ENTRIES: usize = 4_096;
const MAX_BYTES: usize = 1024 * 1024;
const CARGO_CACHE_AUTO_CLEAN_FREQUENCY: &str = "CARGO_CACHE_AUTO_CLEAN_FREQUENCY";
const VALIDATION_ROUTING_ENV: [&str; 32] = [
    "_",
    "__MISE_SESSION",
    "CODEX_TOOL_CALL_ID",
    "GIT_AUTHOR_DATE",
    "GIT_AUTHOR_EMAIL",
    "GIT_AUTHOR_NAME",
    "GIT_COMMITTER_DATE",
    "GIT_COMMITTER_EMAIL",
    "GIT_COMMITTER_NAME",
    "GIT_EDITOR",
    "GIT_REFLOG_ACTION",
    "OLDPWD",
    "PWD",
    "TERLAN_CARGO_METADATA_OWNER",
    "TERLAN_HOOK_LATEST_TERLC",
    "TERLAN_HOOK_VALIDATION_RECEIPT_DIR",
    "TERLAN_RUST_ORCHESTRATOR",
    "TERLAN_RUST_SUITE_REPORT",
    "TERLAN_RUST_PHASE_RECEIPT_DIR",
    "TERLAN_RUST_SUITE_TERLC",
    "TERLAN_RUST_SUITE_VM",
    "TERLAN_RUST_SUITE_NATIVE_WORKER",
    "TERLAN_TEST_CAPABILITY_WORKER",
    "TERLAN_NATIVE_WORKER",
    "TERLAN_TEST_COMPILER",
    "TERLAN_TEST_VM",
    "TERLAN_VM_RUNNER",
    "TERLAN_VALIDATION_BOOTSTRAPPED",
    "TEMP",
    "TMP",
    "TMPDIR",
    "SHLVL",
];
const TEST_PROGRAM_ROUTING_ENV: [&str; 3] = [
    "TERLAN_RUST_SUITE_TERLC",
    "TERLAN_RUST_SUITE_VM",
    "TERLAN_RUST_SUITE_NATIVE_WORKER",
];
const TEST_PROGRAM_RUNTIME_ENV: [(&str, &str); 2] = [
    ("TERLAN_RUST_SUITE_VM", "TERLAN_VM_RUNNER"),
    ("TERLAN_RUST_SUITE_NATIVE_WORKER", "TERLAN_NATIVE_WORKER"),
];

/// Owns one snapshot; subsequent launches never inherit the live environment.
pub(super) struct ExecutionEnvironment {
    entries: Vec<(OsString, OsString)>,
    directory: PathBuf,
    test_path: OsString,
    account_home: Option<PathBuf>,
}

impl ExecutionEnvironment {
    /// Uses Cargo's inherited search path without the test-harness binary prefix.
    pub(super) fn capture_direct() -> Result<Self, PhaseFailure> {
        let mut snapshot = Self::capture()?;
        snapshot.test_path = snapshot.value("PATH").unwrap_or_default();
        Ok(snapshot)
    }

    /// Captures before producer admission, preserving native OS string bytes.
    pub(super) fn capture() -> Result<Self, PhaseFailure> {
        // Resolve certificate auto-discovery once, before Cargo/Rustup can add
        // different implicit values to their children. Never mutate global env.
        let certificates = openssl_probe::probe();
        let mut snapshot =
            Self::from_entries(env::vars_os(), &env::current_dir().map_err(failure)?)?;
        snapshot.account_home = home::home_dir();
        snapshot
            .with_value(CARGO_CACHE_AUTO_CLEAN_FREQUENCY, "never")?
            .with_certificates(certificates)
    }

    /// Prevents Cargo's global cache collector from changing admitted resolver inputs.
    fn with_value(self, key: &str, value: &str) -> Result<Self, PhaseFailure> {
        let mut command = self.command(Path::new("environment-normalization"));
        command.env(key, value);
        let mut normalized = Self::from_entries(
            command
                .get_envs()
                .filter_map(|(key, value)| Some((key.to_owned(), value?.to_owned()))),
            &self.directory,
        )?;
        normalized.account_home = self.account_home;
        Ok(normalized)
    }

    fn with_certificates(
        self,
        certificates: openssl_probe::ProbeResult,
    ) -> Result<Self, PhaseFailure> {
        let mut command = self.command(Path::new("certificate-environment"));
        for (key, discovered) in [
            ("SSL_CERT_FILE", certificates.cert_file),
            ("SSL_CERT_DIR", certificates.cert_dir.into_iter().next()),
        ] {
            if !self
                .value(key)
                .is_some_and(|value| !value.is_empty() && self.directory.join(value).exists())
            {
                if let Some(path) = discovered {
                    command.env(key, path);
                }
            }
        }
        let mut normalized = Self::from_entries(
            command
                .get_envs()
                .filter_map(|(key, value)| Some((key.to_owned(), value?.to_owned()))),
            &self.directory,
        )?;
        normalized.account_home = self.account_home;
        Ok(normalized)
    }

    /// Normalizes supplied OS values without changing process-global state.
    pub(super) fn from_entries(
        entries: impl IntoIterator<Item = (OsString, OsString)>,
        directory: &Path,
    ) -> Result<Self, PhaseFailure> {
        let mut command = Command::new("environment-snapshot");
        command.env_clear();
        let mut bytes = 0;
        for (index, (key, value)) in entries.into_iter().enumerate() {
            bytes += key.as_encoded_bytes().len() + value.as_encoded_bytes().len();
            if index >= MAX_ENTRIES || bytes > MAX_BYTES {
                return Err(failure(
                    "execution environment exceeds its entry or byte budget",
                ));
            }
            if key.is_empty()
                || key.as_encoded_bytes().contains(&0)
                || value.as_encoded_bytes().contains(&0)
            {
                return Err(failure("execution environment contains an invalid entry"));
            }
            command.env(key, value);
        }
        let mut snapshot = Self {
            entries: command
                .get_envs()
                .filter_map(|(key, value)| Some((key.to_owned(), value?.to_owned())))
                .collect(),
            directory: std::fs::canonicalize(directory).map_err(failure)?,
            test_path: OsString::new(),
            account_home: None,
        };
        let inherited = snapshot.value("PATH").unwrap_or_default();
        snapshot.test_path = env::join_paths(
            std::iter::once(snapshot.directory.join("target/debug"))
                .chain(env::split_paths(&inherited)),
        )
        .map_err(|_| failure("cannot construct the declared test PATH"))?;
        Ok(snapshot)
    }

    /// Reads a key using Command's platform-specific case comparison rules.
    pub(super) fn value(&self, key: &str) -> Option<OsString> {
        command_value(&self.command(Path::new("environment-query")), key)
    }

    /// Creates a child with the captured working directory and no ambient values.
    pub(super) fn command(&self, program: &Path) -> Command {
        let mut command = Command::new(program);
        command
            .current_dir(&self.directory)
            .env_clear()
            .envs(self.entries.iter().map(|(key, value)| (key, value)));
        command
    }

    /// Adds the declared prebuilt-tool search path for Cargo and test execution.
    pub(super) fn test_command(&self, program: &Path) -> Command {
        let mut command = self.command(program);
        command.env("PATH", &self.test_path);
        for key in TEST_PROGRAM_ROUTING_ENV {
            command.env_remove(key);
        }
        for (source, destination) in TEST_PROGRAM_RUNTIME_ENV {
            if let Some(value) = self.value(source) {
                command.env(destination, value);
            }
        }
        command
    }

    /// Restores immutable test-program routes only for an owning Make graph.
    pub(super) fn test_program_command(&self, program: &Path) -> Command {
        let mut command = self.test_command(program);
        for key in TEST_PROGRAM_ROUTING_ENV {
            if let Some(value) = self.value(key) {
                command.env(key, value);
            }
        }
        command
    }

    /// Uses exactly the PATH installed on Cargo commands for executable admission.
    pub(super) fn test_path(&self) -> &OsStr {
        &self.test_path
    }

    /// Records effective environments, never plaintext variable names or values.
    pub(super) fn json(&self, phases: &[TestPhase]) -> serde_json::Value {
        let program = Path::new("environment-observation");
        let phases = phases
            .iter()
            .map(|phase| {
                let mut command = self.test_command(program);
                command.envs(phase.environment.iter().map(|(key, value)| (key, value)));
                serde_json::json!({"name": phase.name, "identity_sha256": validation_identity(&command)})
            })
            .collect::<Vec<_>>();
        serde_json::json!({
            "scope": "frozen-direct-child-environment-v1",
            "inherited_identity_sha256": validation_identity(&self.command(program)),
            "test_identity_sha256": validation_identity(&self.test_command(program)),
            "phases": phases,
        })
    }
}

/// Queries explicit command settings with the platform's environment-key rules.
pub(super) fn command_value(source: &Command, key: &str) -> Option<OsString> {
    // Keep inheritance enabled only on this never-spawned query: env_remove
    // then retains a tombstone with the map's existing platform-normalized key.
    let mut command = Command::new("environment-query");
    command.envs(
        source
            .get_envs()
            .filter_map(|(key, value)| Some((key, value?))),
    );
    command.env_remove(key);
    let removed = command.get_envs().find(|(_, value)| value.is_none())?.0;
    source
        .get_envs()
        .find(|(key, _)| *key == removed)
        .and_then(|(_, value)| value.map(OsStr::to_owned))
}

impl home::env::Env for ExecutionEnvironment {
    fn home_dir(&self) -> Option<PathBuf> {
        #[cfg(windows)]
        let configured = self.value("USERPROFILE").filter(|value| !value.is_empty());
        #[cfg(not(windows))]
        let configured = self.value("HOME");
        configured
            .map(PathBuf::from)
            .or_else(|| self.account_home.clone())
    }

    fn current_dir(&self) -> std::io::Result<PathBuf> {
        Ok(self.directory.clone())
    }

    fn var_os(&self, key: &str) -> Option<OsString> {
        self.value(key)
    }
}

/// Binds an explicit effective subprocess environment without disclosing values.
pub(super) fn identity(command: &Command) -> String {
    let mut digest = Sha256::new();
    field(&mut digest, b"terlan.execution-environment.v1");
    field(&mut digest, env::consts::OS.as_bytes());
    field(&mut digest, env::consts::ARCH.as_bytes());
    field(
        &mut digest,
        command
            .get_current_dir()
            .unwrap_or(Path::new(""))
            .as_os_str()
            .as_encoded_bytes(),
    );
    for (key, value) in command.get_envs() {
        field(&mut digest, key.as_encoded_bytes());
        field(&mut digest, &[u8::from(value.is_some())]);
        field(
            &mut digest,
            value.unwrap_or(OsStr::new("")).as_encoded_bytes(),
        );
    }
    hex(digest)
}

/// Excludes report/cache destinations that route validation without changing it.
pub(super) fn validation_identity(command: &Command) -> String {
    let mut normalized = Command::new(command.get_program());
    normalized.args(command.get_args()).env_clear();
    let generated_bin = command
        .get_current_dir()
        .map(|directory| directory.join("target/debug"));
    let entries = command
        .get_envs()
        .filter_map(|(key, value)| {
            let value = value?;
            if is_validation_routing_env(key) {
                return None;
            }
            let value = if key == OsStr::new("PATH") {
                let paths = env::split_paths(value)
                    .filter(|path| generated_bin.as_ref() != Some(path))
                    .collect::<Vec<_>>();
                env::join_paths(paths).ok()?
            } else {
                value.to_owned()
            };
            Some((key.to_owned(), value))
        })
        .collect::<Vec<_>>();
    normalized.envs(entries);
    identity(&normalized)
}

/// Binds only variables Cargo and Rust tooling can consume. Make, Terlan,
/// editor, CI and agent routing are admitted by their own owners and must not
/// invalidate the same Cargo tool selection when a later consumer runs
/// outside Make.
pub(super) fn cargo_validation_identity(command: &Command) -> String {
    let mut projected = Command::new(command.get_program());
    projected.args(command.get_args()).env_clear();
    if let Some(directory) = command.get_current_dir() {
        projected.current_dir(directory);
    }
    projected.envs(command.get_envs().filter_map(|(key, value)| {
        if !is_cargo_environment(key) || is_validation_routing_env(key) {
            return None;
        }
        Some((key.to_owned(), value?.to_owned()))
    }));
    validation_identity(&projected)
}

/// Projects a command onto the same Cargo/Rust environment boundary used by
/// tool-selection identity and metadata production.
pub(super) fn retain_cargo_environment(command: &mut Command) {
    let removed = command
        .get_envs()
        .filter(|(key, _)| !is_cargo_environment(key) || is_validation_routing_env(key))
        .map(|(key, _)| key.to_owned())
        .collect::<Vec<_>>();
    for key in removed {
        command.env_remove(key);
    }
}

fn is_cargo_environment(key: &OsStr) -> bool {
    let Some(key) = key.to_str() else {
        return false;
    };
    let upper = key.to_ascii_uppercase();
    [
        "CARGO_", "RUST", "RUSTUP_", "GIT_", "SSH_", "SSL_", "CURL_", "HTTP_", "HTTPS_", "LC_",
    ]
    .iter()
    .any(|prefix| upper.starts_with(prefix))
        || [
            "ALL_PROXY",
            "NO_PROXY",
            "HOME",
            "PATH",
            "USERPROFILE",
            "HOMEDRIVE",
            "HOMEPATH",
            "SYSTEMROOT",
            "SYSTEMDRIVE",
            "COMSPEC",
            "PATHEXT",
            "TMPDIR",
            "TMP",
            "TEMP",
            "LANG",
            "LANGUAGE",
            "TZ",
            "NETRC",
        ]
        .contains(&upper.as_str())
}

/// Recognizes destinations and executable routes whose selected bytes are
/// admitted separately from their location.
pub(super) fn is_validation_routing_env(key: &OsStr) -> bool {
    VALIDATION_ROUTING_ENV
        .iter()
        .any(|route| key == OsStr::new(route))
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "environment-admission-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "execution_environment_test.rs"]
mod execution_environment_test;
