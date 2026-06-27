use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::run_module_readmes;

/// Temporary repository fixture for module README checks.
///
/// Inputs:
/// - Created with a unique path under the system temporary directory.
///
/// Output:
/// - Fixture root path and automatic cleanup on drop.
///
/// Transformation:
/// - Provides a small repository-like tree without external test
///   dependencies.
struct TestRepo {
    root: PathBuf,
}

impl TestRepo {
    /// Creates an empty repository fixture.
    ///
    /// Inputs:
    /// - `name`: diagnostic name segment for the temporary directory.
    ///
    /// Output:
    /// - New fixture with `crates/`, `std/`, and `tools/quality/`.
    ///
    /// Transformation:
    /// - Combines process id and time into a unique temp path, then creates
    ///   the directories used by the module README checker.
    fn new(name: &str) -> io::Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan-quality-readmes-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("crates"))?;
        fs::create_dir_all(root.join("std"))?;
        fs::create_dir_all(root.join("tools/quality"))?;
        fs::write(
            root.join("tools/quality/module_readme_missing_baseline.txt"),
            "",
        )?;
        Ok(Self { root })
    }

    /// Returns the repository fixture root.
    ///
    /// Inputs:
    /// - The fixture.
    ///
    /// Output:
    /// - Borrowed repository root path.
    ///
    /// Transformation:
    /// - Exposes the root as a `Path` for check execution.
    fn root(&self) -> &Path {
        &self.root
    }

    /// Writes one fixture file.
    ///
    /// Inputs:
    /// - `relative`: repository-relative path.
    /// - `text`: file contents.
    ///
    /// Output:
    /// - `Ok(())` when written.
    ///
    /// Transformation:
    /// - Creates parent directories and writes UTF-8 text.
    fn write(&self, relative: &str, text: &str) -> io::Result<()> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)
    }
}

impl Drop for TestRepo {
    /// Removes the temporary repository fixture.
    ///
    /// Inputs:
    /// - The fixture root path.
    ///
    /// Output:
    /// - Best-effort cleanup.
    ///
    /// Transformation:
    /// - Deletes the temporary directory and ignores cleanup failures.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// Verifies Rust module directories with READMEs pass.
///
/// Inputs:
/// - Fixture crate directory containing Rust source and README.
///
/// Output:
/// - Test passes when no missing README is reported.
///
/// Transformation:
/// - Exercises direct Rust source ownership discovery.
#[test]
fn module_readmes_passes_documented_rust_module() -> io::Result<()> {
    let repo = TestRepo::new("rust-clean")?;
    repo.write("crates/example/src/lib.rs", "pub fn value() {}\n")?;
    repo.write("crates/example/src/README.md", "# Example\n")?;

    let summary = run_module_readmes(repo.root()).expect("module readmes clean");

    assert_eq!(summary.missing_count, 0);
    Ok(())
}

/// Verifies missing Rust module README files fail.
///
/// Inputs:
/// - Fixture crate directory containing Rust source without README.
///
/// Output:
/// - Test passes when the missing README diagnostic is reported.
///
/// Transformation:
/// - Locks the permanent rule that source-owning module directories document
///   their context.
#[test]
fn module_readmes_rejects_missing_rust_module_readme() -> io::Result<()> {
    let repo = TestRepo::new("rust-missing")?;
    repo.write("crates/example/src/lib.rs", "pub fn value() {}\n")?;

    let error = run_module_readmes(repo.root()).expect_err("missing README failure");

    assert!(error.contains("crates/example/src: missing README.md; use README_TEMPLATE.md"));
    Ok(())
}

/// Verifies standard-library Terlan directories are checked.
///
/// Inputs:
/// - Fixture std directory containing `.terl` source and README.
///
/// Output:
/// - Test passes when the Terlan source directory is accepted.
///
/// Transformation:
/// - Exercises the std-specific `.terl` and `.terli` ownership rule.
#[test]
fn module_readmes_checks_std_terlan_modules() -> io::Result<()> {
    let repo = TestRepo::new("std-clean")?;
    repo.write("std/core/bool.terl", "module std.core.Bool.\n")?;
    repo.write("std/core/README.md", "# Core\n")?;

    let summary = run_module_readmes(repo.root()).expect("std readmes clean");

    assert_eq!(summary.missing_count, 0);
    Ok(())
}

/// Verifies stale README baseline rows fail.
///
/// Inputs:
/// - Fixture module with a README plus a baseline row that still lists it as
///   missing.
///
/// Output:
/// - Test passes when the stale baseline row is reported.
///
/// Transformation:
/// - Ensures missing-README baselines shrink once documentation is added.
#[test]
fn module_readmes_rejects_stale_baseline_when_readme_exists() -> io::Result<()> {
    let repo = TestRepo::new("stale-readme")?;
    repo.write("crates/example/src/lib.rs", "pub fn value() {}\n")?;
    repo.write("crates/example/src/README.md", "# Example\n")?;
    repo.write(
        "tools/quality/module_readme_missing_baseline.txt",
        "crates/example/src\n",
    )?;

    let error = run_module_readmes(repo.root()).expect_err("stale README baseline");

    assert!(error.contains("crates/example/src: stale README baseline row; README.md now exists"));
    Ok(())
}

/// Verifies stale README baseline rows for removed directories fail.
///
/// Inputs:
/// - Fixture baseline row for a directory that does not exist.
///
/// Output:
/// - Test passes when the stale directory row is reported.
///
/// Transformation:
/// - Prevents deleted module directories from leaving permanent baseline debt.
#[test]
fn module_readmes_rejects_stale_baseline_when_directory_missing() -> io::Result<()> {
    let repo = TestRepo::new("stale-dir")?;
    repo.write(
        "tools/quality/module_readme_missing_baseline.txt",
        "crates/missing\n",
    )?;

    let error = run_module_readmes(repo.root()).expect_err("stale directory baseline");

    assert!(error.contains("crates/missing: stale README baseline row; directory no longer exists"));
    Ok(())
}
