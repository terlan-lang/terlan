use super::*;

/// Verifies coverage path normalization handles nested `#[path]` module output.
///
/// Inputs:
/// - A coverage path containing `../` segments.
///
/// Output:
/// - Repository-like path without the parent segment.
///
/// Transformation:
/// - Keeps matching stable for the standalone `terlan-vm` binary layout.
#[test]
fn coverage_path_normalization_removes_parent_segments() {
    let path = normalize_coverage_path("/repo/crates/terlan/src/vm/../runtime/vm/process.rs");

    assert_eq!(path, "repo/crates/terlan/src/runtime/vm/process.rs");
}

/// Verifies the coverage gate accepts a required file at 100%.
///
/// Inputs:
/// - Minimal cargo-llvm-cov JSON for `process.rs`.
///
/// Output:
/// - No diagnostics for the required file.
///
/// Transformation:
/// - Locks the stable line/function counters used by the release gate.
#[test]
fn coverage_summary_accepts_required_file_with_no_uncovered_lines_or_functions() {
    let report = coverage_report_json(&[], 0);
    let file = find_file_summary(&report, REQUIRED_VM_FILES[0]).expect("file summary");

    let diagnostics = validate_file_summary(REQUIRED_VM_FILES[0], file);

    assert_eq!(diagnostics, Vec::<String>::new());
}

/// Verifies uncovered source lines fail the VM coverage baseline.
///
/// Inputs:
/// - Minimal coverage JSON with one uncovered source segment.
///
/// Output:
/// - Diagnostic naming source-line coverage.
///
/// Transformation:
/// - Prevents the VM-owned process model baseline from silently regressing.
#[test]
fn coverage_summary_rejects_uncovered_lines() {
    let report = coverage_report_json(&[12], 0);
    let file = find_file_summary(&report, REQUIRED_VM_FILES[0]).expect("file summary");

    let diagnostics = validate_file_summary(REQUIRED_VM_FILES[0], file);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("uncovered lines = 12")),
        "expected uncovered line diagnostic: {diagnostics:?}"
    );
}

/// Verifies summary-only reports fail because they cannot prove source lines.
///
/// Inputs:
/// - Minimal coverage JSON without detailed segments.
///
/// Output:
/// - Diagnostic requesting detailed source segments.
///
/// Transformation:
/// - Prevents the Makefile gate from accidentally reverting to summary-only
///   llvm-cov output.
#[test]
fn coverage_summary_rejects_missing_detailed_segments() {
    let mut report = coverage_report_json(&[], 0);
    report["data"][0]["files"][0]
        .as_object_mut()
        .expect("file object")
        .remove("segments");
    let file = find_file_summary(&report, REQUIRED_VM_FILES[0]).expect("file summary");

    let diagnostics = validate_file_summary(REQUIRED_VM_FILES[0], file);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("missing detailed source segments")),
        "expected detailed segment diagnostic: {diagnostics:?}"
    );
}

/// Verifies uncovered functions fail the VM coverage baseline.
///
/// Inputs:
/// - Minimal coverage JSON with one uncovered function.
///
/// Output:
/// - Diagnostic naming function coverage.
///
/// Transformation:
/// - Protects VM process-model API coverage from accidental test removal.
#[test]
fn coverage_summary_rejects_uncovered_functions() {
    let report = coverage_report_json(&[], 1);
    let file = find_file_summary(&report, REQUIRED_VM_FILES[0]).expect("file summary");

    let diagnostics = validate_file_summary(REQUIRED_VM_FILES[0], file);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.contains("uncovered functions = 1")),
        "expected uncovered function diagnostic: {diagnostics:?}"
    );
}

/// Builds minimal cargo-llvm-cov JSON for tests.
fn coverage_report_json(uncovered_lines: &[u64], uncovered_functions: u64) -> Value {
    let mut segments = vec![serde_json::json!([1, 1, 1, true, true, false])];
    for line in uncovered_lines {
        segments.push(serde_json::json!([line, 1, 0, true, true, false]));
    }
    serde_json::json!({
        "data": [{
            "files": [{
                "filename": format!("/repo/{}", REQUIRED_VM_FILES[0]),
                "segments": segments,
                "summary": {
                    "lines": {
                        "count": 10,
                        "covered": 10 - uncovered_lines.len() as u64,
                        "notcovered": uncovered_lines.len() as u64,
                        "percent": if uncovered_lines.is_empty() { 100.0 } else { 90.0 }
                    },
                    "functions": {
                        "count": 4,
                        "covered": 4 - uncovered_functions,
                        "notcovered": uncovered_functions,
                        "percent": if uncovered_functions == 0 { 100.0 } else { 75.0 }
                    }
                }
            }]
        }]
    })
}
