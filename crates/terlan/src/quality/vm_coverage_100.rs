use std::env;
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::terlan_quality::{render_failure, QualityResult};

const DEFAULT_COVERAGE_JSON: &str = "/tmp/terlan-vm-coverage-100.json";

const REQUIRED_VM_FILES: &[&str] = &[
    "crates/terlan/src/runtime/vm/actor.rs",
    "crates/terlan/src/runtime/vm/failure.rs",
    "crates/terlan/src/runtime/vm/process.rs",
    "crates/terlan/src/runtime/vm/resource.rs",
    "crates/terlan/src/runtime/vm/scheduler.rs",
    "crates/terlan/src/runtime/vm/supervision.rs",
    "crates/terlan/src/runtime/vm/timer.rs",
];

/// Summary produced by the VM 100% coverage gate.
///
/// Inputs:
/// - A `cargo llvm-cov --json` report.
///
/// Output:
/// - Stable count of VM-owned files whose line/function coverage is enforced.
///
/// Transformation:
/// - Treats selected VM-owned runtime files as release baselines and rejects
///   any uncovered lines or functions in those files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmCoverage100Summary {
    pub coverage_file_count: usize,
}

/// Runs the VM 100% coverage gate.
///
/// Inputs:
/// - `root`: repository root.
/// - Optional `TERLAN_VM_COVERAGE_JSON` path to a coverage JSON report.
///
/// Output:
/// - Success summary when every required VM-owned file has no uncovered lines
///   and no uncovered functions.
/// - Stable diagnostics when the report is missing, malformed, stale, or below
///   the enforced baseline.
///
/// Transformation:
/// - Parses cargo-llvm-cov detailed JSON and compares required VM-owned file
///   rows by normalized repository suffix.
pub fn run_vm_coverage_100(root: &Path) -> QualityResult<VmCoverage100Summary> {
    let report_path =
        env::var("TERLAN_VM_COVERAGE_JSON").unwrap_or_else(|_| DEFAULT_COVERAGE_JSON.to_string());
    let text = fs::read_to_string(&report_path)
        .map_err(|err| format!("{report_path}: failed to read VM coverage report: {err}"))?;
    let report: Value = serde_json::from_str(&text)
        .map_err(|err| format!("{report_path}: invalid coverage JSON: {err}"))?;

    let mut diagnostics = Vec::new();
    for required_file in REQUIRED_VM_FILES {
        if !root.join(required_file).is_file() {
            diagnostics.push(format!("`{required_file}` is missing from the repository"));
            continue;
        }
        match find_file_summary(&report, required_file) {
            Some(summary) => diagnostics.extend(validate_file_summary(required_file, summary)),
            None => diagnostics.push(format!(
                "`{required_file}` is missing from the VM coverage report"
            )),
        }
    }

    if !diagnostics.is_empty() {
        return Err(render_failure("vm-coverage-100", &diagnostics));
    }

    Ok(VmCoverage100Summary {
        coverage_file_count: REQUIRED_VM_FILES.len(),
    })
}

/// Finds one file summary by repository-relative suffix.
fn find_file_summary<'a>(report: &'a Value, required_file: &str) -> Option<&'a Value> {
    report
        .get("data")?
        .as_array()?
        .first()?
        .get("files")?
        .as_array()?
        .iter()
        .find(|file| {
            file.get("filename")
                .and_then(Value::as_str)
                .map(normalize_coverage_path)
                .is_some_and(|filename| filename.ends_with(required_file))
        })
}

/// Validates one required file row has no uncovered source lines or functions.
fn validate_file_summary(required_file: &str, file: &Value) -> Vec<String> {
    let mut diagnostics = Vec::new();
    let Some(summary) = file.get("summary") else {
        return vec![format!("`{required_file}` coverage row is missing summary")];
    };

    let uncovered_lines = uncovered_source_lines(file);
    let uncovered_functions = summary_count(summary, "functions", "notcovered");
    match uncovered_lines {
        Some(lines) if lines.is_empty() => {}
        Some(lines) => diagnostics.push(format!(
            "`{required_file}` must have 100% source-line coverage; uncovered lines = {}",
            render_lines(&lines)
        )),
        None => diagnostics.push(format!(
            "`{required_file}` coverage row is missing detailed source segments"
        )),
    }
    if uncovered_functions != Some(0) {
        diagnostics.push(format!(
            "`{required_file}` must have 100% function coverage; uncovered functions = {}",
            render_count(uncovered_functions)
        ));
    }
    diagnostics
}

/// Returns source lines with explicit zero-count coverage segments.
fn uncovered_source_lines(file: &Value) -> Option<Vec<u64>> {
    let mut lines = file
        .get("segments")?
        .as_array()?
        .iter()
        .filter_map(|segment| {
            let segment = segment.as_array()?;
            let line = segment.first()?.as_u64()?;
            let count = segment.get(2)?.as_u64()?;
            let has_count = segment.get(3)?.as_bool()?;
            (has_count && count == 0).then_some(line)
        })
        .collect::<Vec<_>>();
    lines.sort_unstable();
    lines.dedup();
    Some(lines)
}

/// Reads one integer counter from a coverage summary object.
fn summary_count(summary: &Value, group: &str, field: &str) -> Option<u64> {
    let group = summary.get(group)?;
    if let Some(value) = group.get(field).and_then(Value::as_u64) {
        return Some(value);
    }
    if field == "notcovered" {
        let count = group.get("count")?.as_u64()?;
        let covered = group.get("covered")?.as_u64()?;
        return count.checked_sub(covered);
    }
    None
}

/// Renders an optional counter for stable diagnostics.
fn render_count(count: Option<u64>) -> String {
    count.map_or_else(|| "missing".to_string(), |count| count.to_string())
}

/// Renders uncovered source lines for stable diagnostics.
fn render_lines(lines: &[u64]) -> String {
    lines
        .iter()
        .map(u64::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Normalizes paths emitted through nested `#[path]` Rust module layouts.
fn normalize_coverage_path(path: &str) -> String {
    let mut parts = Vec::new();
    let normalized = path.replace('\\', "/");
    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part),
        }
    }
    parts.join("/")
}

#[cfg(test)]
#[path = "vm_coverage_100_test.rs"]
mod vm_coverage_100_test;
