use std::fs;
use std::path::Path;

use serde::Serialize;

use super::style::TestOutputStyle;
use super::DiscoveredTest;

/// Serializable source-level test manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TestManifest {
    source_path: String,
    module_name: String,
    target: String,
    target_profile: String,
    tests: Vec<TestManifestEntry>,
}

/// Serializable metadata for one discovered test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TestManifestEntry {
    name: String,
    span_start: usize,
    span_end: usize,
}

/// Serializable test execution result manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TestResultManifest {
    source_path: String,
    module_name: String,
    target: String,
    target_profile: String,
    passed: usize,
    failed: usize,
    tests: Vec<TestResultManifestEntry>,
}

/// Serializable execution result for one discovered test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TestResultManifestEntry {
    name: String,
    status: String,
    message: Option<String>,
    span_start: usize,
    span_end: usize,
}

/// In-memory execution report for a test run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestRunReport {
    pub(super) passed: usize,
    pub(super) failed: usize,
    pub(super) results: Vec<TestRunResult>,
}

/// In-memory execution result for one test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TestRunResult {
    pub(super) name: String,
    pub(super) status: TestRunStatus,
    pub(super) message: Option<String>,
    pub(super) span_start: usize,
    pub(super) span_end: usize,
}

/// Stable status labels for test result artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TestRunStatus {
    Passed,
    Failed,
}

impl TestRunStatus {
    /// Returns the manifest label for this status.
    ///
    /// Inputs:
    /// - `self`: status value to serialize.
    ///
    /// Output:
    /// - Stable lowercase status label.
    ///
    /// Transformation:
    /// - Maps enum variants to the public result-manifest vocabulary.
    fn as_str(self) -> &'static str {
        match self {
            TestRunStatus::Passed => "passed",
            TestRunStatus::Failed => "failed",
        }
    }
}

impl TestRunReport {
    /// Returns whether all tests passed.
    ///
    /// Inputs:
    /// - `self`: completed execution report.
    ///
    /// Output:
    /// - `true` when no test failed.
    ///
    /// Transformation:
    /// - Checks the aggregate failed count without inspecting stdout.
    pub(super) fn is_success(&self) -> bool {
        self.failed == 0
    }
}

/// Builds a validation-only pass report for JS tests.
///
/// Inputs:
/// - `tests`: discovered test metadata already accepted by test discovery.
///
/// Output:
/// - A `TestRunReport` with every test marked passed.
///
/// Transformation:
/// - Converts source-level test metadata into result entries with a stable
///   message that distinguishes compile/discovery validation from runtime
///   JavaScript execution.
pub(super) fn validation_pass_report(tests: &[DiscoveredTest]) -> TestRunReport {
    TestRunReport {
        passed: tests.len(),
        failed: 0,
        results: tests
            .iter()
            .map(|test| TestRunResult {
                name: test.name.clone(),
                status: TestRunStatus::Passed,
                message: Some("validated without runtime execution".to_string()),
                span_start: test.span_start,
                span_end: test.span_end,
            })
            .collect(),
    }
}

/// Prints a validation-only test report.
///
/// Inputs:
/// - `report`: validation-only result report for one JS test module.
/// - `style`: terminal color policy for pass/fail labels.
///
/// Output:
/// - Human-readable test status lines written to stdout.
///
/// Transformation:
/// - Renders the same compact shape as the Erlang runner while adding
///   `(validated)` to make the non-runtime status explicit.
pub(super) fn print_validation_pass_report(report: &TestRunReport, style: TestOutputStyle) {
    println!("running {} tests", report.results.len());
    for result in &report.results {
        println!(
            "test {} ... {} (validated)",
            result.name,
            style.success("ok")
        );
    }
    println!(
        "test result: {}. {} passed; 0 failed",
        style.success("ok"),
        report.passed
    );
}

/// Builds a report for literal boolean tests.
///
/// Inputs:
/// - `tests`: discovered tests whose bodies were classified as literal bools.
///
/// Output:
/// - `TestRunReport` with `true` tests passed and `false` tests failed.
///
/// Transformation:
/// - Converts syntax-known literal results into the same report shape used by
///   runtime backends, without compiling target artifacts.
pub(super) fn literal_bool_report(tests: &[DiscoveredTest]) -> TestRunReport {
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut results = Vec::new();
    for test in tests {
        let did_pass = test.literal_bool_result.unwrap_or(false);
        if did_pass {
            passed += 1;
            results.push(TestRunResult {
                name: test.name.clone(),
                status: TestRunStatus::Passed,
                message: Some("validated literal true without backend execution".to_string()),
                span_start: test.span_start,
                span_end: test.span_end,
            });
        } else {
            failed += 1;
            results.push(TestRunResult {
                name: test.name.clone(),
                status: TestRunStatus::Failed,
                message: Some("literal false".to_string()),
                span_start: test.span_start,
                span_end: test.span_end,
            });
        }
    }
    TestRunReport {
        passed,
        failed,
        results,
    }
}

