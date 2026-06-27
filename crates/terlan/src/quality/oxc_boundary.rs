use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::terlan_quality::QualityResult;

/// Source roots where Oxc symbols are forbidden.
const FORBIDDEN_SOURCE_ROOTS: &[&str] = &[
    "crates/terlan/src/compiler",
    "crates/terlan/src/backends/erlang",
    "crates/terlan/src/html",
    "crates/terlan/src/lsp",
    "crates/terlan/src/runtime/safenative",
    "crates/terlan/src/validation",
];

/// Cargo manifests allowed to depend on Oxc crates.
const APPROVED_OXC_DEP_CRATES: &[&str] = &["crates/terlan/Cargo.toml"];

/// Summary produced by the Oxc boundary check.
///
/// Inputs:
/// - `finding_count`: number of Oxc boundary findings.
///
/// Output:
/// - Stable success metric rendered by the command-line wrapper.
///
/// Transformation:
/// - Keeps boundary scan metrics separate from failure diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OxcBoundarySummary {
    pub finding_count: usize,
}

/// Oxc ownership boundary violation.
///
/// Inputs:
/// - `path`: repository-relative file path.
/// - `line`: optional one-based source line number.
/// - `message`: violation message.
///
/// Output:
/// - Immutable diagnostic record.
///
/// Transformation:
/// - Keeps file identity and explanation together for readable checker output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OxcBoundaryFinding {
    pub path: PathBuf,
    pub line: Option<usize>,
    pub message: String,
}

impl OxcBoundaryFinding {
    /// Returns a human-readable diagnostic line.
    ///
    /// Inputs:
    /// - Finding path, optional line, and message.
    ///
    /// Output:
    /// - Stable diagnostic text.
    ///
    /// Transformation:
    /// - Formats source findings as `path:line: message` and manifest findings
    ///   as `path: message`.
    pub fn render(&self) -> String {
        match self.line {
            Some(line) => format!("{}:{line}: {}", self.path.display(), self.message),
            None => format!("{}: {}", self.path.display(), self.message),
        }
    }
}

/// Runs the Oxc ownership boundary checker.
///
/// Inputs:
/// - `root`: repository root containing `crates/`.
///
/// Output:
/// - Success summary when Oxc stays inside approved JS/backend ownership.
/// - Diagnostics when Oxc symbols or dependencies leak into forbidden crates.
///
/// Transformation:
/// - Scans forbidden Rust source roots for Oxc-related identifiers.
/// - Scans crate manifests for Oxc dependencies outside approved crates.
pub fn run_oxc_boundary(root: &Path) -> QualityResult<OxcBoundarySummary> {
    let findings = check_boundary(root)?;
    if !findings.is_empty() {
        let mut message = String::from("oxc-boundary-check failed:");
        for finding in &findings {
            message.push_str("\n  - ");
            message.push_str(&finding.render());
        }
        return Err(message);
    }
    Ok(OxcBoundarySummary { finding_count: 0 })
}

/// Returns all current Oxc boundary findings.
///
/// Inputs:
/// - `root`: repository root containing forbidden source roots and manifests.
///
/// Output:
/// - Combined finding records.
///
/// Transformation:
/// - Concatenates source-symbol findings with dependency findings.
fn check_boundary(root: &Path) -> QualityResult<Vec<OxcBoundaryFinding>> {
    let mut findings = source_findings(root)?;
    findings.extend(manifest_findings(root)?);
    Ok(findings)
}

/// Returns Oxc symbol findings in forbidden Rust source files.
///
/// Inputs:
/// - `root`: repository root.
///
/// Output:
/// - Finding records for each Oxc symbol occurrence.
///
/// Transformation:
/// - Searches line by line so diagnostics can point to the exact leak.
fn source_findings(root: &Path) -> QualityResult<Vec<OxcBoundaryFinding>> {
    let symbol_pattern = Regex::new(r"\b(?:Oxc|oxc_|oxc::)")
        .map_err(|err| format!("failed to compile Oxc symbol regex: {err}"))?;
    let mut findings = Vec::new();
    for relative in forbidden_source_files(root)? {
        let text = fs::read_to_string(root.join(&relative)).map_err(|err| {
            format!(
                "{}: failed to read Rust source: {err}",
                root.join(&relative).display()
            )
        })?;
        findings.extend(source_findings_for_text(&relative, &text, &symbol_pattern));
    }
    Ok(findings)
}

