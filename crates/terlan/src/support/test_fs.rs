use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Owns one exclusively created scratch directory through normal exit and unwind.
/// This does not claim cleanup after process abort or forced termination.
pub(crate) struct TestDirectory {
    path: Option<PathBuf>,
}

impl TestDirectory {
    /// Creates scratch without deleting or adopting a pre-existing directory.
    pub(crate) fn new(prefix: &str, name: &str) -> Self {
        for label in [prefix, name] {
            assert!(
                !label.is_empty()
                    && label
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric()
                            || matches!(byte, b'_' | b'-' | b'.')),
                "invalid scratch label"
            );
        }
        for _ in 0..64 {
            let path = temp_path(prefix, name);
            match fs::create_dir(&path) {
                Ok(()) => return Self { path: Some(path) },
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => panic!("create test scratch {}: {error}", path.display()),
            }
        }
        panic!("could not reserve unique test scratch");
    }

    /// Returns the owned path without transferring cleanup responsibility.
    pub(crate) fn path(&self) -> &Path {
        self.path.as_deref().expect("owned test scratch")
    }

    /// Removes scratch on success, surfacing cleanup errors as test failures.
    pub(crate) fn close(mut self) {
        fs::remove_dir_all(self.path()).expect("remove test scratch");
        self.path = None;
    }
}

impl std::ops::Deref for TestDirectory {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        self.path()
    }
}

impl AsRef<Path> for TestDirectory {
    fn as_ref(&self) -> &Path {
        self.path()
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            if let Err(error) = fs::remove_dir_all(path) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    eprintln!(
                        "test scratch cleanup failed for {}: {error}",
                        path.display()
                    );
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "test_fs_test.rs"]
mod tests;

/// Returns the repository root for filesystem-backed tests.
///
/// Inputs:
/// - Cargo's `CARGO_MANIFEST_DIR` for the single `crates/terlan` package.
///
/// Output:
/// - Absolute path to the repository root.
///
/// Transformation:
/// - Walks from `crates/terlan` to `crates`, then to the repository root and
///   canonicalizes the result so tests can join committed fixture paths.
pub(crate) fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("canonical repository root")
}

/// Creates a clean unique temporary directory for tests.
///
/// Inputs:
/// - `prefix`: feature or command prefix included in the directory name.
/// - `name`: readable test-specific suffix included in the directory name.
///
/// Output:
/// - Empty directory under the process temporary directory.
///
/// Transformation:
/// - Combines prefix, test name, process id, and current nanoseconds to avoid
///   collisions, removes stale content at the computed path, and recreates the
///   directory.
pub(crate) fn temp_dir(prefix: &str, name: &str) -> PathBuf {
    let path = temp_path(prefix, name);
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create temporary test directory");
    path
}

/// Creates a unique temporary filesystem path for tests.
///
/// Inputs:
/// - `prefix`: feature or command prefix included in the path name.
/// - `name`: readable test-specific suffix included in the path name.
///
/// Output:
/// - Path under the process temporary directory. The path is not created.
///
/// Transformation:
/// - Combines prefix, test name, process id, and current nanoseconds to avoid
///   collisions in parallel test runs.
pub(crate) fn temp_path(prefix: &str, name: &str) -> PathBuf {
    static NEXT_PATH: AtomicU64 = AtomicU64::new(0);
    std::env::temp_dir().join(format!(
        "terlan_{prefix}_{name}_{}_{}_{}",
        std::process::id(),
        timestamp_nanos(),
        NEXT_PATH.fetch_add(1, Ordering::Relaxed)
    ))
}

/// Writes a UTF-8 test fixture file.
///
/// Inputs:
/// - `path`: target file path.
/// - `contents`: text to write.
///
/// Output:
/// - File written at `path`.
///
/// Transformation:
/// - Creates the parent directory when present and writes the provided text.
pub(crate) fn write_file(path: &Path, contents: &str) {
    try_write_file(path, contents).expect("write fixture file");
}

fn try_write_file(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)
}

/// Owns a temporary repository and removes it on drop, including during unwinding.
pub(crate) struct TestRepo {
    root: PathBuf,
}

impl TestRepo {
    pub(crate) fn new(name: &str) -> io::Result<Self> {
        let root = temp_path("quality", name);
        fs::create_dir(&root)?;
        Ok(Self { root })
    }

    pub(crate) fn fixture(name: &str) -> Self {
        Self::new(name).expect("create repository fixture")
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }

    pub(crate) fn write(&self, relative: &str, contents: &str) -> io::Result<()> {
        try_write_file(&self.root.join(relative), contents)
    }

    pub(crate) fn write_fixture(&self, relative: &str, contents: &str) {
        self.write(relative, contents)
            .expect("write repository fixture");
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Returns the current timestamp in nanoseconds for unique test paths.
///
/// Inputs:
/// - System clock state.
///
/// Output:
/// - Nanosecond timestamp or `0` when the system clock is before the Unix
///   epoch.
///
/// Transformation:
/// - Converts `SystemTime::now()` into a compact numeric suffix.
fn timestamp_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}
