//! Selected executable byte identities; not transitive SDK/library provenance.

use crate::{file_identity, PhaseFailure};
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use terlan_process_owner::ProcessControl;

const MAX_EXECUTABLE_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const HARNESS_ROLE: &str = "terlan-library-harness";

#[derive(Clone, Debug, PartialEq, Eq)]
struct ExecutableIdentity {
    role: &'static str,
    path: PathBuf,
    resolved: PathBuf,
    sha256: String,
    bytes: u64,
}

/// Admission and closeout identities for the driver's selected executables.
#[derive(Clone, Default)]
pub(super) struct ExecutableBinding {
    before: Vec<ExecutableIdentity>,
    after: Option<Vec<ExecutableIdentity>>,
    declaration: Option<crate::cargo_harness_admission::DeclaredHarness>,
}

impl ExecutableBinding {
    /// Binds the declared executables without launching probes or compilers.
    pub(super) fn capture(
        paths: &[(&'static str, PathBuf)],
        control: ProcessControl<'_>,
    ) -> Result<Self, PhaseFailure> {
        Self::capture_bounded(paths, MAX_EXECUTABLE_BYTES, control)
    }

    /// Shares byte accounting with callers that admit several independent target bindings.
    pub(super) fn capture_bounded(
        paths: &[(&'static str, PathBuf)],
        budget: u64,
        control: ProcessControl<'_>,
    ) -> Result<Self, PhaseFailure> {
        if budget > MAX_EXECUTABLE_BYTES {
            return Err(failure("executable byte budget exceeds the shared ceiling"));
        }
        if paths.is_empty() || paths.len() > 16 {
            return Err(failure("invalid executable inventory size"));
        }
        let started = Instant::now();
        let mut before = Vec::new();
        let mut remaining = budget;
        for (role, path) in paths {
            if before
                .iter()
                .any(|value: &ExecutableIdentity| value.role == *role)
            {
                return Err(failure("duplicate executable role"));
            }
            let identity = capture_file(role, path, remaining, control, started)?;
            remaining -= identity.bytes;
            before.push(identity);
        }
        Ok(Self {
            before,
            after: None,
            declaration: None,
        })
    }

    /// Binds test-program outputs after the workspace build, before any test executes.
    pub(super) fn bind_test_programs(
        &mut self,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        self.append_programs(&crate::test_program_paths(), control)
    }

    fn append_programs(
        &mut self,
        paths: &[(&'static str, PathBuf)],
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        if !self.is_bound()
            || self.after.is_some()
            || paths.is_empty()
            || self.before.len() + paths.len() > 16
            || paths
                .iter()
                .any(|(role, _)| self.before.iter().any(|row| row.role == *role))
        {
            return Err(failure(
                "test programs are unowned, already bound or closed",
            ));
        }
        let mut remaining =
            MAX_EXECUTABLE_BYTES - self.before.iter().map(|row| row.bytes).sum::<u64>();
        let mut added = Vec::new();
        let started = Instant::now();
        for (role, path) in paths {
            if added
                .iter()
                .any(|row: &ExecutableIdentity| row.role == *role)
            {
                return Err(failure("duplicate test program role"));
            }
            let identity = capture_file(role, path, remaining, control, started)?;
            remaining -= identity.bytes;
            added.push(identity);
        }
        self.before.extend(added);
        Ok(())
    }

    /// Binds production harness bytes only after declared libtest ownership was admitted.
    pub(super) fn bind_declared_harness(
        &mut self,
        declaration: crate::cargo_harness_admission::DeclaredHarness,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        declaration.verify(control)?;
        self.bind_harness(&declaration.executable, control)?;
        self.declaration = Some(declaration);
        Ok(())
    }

    /// Adds the one harness produced by the observed union-feature Cargo build.
    pub(super) fn bind_harness(
        &mut self,
        path: &Path,
        control: ProcessControl<'_>,
    ) -> Result<(), PhaseFailure> {
        if !self.is_bound()
            || self.after.is_some()
            || self.before.iter().any(|row| row.role == HARNESS_ROLE)
        {
            return Err(failure(
                "harness admission is missing, repeated, or already closed",
            ));
        }
        let remaining = MAX_EXECUTABLE_BYTES - self.before.iter().map(|row| row.bytes).sum::<u64>();
        self.before.push(capture_file(
            HARNESS_ROLE,
            path,
            remaining,
            control,
            Instant::now(),
        )?);
        Ok(())
    }

    /// Checks the exact selected harness before each test or inventory launch.
    pub(super) fn verify_harness(
        &self,
        control: ProcessControl<'_>,
    ) -> Result<PathBuf, PhaseFailure> {
        self.verify_program(HARNESS_ROLE, control)
    }

    /// Returns the admitted absolute invocation path after verifying its bytes.
    /// Keep its name: canonicalizing a multicall shim can change argv[0] dispatch.
    pub(super) fn verify_program(
        &self,
        role: &str,
        control: ProcessControl<'_>,
    ) -> Result<PathBuf, PhaseFailure> {
        if role == HARNESS_ROLE {
            if let Some(declaration) = &self.declaration {
                declaration.verify(control)?;
            }
        }
        let expected = self
            .before
            .iter()
            .find(|row| row.role == role)
            .ok_or_else(|| failure("selected executable has not been admitted"))?;
        let actual = capture_file(
            expected.role,
            &expected.path,
            MAX_EXECUTABLE_BYTES,
            control,
            Instant::now(),
        )?;
        if expected != &actual {
            return Err(failure(format!(
                "selected executable `{role}` changed after admission"
            )));
        }
        Ok(expected.path.clone())
    }

    /// Retains the full closeout observation and rejects changed executable bytes.
    pub(super) fn verify(&mut self, control: ProcessControl<'_>) -> Result<(), PhaseFailure> {
        if !self.is_bound() || self.after.is_some() {
            return Err(failure("executable closeout is missing or repeated"));
        }
        if let Some(declaration) = &self.declaration {
            declaration.verify(control)?;
        }
        let paths = self
            .before
            .iter()
            .map(|row| (row.role, row.path.clone()))
            .collect::<Vec<_>>();
        self.after = Some(Self::capture(&paths, control)?.before);
        if !self.verified() {
            return Err(failure(
                "selected executable inputs changed during execution",
            ));
        }
        Ok(())
    }

    /// Distinguishes an unbound generic ledger from executable-bound suite work.
    pub(super) fn is_bound(&self) -> bool {
        !self.before.is_empty()
    }

    /// Recognizes hard-linked or copied proxies by admitted bytes and permissions.
    pub(super) fn same_identity(&self, left: &str, right: &str) -> bool {
        self.before
            .iter()
            .find(|row| row.role == left)
            .zip(self.before.iter().find(|row| row.role == right))
            .is_some_and(|(left, right)| left.bytes == right.bytes && left.sha256 == right.sha256)
    }

    /// Requires equal byte observations, not an environment skip flag.
    pub(super) fn verified(&self) -> bool {
        self.is_bound() && self.after.as_ref() == Some(&self.before)
    }

    /// Reports only the actual selected executable scope, excluding transitive tools.
    pub(super) fn json(&self) -> serde_json::Value {
        let rows = |values: &[ExecutableIdentity]| {
            values
                .iter()
                .map(|row| {
                    serde_json::json!({
                        "role": row.role, "path": row.path, "resolved_path": row.resolved,
                        "identity_sha256": row.sha256, "bytes": row.bytes,
                    })
                })
                .collect::<Vec<_>>()
        };
        serde_json::json!({"scope": "selected-executable-bytes-v1", "before": rows(&self.before),
            "after": self.after.as_deref().map(rows), "declaration": self.declaration.as_ref().map(|value| value.json()), "verified": self.verified()})
    }
}

fn capture_file(
    role: &'static str,
    path: &Path,
    budget: u64,
    control: ProcessControl<'_>,
    started: Instant,
) -> Result<ExecutableIdentity, PhaseFailure> {
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_err(failure)?.join(path)
    };
    let resolved = fs::canonicalize(&path).map_err(|error| failure(format!("{role}: {error}")))?;
    if path.to_str().is_none() || resolved.to_str().is_none() {
        return Err(failure("executable paths must be UTF-8"));
    }
    let metadata = fs::metadata(&resolved).map_err(failure)?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(failure(format!(
            "{role}: executable is empty or not regular"
        )));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err(failure(format!("{role}: executable permission is missing")));
        }
    }
    let mut digest = Sha256::new();
    file_identity::field(&mut digest, b"terlan.selected-executable.v1");
    let bytes = file_identity::hash_file(&resolved, &mut digest, budget, control, started)?;
    if fs::canonicalize(&path).map_err(failure)? != resolved {
        return Err(failure("executable link changed during hashing"));
    }
    Ok(ExecutableIdentity {
        role,
        path,
        resolved,
        sha256: file_identity::hex(digest),
        bytes,
    })
}

/// Resolves the same explicit path or search path used by the process builder.
pub(super) fn resolve_program(program: &str, search: &OsStr) -> Result<PathBuf, PhaseFailure> {
    if Path::new(program).components().count() > 1 {
        #[cfg(windows)]
        if !program.to_ascii_lowercase().ends_with(".exe") {
            let suffixed = PathBuf::from(format!("{program}.exe"));
            if suffixed.is_file() {
                return Ok(suffixed);
            }
        }
        return Ok(PathBuf::from(program));
    }
    for directory in std::env::split_paths(search) {
        #[cfg(windows)]
        if directory.as_os_str().is_empty() {
            continue;
        }
        let candidate = directory.join(program);
        #[cfg(windows)]
        let candidate = if program.contains('.') {
            candidate
        } else {
            candidate.with_extension("exe")
        };
        if is_executable(&candidate) {
            return Ok(candidate);
        }
    }
    Err(failure(format!(
        "cannot resolve selected executable `{program}`"
    )))
}

fn is_executable(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        metadata.is_file()
    }
}

fn failure(detail: impl ToString) -> PhaseFailure {
    PhaseFailure {
        outcome: "executable-identity-failed",
        detail: detail.to_string(),
    }
}

#[cfg(test)]
#[path = "executable_binding_test.rs"]
mod tests;