/// Returns Oxc symbol findings for one source file.
///
/// Inputs:
/// - `path`: repository-relative source path.
/// - `text`: source file contents.
/// - `symbol_pattern`: compiled Oxc symbol regex.
///
/// Output:
/// - Finding records with one-based source line numbers.
///
/// Transformation:
/// - Tests each source line against the Oxc symbol pattern.
fn source_findings_for_text(
    path: &Path,
    text: &str,
    symbol_pattern: &Regex,
) -> Vec<OxcBoundaryFinding> {
    text.lines()
        .enumerate()
        .filter_map(|(index, line)| {
            symbol_pattern.is_match(line).then(|| OxcBoundaryFinding {
                path: path.to_path_buf(),
                line: Some(index + 1),
                message: "Oxc symbol is outside JS backend or binding-generator ownership"
                    .to_owned(),
            })
        })
        .collect()
}

/// Returns Oxc dependency findings outside approved crate manifests.
///
/// Inputs:
/// - `root`: repository root containing `crates/*/Cargo.toml`.
///
/// Output:
/// - Finding records for disallowed Oxc dependencies.
///
/// Transformation:
/// - Allows Oxc dependencies only in explicitly approved crate manifests.
fn manifest_findings(root: &Path) -> QualityResult<Vec<OxcBoundaryFinding>> {
    let dep_pattern = Regex::new(r"(?m)^\s*oxc[-_A-Za-z0-9]*\s*=")
        .map_err(|err| format!("failed to compile Oxc dependency regex: {err}"))?;
    let mut findings = Vec::new();
    let crates = root.join("crates");
    if !crates.is_dir() {
        return Ok(findings);
    }
    for entry in fs::read_dir(&crates)
        .map_err(|err| format!("{}: failed to read crates dir: {err}", crates.display()))?
    {
        let crate_dir = entry
            .map_err(|err| format!("{}: failed to read crates entry: {err}", crates.display()))?
            .path();
        let manifest = crate_dir.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let relative = manifest
            .strip_prefix(root)
            .map_err(|err| {
                format!(
                    "{}: failed to relativize manifest: {err}",
                    manifest.display()
                )
            })?
            .to_path_buf();
        if APPROVED_OXC_DEP_CRATES
            .iter()
            .any(|approved| relative == Path::new(approved))
        {
            continue;
        }
        let text = fs::read_to_string(&manifest)
            .map_err(|err| format!("{}: failed to read manifest: {err}", manifest.display()))?;
        if dep_pattern.is_match(&text) {
            findings.push(OxcBoundaryFinding {
                path: relative,
                line: None,
                message: "Oxc dependency must stay in approved JS backend/bindgen crates"
                    .to_owned(),
            });
        }
    }
    Ok(findings)
}

/// Returns Rust source files in Oxc-forbidden compiler areas.
///
/// Inputs:
/// - `root`: repository root.
///
/// Output:
/// - Sorted repository-relative Rust source paths.
///
/// Transformation:
/// - Recursively scans only configured forbidden roots that exist in the
///   current checkout.
fn forbidden_source_files(root: &Path) -> QualityResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    for source_root in FORBIDDEN_SOURCE_ROOTS {
        let absolute = root.join(source_root);
        if absolute.exists() {
            collect_rust_files(root, &absolute, &mut files)?;
        }
    }
    files.sort();
    Ok(files)
}

/// Recursively collects Rust source files.
///
/// Inputs:
/// - `root`: repository root used for relative paths.
/// - `dir`: current directory being scanned.
/// - `files`: output accumulator.
///
/// Output:
/// - `Ok(())` when the directory tree was read.
/// - Diagnostic string when a directory entry or relative path fails.
///
/// Transformation:
/// - Traverses directories recursively and pushes only `.rs` file paths.
fn collect_rust_files(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> QualityResult<()> {
    for entry in fs::read_dir(dir)
        .map_err(|err| format!("{}: failed to read source dir: {err}", dir.display()))?
    {
        let path = entry
            .map_err(|err| format!("{}: failed to read source entry: {err}", dir.display()))?
            .path();
        if path.is_dir() {
            collect_rust_files(root, &path, files)?;
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            let relative = path.strip_prefix(root).map_err(|err| {
                format!("{}: failed to relativize Rust path: {err}", path.display())
            })?;
            files.push(relative.to_path_buf());
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "oxc_boundary_test.rs"]
mod oxc_boundary_test;