/// Prints a validation-only literal boolean report.
///
/// Inputs:
/// - `report`: completed literal bool report.
/// - `style`: terminal color policy for pass/fail labels.
///
/// Output:
/// - Human-readable test runner output on stdout.
///
/// Transformation:
/// - Mirrors runtime runner output while marking passing tests as validated
///   instead of target-executed.
pub(super) fn print_literal_bool_report(report: &TestRunReport, style: TestOutputStyle) {
    println!("running {} tests", report.results.len());
    for result in &report.results {
        match result.status {
            TestRunStatus::Passed => {
                println!(
                    "test {} ... {} (validated)",
                    result.name,
                    style.success("ok")
                );
            }
            TestRunStatus::Failed => {
                println!("test {} ... {}", result.name, style.failure("FAILED"));
                if let Some(message) = result.message.as_deref() {
                    println!("  {message}");
                }
            }
        }
    }
    if report.failed == 0 {
        println!(
            "test result: {}. {} passed; 0 failed",
            style.success("ok"),
            report.passed
        );
    } else {
        println!(
            "test result: {}. {} passed; {} failed",
            style.failure("FAILED"),
            report.passed,
            report.failed
        );
    }
}

/// Writes a source-level test manifest.
///
/// Inputs:
/// - `manifest_path`: filesystem path for the JSON manifest.
/// - `source_path`: user-provided source file path.
/// - `module_name`: parsed Terlan module name.
/// - `target`: selected test runner target.
/// - `target_profile`: selected compiler target profile.
/// - `tests`: discovered source-level tests with spans.
///
/// Output:
/// - `Ok(())` when deterministic JSON is written.
/// - `Err(message)` when serialization, parent-directory creation, or file
///   writing fails.
///
/// Transformation:
/// - Converts discovered test metadata into a stable JSON artifact for release
///   gates and downstream runner integrations.
pub(super) fn write_test_manifest(
    manifest_path: &Path,
    source_path: &str,
    module_name: &str,
    target: &str,
    target_profile: &str,
    tests: &[DiscoveredTest],
) -> Result<(), String> {
    if let Some(parent) = manifest_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create test manifest directory {}: {err}",
                parent.display()
            )
        })?;
    }
    let manifest = TestManifest {
        source_path: source_path.to_string(),
        module_name: module_name.to_string(),
        target: target.to_string(),
        target_profile: target_profile.to_string(),
        tests: tests
            .iter()
            .map(|test| TestManifestEntry {
                name: test.name.clone(),
                span_start: test.span_start,
                span_end: test.span_end,
            })
            .collect(),
    };
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|err| format!("failed to serialize test manifest: {err}"))?;
    fs::write(manifest_path, format!("{json}\n")).map_err(|err| {
        format!(
            "failed to write test manifest {}: {err}",
            manifest_path.display()
        )
    })
}

/// Writes a source-level test execution result manifest.
///
/// Inputs:
/// - `manifest_path`: filesystem path for the JSON manifest.
/// - `source_path`: user-provided source file path.
/// - `module_name`: parsed Terlan module name.
/// - `target`: selected test runner target.
/// - `target_profile`: selected compiler target profile.
/// - `report`: direct BEAM execution report.
///
/// Output:
/// - `Ok(())` when deterministic JSON is written.
/// - `Err(message)` when serialization, parent-directory creation, or file
///   writing fails.
///
/// Transformation:
/// - Converts runtime pass/fail data into a stable JSON artifact for release
///   tooling and runner integrations.
pub(super) fn write_test_result_manifest(
    manifest_path: &Path,
    source_path: &str,
    module_name: &str,
    target: &str,
    target_profile: &str,
    report: &TestRunReport,
) -> Result<(), String> {
    if let Some(parent) = manifest_path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "failed to create test result manifest directory {}: {err}",
                parent.display()
            )
        })?;
    }
    let manifest = TestResultManifest {
        source_path: source_path.to_string(),
        module_name: module_name.to_string(),
        target: target.to_string(),
        target_profile: target_profile.to_string(),
        passed: report.passed,
        failed: report.failed,
        tests: report
            .results
            .iter()
            .map(|result| TestResultManifestEntry {
                name: result.name.clone(),
                status: result.status.as_str().to_string(),
                message: result.message.clone(),
                span_start: result.span_start,
                span_end: result.span_end,
            })
            .collect(),
    };
    let json = serde_json::to_string_pretty(&manifest)
        .map_err(|err| format!("failed to serialize test result manifest: {err}"))?;
    fs::write(manifest_path, format!("{json}\n")).map_err(|err| {
        format!(
            "failed to write test result manifest {}: {err}",
            manifest_path.display()
        )
    })
}
