use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{has_inline_test_marker, run_rust_quality, run_rustdoc, write_rustdoc_baseline};

/// Temporary quality-check repository fixture.
///
/// Inputs:
/// - Created with a unique path under the system temporary directory.
///
/// Output:
/// - Fixture root path and automatic cleanup on drop.
///
/// Transformation:
/// - Gives tests an isolated repository-like tree without adding external test
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
    /// - New fixture with `crates/` and `tools/quality/` directories.
    ///
    /// Transformation:
    /// - Combines process id and monotonic-ish time into a unique temp path,
    ///   then creates the directories required by the quality checker.
    fn new(name: &str) -> io::Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan-quality-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("crates/test/src"))?;
        fs::create_dir_all(root.join("tools/quality"))?;
        fs::write(root.join("tools/quality/rust_file_size_baseline.tsv"), "")?;
        fs::write(root.join("tools/quality/rust_inline_test_baseline.txt"), "")?;
        fs::write(root.join("tools/quality/rustdoc_missing_baseline.tsv"), "")?;
        Ok(Self { root })
    }

    /// Returns the repository fixture root path.
    ///
    /// Inputs:
    /// - The fixture.
    ///
    /// Output:
    /// - Borrowed repository root path.
    ///
    /// Transformation:
    /// - Exposes the root as a `Path` so checks can run against the fixture.
    fn root(&self) -> &Path {
        &self.root
    }

    /// Writes a Rust source fixture.
    ///
    /// Inputs:
    /// - `relative`: repository-relative source path.
    /// - `text`: source contents.
    ///
    /// Output:
    /// - `Ok(())` when the file is written.
    ///
    /// Transformation:
    /// - Creates parent directories and writes UTF-8 test source.
    fn write_source(&self, relative: &str, text: &str) -> io::Result<()> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)
    }

    /// Writes a quality baseline fixture.
    ///
    /// Inputs:
    /// - `name`: baseline filename under `tools/quality`.
    /// - `text`: baseline contents.
    ///
    /// Output:
    /// - `Ok(())` when the file is written.
    ///
    /// Transformation:
    /// - Updates the fixture baseline used by `run_rust_quality`.
    fn write_baseline(&self, name: &str, text: &str) -> io::Result<()> {
        fs::write(self.root.join("tools/quality").join(name), text)
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

/// Verifies adjacent test modules are not treated as inline-test debt.
///
/// Inputs:
/// - Source text containing `#[cfg(test)]` followed by an adjacent path module.
///
/// Output:
/// - Test passes when the marker is accepted.
///
/// Transformation:
/// - Locks the approved `#[path = "*_test.rs"]` pattern used throughout the
///   codebase.
#[test]
fn inline_test_marker_allows_adjacent_path_module() {
    let source = r#"
pub fn value() -> i32 { 1 }

#[cfg(test)]
#[path = "value_test.rs"]
mod value_test;
"#;

    assert!(!has_inline_test_marker(source));
}

/// Verifies ordinary inline test modules are rejected.
///
/// Inputs:
/// - Source text containing a direct inline test module.
///
/// Output:
/// - Test passes when the marker is detected as quality debt.
///
/// Transformation:
/// - Preserves the permanent rule that tests live in adjacent `*_test.rs`
///   files rather than implementation modules.
#[test]
fn inline_test_marker_rejects_inline_module() {
    let source = r#"
pub fn value() -> i32 { 1 }

#[cfg(test)]
mod tests {
    #[test]
    fn value_is_one() {}
}
"#;

    assert!(has_inline_test_marker(source));
}

/// Verifies documentation text mentioning test attributes is ignored.
///
/// Inputs:
/// - Source text whose Rustdoc mentions `#[cfg(test)]` but does not contain a
///   real attribute line.
///
/// Output:
/// - Test passes when the mention is not classified as inline-test debt.
///
/// Transformation:
/// - Prevents quality-rule documentation from tripping the quality scanner
///   itself.
#[test]
fn inline_test_marker_ignores_doc_comment_mentions() {
    let source = r#"
/// Explains why `#[cfg(test)]` belongs next to path-based modules.
pub fn value() -> i32 { 1 }
"#;

    assert!(!has_inline_test_marker(source));
}

/// Verifies a clean fixture passes the Rust quality gate.
///
/// Inputs:
/// - Repository fixture with one small implementation file and no baselines.
///
/// Output:
/// - Test passes when the quality summary reports no existing debt.
///
/// Transformation:
/// - Exercises the public quality-check entrypoint against a minimal repo tree.
#[test]
fn rust_quality_passes_clean_fixture() -> io::Result<()> {
    let repo = TestRepo::new("clean")?;
    repo.write_source("crates/test/src/lib.rs", "pub fn value() -> i32 { 1 }\n")?;

    let summary = run_rust_quality(repo.root()).expect("clean quality");

    assert_eq!(summary.oversized_count, 0);
    assert_eq!(summary.inline_test_count, 0);
    Ok(())
}

/// Verifies oversized files require an explicit reviewed baseline.
///
/// Inputs:
/// - Repository fixture with an implementation file over the default line
///   limit and no size baseline row.
///
/// Output:
/// - Test passes when the quality check reports the oversized file.
///
/// Transformation:
/// - Confirms the Rust implementation preserves the Python gate's hard
///   new-file limit behavior.
#[test]
fn rust_quality_rejects_unbaselined_oversized_file() -> io::Result<()> {
    let repo = TestRepo::new("oversized")?;
    let source = (0..1001)
        .map(|index| format!("pub fn value_{index}() -> i32 {{ {index} }}\n"))
        .collect::<String>();
    repo.write_source("crates/test/src/lib.rs", &source)?;

    let error = run_rust_quality(repo.root()).expect_err("oversized failure");

    assert!(error.contains("crates/test/src/lib.rs: 1001 lines exceeds 1000"));
    Ok(())
}

/// Verifies stale baseline rows fail the Rust quality gate.
///
/// Inputs:
/// - Repository fixture with a baseline row for a missing file.
///
/// Output:
/// - Test passes when the quality check reports the stale baseline.
///
/// Transformation:
/// - Keeps baselines honest so debt rows are removed as files are split or
///   deleted.
#[test]
fn rust_quality_rejects_stale_size_baseline_row() -> io::Result<()> {
    let repo = TestRepo::new("stale-size")?;
    repo.write_source("crates/test/src/lib.rs", "pub fn value() -> i32 { 1 }\n")?;
    repo.write_baseline(
        "rust_file_size_baseline.tsv",
        "crates/test/src/missing.rs\t1200\n",
    )?;

    let error = run_rust_quality(repo.root()).expect_err("stale baseline failure");

    assert!(error.contains("crates/test/src/missing.rs: stale file-size baseline row"));
    Ok(())
}

/// Verifies inline test baselines still gate direct inline test modules.
///
/// Inputs:
/// - Repository fixture with one inline test module and no inline baseline.
///
/// Output:
/// - Test passes when the quality check reports the inline test debt.
///
/// Transformation:
/// - Ensures new inline `#[cfg(test)]` modules cannot bypass the permanent
///   adjacent-test layout rule.
#[test]
fn rust_quality_rejects_unbaselined_inline_test_module() -> io::Result<()> {
    let repo = TestRepo::new("inline")?;
    repo.write_source(
        "crates/test/src/lib.rs",
        r#"
pub fn value() -> i32 { 1 }

#[cfg(test)]
mod tests {
    #[test]
    fn value_is_one() {}
}
"#,
    )?;

    let error = run_rust_quality(repo.root()).expect_err("inline failure");

    assert!(error.contains(
        "crates/test/src/lib.rs: new inline #[cfg(test)] block; move tests to adjacent *_test.rs"
    ));
    Ok(())
}

/// Verifies documented implementation items pass the Rustdoc gate.
///
/// Inputs:
/// - Repository fixture with documented Rust function and struct.
///
/// Output:
/// - Test passes when the Rustdoc summary reports no undocumented items.
///
/// Transformation:
/// - Exercises the Rustdoc scanner's normal documented-item path.
#[test]
fn rustdoc_passes_documented_items() -> io::Result<()> {
    let repo = TestRepo::new("rustdoc-clean")?;
    repo.write_source(
        "crates/test/src/lib.rs",
        r#"
/// Stores one value.
pub struct Value {
    item: i32,
}

/// Returns the stable value.
pub fn value() -> i32 {
    1
}
"#,
    )?;

    let summary = run_rustdoc(repo.root()).expect("rustdoc clean");

    assert_eq!(summary.undocumented_count, 0);
    Ok(())
}

/// Verifies undocumented implementation items fail the Rustdoc gate.
///
/// Inputs:
/// - Repository fixture with an undocumented Rust function.
///
/// Output:
/// - Test passes when the Rustdoc checker reports the item and line.
///
/// Transformation:
/// - Locks the permanent rule that implementation functions and types need
///   adjacent Rustdoc.
#[test]
fn rustdoc_rejects_undocumented_function() -> io::Result<()> {
    let repo = TestRepo::new("rustdoc-missing")?;
    repo.write_source(
        "crates/test/src/lib.rs",
        r#"
pub fn value() -> i32 {
    1
}
"#,
    )?;

    let error = run_rustdoc(repo.root()).expect_err("rustdoc failure");

    assert!(error.contains("crates/test/src/lib.rs:2: undocumented fn `value`"));
    Ok(())
}

/// Verifies stale Rustdoc baseline rows fail the Rustdoc gate.
///
/// Inputs:
/// - Repository fixture with a baseline row for a now-documented function.
///
/// Output:
/// - Test passes when the stale row is reported.
///
/// Transformation:
/// - Ensures the Rustdoc baseline shrinks as documentation debt is removed.
#[test]
fn rustdoc_rejects_stale_baseline_row() -> io::Result<()> {
    let repo = TestRepo::new("rustdoc-stale")?;
    repo.write_source(
        "crates/test/src/lib.rs",
        r#"
/// Returns the stable value.
pub fn value() -> i32 {
    1
}
"#,
    )?;
    repo.write_baseline(
        "rustdoc_missing_baseline.tsv",
        "crates/test/src/lib.rs\tfn\tvalue\tpub fn value() -> i32\n",
    )?;

    let error = run_rustdoc(repo.root()).expect_err("rustdoc stale baseline");

    assert!(error.contains(
        "crates/test/src/lib.rs\tfn\tvalue\tpub fn value() -> i32: stale Rustdoc baseline row"
    ));
    Ok(())
}

/// Verifies raw string fixture contents do not count as Rust items.
///
/// Inputs:
/// - Repository fixture with source-like text inside a raw string literal.
///
/// Output:
/// - Test passes when the embedded fake function is ignored.
///
/// Transformation:
/// - Preserves the old Python check's fixture-string guard in the Rust port.
#[test]
fn rustdoc_ignores_raw_string_fixture_items() -> io::Result<()> {
    let repo = TestRepo::new("rustdoc-raw-string")?;
    repo.write_source(
        "crates/test/src/lib.rs",
        r##"
/// Returns fixture text.
pub fn fixture() -> &'static str {
    r#"
pub fn fake_fixture_function() -> i32 {
    1
}
"#
}
"##,
    )?;

    let summary = run_rustdoc(repo.root()).expect("raw string ignored");

    assert_eq!(summary.undocumented_count, 0);
    Ok(())
}

/// Verifies Rustdoc baseline writing uses current undocumented items.
///
/// Inputs:
/// - Repository fixture with one undocumented function.
///
/// Output:
/// - Test passes when baseline writing records the item and subsequent
///   validation succeeds.
///
/// Transformation:
/// - Covers the maintainer-only `rust-docs --write-baseline` path.
#[test]
fn rustdoc_write_baseline_records_current_items() -> io::Result<()> {
    let repo = TestRepo::new("rustdoc-write")?;
    repo.write_source(
        "crates/test/src/lib.rs",
        r#"
pub fn value() -> i32 {
    1
}
"#,
    )?;

    let count = write_rustdoc_baseline(repo.root()).expect("write rustdoc baseline");
    let summary = run_rustdoc(repo.root()).expect("written baseline valid");

    assert_eq!(count, 1);
    assert_eq!(summary.undocumented_count, 1);
    Ok(())
}
