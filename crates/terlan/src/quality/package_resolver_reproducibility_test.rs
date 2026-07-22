use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

const LOCKFILE_CONTRACT: &str = r#"
terlan.lock
terlan-owned dependency resolution artifact
compiler contract
reproducible package resolution
local path dependencies
git dependencies
immutable `rev`
optional static index
hex, npm, and cargo dependencies
must be deterministic
target package manager lockfiles
secondary to `terlan.lock`
package name
package version
resolved source
source checksum
target/capability constraints
generated binding hashes
native artifact hashes
resolver version
"#;

const GIT_SOURCE_CONTRACT: &str = r#"
git source dependencies
url
immutable `rev`
floating branches and tags
resolution input
terlan.lock
release builds
must be deterministic
implicit network
local path dependencies
target package manager metadata
secondary
dependency name
repository url
immutable revision
resolved revision checksum
lockfile entry
resolver version
"#;

/// Verifies the resolver gate composes package source contracts and writes the
/// roadmap-required report.
///
/// Inputs:
/// - A temporary repository root with complete package lockfile and Git-source
///   contract docs.
///
/// Output:
/// - Test passes when the gate succeeds and writes the JSON report.
///
/// Transformation:
/// - Exercises the package resolver reproducibility gate without reading the
///   real repository docs.
#[test]
fn package_resolver_reproducibility_writes_report() {
    let root = TempRepo::new("package_resolver_reproducibility_writes_report");
    root.write("docs/package/TERLAN_PACKAGE_LOCKFILE.md", LOCKFILE_CONTRACT);
    root.write(
        "docs/package/TERLAN_PACKAGE_GIT_SOURCE.md",
        GIT_SOURCE_CONTRACT,
    );

    let summary = run_package_resolver_reproducibility(root.path())
        .expect("package resolver reproducibility gate");

    assert_eq!(12, summary.lockfile_term_count);
    assert_eq!(8, summary.lockfile_field_count);
    assert_eq!(12, summary.git_source_term_count);
    assert_eq!(6, summary.git_source_field_count);
    assert_eq!(
        "target/quality/package-resolver-reproducibility-report.json",
        summary.report_path
    );

    let report_path = root
        .path()
        .join("target/quality/package-resolver-reproducibility-report.json");
    let report = fs::read_to_string(&report_path).expect("read resolver report");
    assert!(report.contains("terlan.package-resolver-reproducibility.v1"));
    assert!(report.contains("contract-backed package source reproducibility"));
    assert!(report.contains("forbidden floating Git authority"));
}

/// Verifies the resolver gate preserves underlying package-source diagnostics.
///
/// Inputs:
/// - A temporary repository root with only the lockfile contract present.
///
/// Output:
/// - Test passes when the missing Git-source contract is surfaced.
///
/// Transformation:
/// - Keeps the composite gate from masking the failing subcheck.
#[test]
fn package_resolver_reproducibility_reports_missing_git_source_contract() {
    let root = TempRepo::new("package_resolver_reproducibility_missing_git_source");
    root.write("docs/package/TERLAN_PACKAGE_LOCKFILE.md", LOCKFILE_CONTRACT);

    let error = run_package_resolver_reproducibility(root.path())
        .expect_err("missing Git source contract should fail");

    assert!(error.contains("TERLAN_PACKAGE_GIT_SOURCE.md"));
    assert!(error.contains("failed to read package Git source contract"));
}

struct TempRepo {
    path: PathBuf,
}

impl TempRepo {
    fn new(name: &str) -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("terlan_{name}_{stamp}"));
        fs::create_dir_all(&path).expect("create temp repo");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn write(&self, relative_path: &str, text: &str) {
        let path = self.path.join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(path, text).expect("write fixture");
    }
}

impl Drop for TempRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
