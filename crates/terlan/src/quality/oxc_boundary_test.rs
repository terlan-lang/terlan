use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use regex::Regex;

use super::{
    forbidden_source_files, manifest_findings, run_oxc_boundary, source_findings_for_text,
    OxcBoundaryFinding,
};

/// Temporary repository fixture for Oxc boundary checks.
///
/// Inputs:
/// - Created with a unique path under the system temporary directory.
///
/// Output:
/// - Fixture root path and automatic cleanup on drop.
///
/// Transformation:
/// - Provides a tiny repo-shaped tree without external test dependencies.
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
    /// - New fixture root.
    ///
    /// Transformation:
    /// - Combines process id and time into a unique temp path, then creates
    ///   the root directory.
    fn new(name: &str) -> io::Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan-quality-oxc-boundary-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Returns the fixture root path.
    ///
    /// Inputs:
    /// - The fixture.
    ///
    /// Output:
    /// - Borrowed repository root path.
    ///
    /// Transformation:
    /// - Exposes the root as a `Path` for quality-check execution.
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
    /// - `Ok(())` when the file is written.
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

/// Verifies Oxc symbols are detected in forbidden source text.
///
/// Inputs:
/// - Fixture Rust source containing Oxc symbol shapes.
///
/// Output:
/// - Source findings with one-based line numbers.
///
/// Transformation:
/// - Applies the Oxc symbol regex to each source line.
#[test]
fn source_findings_for_text_reports_oxc_symbols() {
    let pattern = Regex::new(r"\b(?:Oxc|oxc_|oxc::)").expect("regex should compile");
    let text = "\
fn clean() {}\n\
fn leaked() { let value = OxcThing; }\n\
fn also_leaked() { oxc::parse(); }\n";

    let findings = source_findings_for_text(
        Path::new("crates/terlan/src/compiler/typeck/mod.rs"),
        text,
        &pattern,
    );

    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0].line, Some(2));
    assert_eq!(findings[1].line, Some(3));
}

/// Verifies normal Rust source does not produce Oxc findings.
///
/// Inputs:
/// - Fixture Rust source without Oxc symbols.
///
/// Output:
/// - Empty findings list.
///
/// Transformation:
/// - Confirms unrelated compiler code remains valid under the boundary check.
#[test]
fn source_findings_for_text_accepts_non_oxc_source() {
    let pattern = Regex::new(r"\b(?:Oxc|oxc_|oxc::)").expect("regex should compile");

    let findings = source_findings_for_text(
        Path::new("crates/terlan/src/compiler/typeck/mod.rs"),
        "fn lower_core_expr() {}\n",
        &pattern,
    );

    assert!(findings.is_empty());
}

/// Verifies Oxc dependency leaks are detected outside approved manifests.
///
/// Inputs:
/// - Temporary repo-shaped `crates/` tree with one approved manifest and one
///   disallowed manifest.
///
/// Output:
/// - One manifest finding for the disallowed crate.
///
/// Transformation:
/// - Scans Cargo manifests and preserves approved Oxc dependencies only in the
///   main Terlan package.
#[test]
fn manifest_findings_rejects_unapproved_oxc_dependencies() {
    let repo = TestRepo::new("manifest-leak").expect("create fixture");
    repo.write(
        "crates/terlan/Cargo.toml",
        "[package]\nname = \"terlan\"\n[dependencies]\noxc-parser = \"0.1\"\n",
    )
    .expect("write approved manifest");
    repo.write(
        "crates/other/Cargo.toml",
        "[package]\nname = \"other\"\n[dependencies]\noxc_resolver = \"0.1\"\n",
    )
    .expect("write disallowed manifest");

    let findings = manifest_findings(repo.root()).expect("manifest scan succeeds");

    assert_eq!(findings.len(), 1);
    assert_eq!(findings[0].path, Path::new("crates/other/Cargo.toml"));
    assert_eq!(findings[0].line, None);
    assert!(findings[0].message.contains("Oxc dependency"));
}

/// Verifies forbidden source discovery returns sorted Rust files only.
///
/// Inputs:
/// - Temporary forbidden compiler source root containing Rust and non-Rust
///   files.
///
/// Output:
/// - Sorted repository-relative `.rs` paths.
///
/// Transformation:
/// - Recursively walks configured forbidden roots and ignores unrelated file
///   extensions.
#[test]
fn forbidden_source_files_collects_sorted_rust_sources() {
    let repo = TestRepo::new("source-files").expect("create fixture");
    repo.write("crates/terlan/src/compiler/typeck/z.rs", "fn z() {}\n")
        .expect("write z source");
    repo.write("crates/terlan/src/compiler/typeck/a.rs", "fn a() {}\n")
        .expect("write a source");
    repo.write("crates/terlan/src/compiler/typeck/readme.txt", "ignored\n")
        .expect("write ignored text");

    let files = forbidden_source_files(repo.root()).expect("source discovery succeeds");

    assert_eq!(
        files,
        vec![
            Path::new("crates/terlan/src/compiler/typeck/a.rs").to_path_buf(),
            Path::new("crates/terlan/src/compiler/typeck/z.rs").to_path_buf(),
        ]
    );
}

/// Verifies the full boundary checker accepts a clean repo-shaped fixture.
///
/// Inputs:
/// - Temporary repository root with clean forbidden source and non-Oxc
///   manifests.
///
/// Output:
/// - Success summary with zero findings.
///
/// Transformation:
/// - Runs source and manifest checks through the public quality boundary.
#[test]
fn run_oxc_boundary_accepts_clean_repository_shape() {
    let repo = TestRepo::new("clean-repo").expect("create fixture");
    repo.write(
        "crates/terlan/src/compiler/typeck/mod.rs",
        "fn lower_core_expr() {}\n",
    )
    .expect("write clean source");
    repo.write(
        "crates/other/Cargo.toml",
        "[package]\nname = \"other\"\n[dependencies]\nserde = \"1\"\n",
    )
    .expect("write clean manifest");

    let summary = run_oxc_boundary(repo.root()).expect("clean fixture passes");

    assert_eq!(summary.finding_count, 0);
}

/// Verifies rendered findings include line numbers only when present.
///
/// Inputs:
/// - Source-style and manifest-style finding records.
///
/// Output:
/// - Stable rendered diagnostic strings.
///
/// Transformation:
/// - Formats source findings as `path:line` and manifest findings as `path`.
#[test]
fn oxc_boundary_finding_render_formats_source_and_manifest_findings() {
    let source = OxcBoundaryFinding {
        path: Path::new("crates/terlan/src/compiler/typeck/mod.rs").to_path_buf(),
        line: Some(7),
        message: "source leak".to_owned(),
    };
    let manifest = OxcBoundaryFinding {
        path: Path::new("crates/other/Cargo.toml").to_path_buf(),
        line: None,
        message: "manifest leak".to_owned(),
    };

    assert_eq!(
        source.render(),
        "crates/terlan/src/compiler/typeck/mod.rs:7: source leak"
    );
    assert_eq!(manifest.render(), "crates/other/Cargo.toml: manifest leak");
}
