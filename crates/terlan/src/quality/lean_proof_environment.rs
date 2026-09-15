//! Explicit host-tool inputs and private home/temp state for proof subprocesses.

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::terlan_quality::QualityResult;

/// Captures the permitted host settings once; no ambient proof flags survive.
pub(super) struct ProofEnvironment {
    path: OsString,
    elan_home: PathBuf,
    system_root: Option<OsString>,
}

impl ProofEnvironment {
    /// Identifies the declared environment; private paths use stable placeholders.
    #[cfg(target_os = "linux")]
    pub(super) fn identity(&self) -> QualityResult<String> {
        serde_json::to_string(&serde_json::json!({
            "policy": "terlan.proof-environment.v1",
            "path": self.path,
            "elan_home": self.elan_home,
            "system_root": self.system_root,
            "home": "<workspace>/home",
            "temporary": "<workspace>/tmp",
            "locale": "C",
            "timezone": "UTC",
        }))
        .map_err(|error| error.to_string())
    }

    /// Resolves only PATH, the installed Elan home, and Windows' system root.
    pub(super) fn capture() -> QualityResult<Self> {
        Self::from_lookup(|key| std::env::var_os(key))
    }

    pub(super) fn from_lookup(
        mut lookup: impl FnMut(&str) -> Option<OsString>,
    ) -> QualityResult<Self> {
        let path = lookup("PATH").ok_or("Lean proof execution requires PATH")?;
        if path.is_empty() || std::env::split_paths(&path).any(|entry| !entry.is_absolute()) {
            return Err("Lean proof PATH must contain only absolute nonempty entries".into());
        }
        let elan_home = match lookup("ELAN_HOME") {
            Some(value) => PathBuf::from(value),
            None => {
                let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
                PathBuf::from(lookup(key).ok_or("Lean proof execution requires ELAN_HOME")?)
                    .join(".elan")
            }
        };
        if !elan_home.is_absolute() || !elan_home.is_dir() {
            return Err("Lean proof ELAN_HOME must name an existing absolute directory".into());
        }
        let system_root = if cfg!(windows) {
            let value = lookup("SystemRoot").ok_or("Lean proof execution requires SystemRoot")?;
            if !Path::new(&value).is_absolute() {
                return Err("Lean proof SystemRoot must be absolute".into());
            }
            Some(value)
        } else {
            None
        };
        Ok(Self {
            path,
            elan_home,
            system_root,
        })
    }

    /// Selects a tool without cwd search or platform-dependent child PATH lookup.
    pub(super) fn program(&self, name: &str) -> QualityResult<PathBuf> {
        if !matches!(name, "lake" | "elan") {
            return Err("unsupported Lean proof tool".into());
        }
        let filename = if cfg!(windows) {
            format!("{name}.exe")
        } else {
            name.into()
        };
        for directory in std::env::split_paths(&self.path) {
            let candidate = directory.join(&filename);
            if executable_file(&candidate) {
                // Preserve the proxy's name: Elan dispatches lake via argv[0].
                return Ok(candidate);
            }
        }
        Err(format!(
            "Lean proof tool `{name}` is unavailable on the declared PATH"
        ))
    }

    /// Creates a closed environment in an exclusively owned proof workspace.
    pub(super) fn configure(
        &self,
        command: &mut Command,
        workspace: &Path,
        channel: &str,
    ) -> QualityResult<()> {
        let workspace = fs::canonicalize(workspace)
            .map_err(|error| format!("cannot resolve private proof workspace: {error}"))?;
        let home = workspace.join("home");
        let temporary = workspace.join("tmp");
        for directory in [&home, &temporary] {
            fs::create_dir(directory).map_err(|error| {
                format!("cannot reserve private proof process directory: {error}")
            })?;
        }
        command
            .env_clear()
            .env("PATH", &self.path)
            .env("ELAN_HOME", &self.elan_home)
            .env("ELAN_TOOLCHAIN", channel)
            .env("ELAN_NO_UPDATE_CHECK", "1")
            .env("HOME", &home)
            .env("USERPROFILE", &home)
            .env("TMPDIR", &temporary)
            .env("TMP", &temporary)
            .env("TEMP", &temporary)
            .env("LANG", "C")
            .env("LC_ALL", "C")
            .env("TZ", "UTC");
        if let Some(system_root) = &self.system_root {
            command.env("SystemRoot", system_root);
        }
        Ok(())
    }
}

fn executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// A longer or prefixed version string must not satisfy an exact tool pin.
pub(super) fn matches_version(output: &str, expected: &str) -> bool {
    output
        .trim()
        .strip_prefix("Lean (version ")
        .and_then(|text| text.split_once(','))
        .is_some_and(|(version, suffix)| {
            version == expected && suffix.ends_with(')') && !suffix.contains(['\r', '\n'])
        })
}

#[cfg(test)]
#[path = "lean_proof_environment_test.rs"]
mod tests;
