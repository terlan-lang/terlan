use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

/// Verifies the operator coverage gate accepts a complete fixture.
///
/// Inputs:
/// - A matrix with every required operator row.
/// - Positive Terlan test anchors for VM-supported rows.
/// - Source and adversarial references for every row.
///
/// Output:
/// - Summary counts proving the fixture is accepted.
///
/// Transformation:
/// - Locks the gate contract without depending on repository-global operator
///   files.
#[test]
fn operator_coverage_100_accepts_complete_fixture() {
    let root = temp_repo("operator_coverage_accepts");
    write_fixture(&root);

    let summary = run_operator_coverage_100(&root).expect("complete fixture should pass");

    assert_eq!(summary.operator_count, REQUIRED_OPERATORS.len());
    assert_eq!(summary.positive_test_count, supported_operator_count());
    assert_eq!(
        summary.adversarial_reference_count,
        REQUIRED_OPERATORS.len() - supported_operator_count()
    );
    assert_eq!(summary.source_fragment_count, REQUIRED_OPERATORS.len());
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies missing required operator rows are rejected.
///
/// Inputs:
/// - A matrix with the `pipe_forward` row removed.
///
/// Output:
/// - Diagnostic naming the missing operator.
///
/// Transformation:
/// - Prevents syntax additions or matrix edits from silently losing operator
///   coverage.
#[test]
fn operator_coverage_100_rejects_missing_operator() {
    let root = temp_repo("operator_coverage_missing_operator");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(OPERATOR_MATRIX)).expect("matrix");
    let mut matrix: serde_json::Value = serde_json::from_str(&text).expect("matrix JSON");
    matrix["operators"]
        .as_array_mut()
        .expect("operators")
        .retain(|operator| {
            operator.get("id").and_then(serde_json::Value::as_str) != Some("pipe_forward")
        });
    write(
        &root,
        OPERATOR_MATRIX,
        &serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    );

    let error = run_operator_coverage_100(&root).expect_err("missing operator should fail");

    assert!(error.contains("missing operator `pipe_forward`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies stale positive test references are rejected.
///
/// Inputs:
/// - A supported operator row that references a missing `@test` function.
///
/// Output:
/// - Diagnostic naming the stale positive test.
///
/// Transformation:
/// - Keeps supported operator rows tied to executable Terlan tests.
#[test]
fn operator_coverage_100_rejects_missing_positive_test() {
    let root = temp_repo("operator_coverage_missing_positive");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(OPERATOR_MATRIX)).expect("matrix");
    let mut matrix: serde_json::Value = serde_json::from_str(&text).expect("matrix JSON");
    matrix["operators"][0]["positive_tests"][0] =
        serde_json::Value::String("missing_operator_test".to_string());
    write(
        &root,
        OPERATOR_MATRIX,
        &serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    );

    let error = run_operator_coverage_100(&root).expect_err("missing test should fail");

    assert!(error.contains("missing positive test `missing_operator_test`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies unsupported rows require diagnostics and adversarial references.
///
/// Inputs:
/// - An unsupported `rem_keyword` row with no rejection contract.
///
/// Output:
/// - Diagnostics naming the missing adversarial reference and diagnostic code.
///
/// Transformation:
/// - Prevents unsupported operators from becoming undocumented VM gaps.
#[test]
fn operator_coverage_100_rejects_unsupported_operator_without_contract() {
    let root = temp_repo("operator_coverage_missing_adversarial");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(OPERATOR_MATRIX)).expect("matrix");
    let mut matrix: serde_json::Value = serde_json::from_str(&text).expect("matrix JSON");
    for operator in matrix["operators"].as_array_mut().expect("operators") {
        if operator.get("id").and_then(serde_json::Value::as_str) == Some("rem_keyword") {
            operator["adversarial_tests"] = serde_json::Value::Array(Vec::new());
            operator["diagnostic"] = serde_json::Value::Null;
        }
    }
    write(
        &root,
        OPERATOR_MATRIX,
        &serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    );

    let error = run_operator_coverage_100(&root).expect_err("unsupported row should fail");

    assert!(error.contains("unsupported operator `rem_keyword` must list adversarial tests"));
    assert!(error.contains("unsupported operator `rem_keyword` must declare a diagnostic code"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies stale source fragments are rejected.
///
/// Inputs:
/// - A row whose source fragment needle no longer exists.
///
/// Output:
/// - Diagnostic naming the stale source reference.
///
/// Transformation:
/// - Keeps matrix source anchors synchronized with implementation code.
#[test]
fn operator_coverage_100_rejects_stale_source_fragment() {
    let root = temp_repo("operator_coverage_stale_source");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(OPERATOR_MATRIX)).expect("matrix");
    let mut matrix: serde_json::Value = serde_json::from_str(&text).expect("matrix JSON");
    matrix["operators"][0]["source_fragments"][0] =
        serde_json::Value::String("src/operators.rs::missing_anchor".to_string());
    write(
        &root,
        OPERATOR_MATRIX,
        &serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    );

    let error = run_operator_coverage_100(&root).expect_err("stale source should fail");

    assert!(error.contains("source fragment reference `src/operators.rs::missing_anchor`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Writes a complete minimal fixture for the gate.
fn write_fixture(root: &Path) {
    let rows = REQUIRED_OPERATORS
        .iter()
        .map(|operator| operator_row(operator))
        .collect::<Vec<_>>()
        .join(",\n");
    write(
        root,
        OPERATOR_MATRIX,
        &format!(
            r#"{{
  "schema": "terlan.operator-coverage.v1",
  "positive_test_files": [
    "tests/operator/OperatorCoverageTest.terl"
  ],
  "operators": [
{rows}
  ]
}}
"#
        ),
    );

    let test_body = REQUIRED_OPERATORS
        .iter()
        .filter(|operator| is_supported_operator(operator))
        .map(|operator| {
            format!(
                "@test\npub {operator}_executes(): Bool ->\n    true.\n",
                operator = operator
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    write(
        root,
        "tests/operator/OperatorCoverageTest.terl",
        &format!("module tests.operator.OperatorCoverageTest.\n\n{test_body}"),
    );
    write(
        root,
        "src/operators.rs",
        "supported_anchor\nunsupported_anchor\n",
    );
    write(root, "tests/adversarial.rs", "unsupported_anchor\n");
}

/// Returns one operator matrix row for a fixture operator.
fn operator_row(operator: &str) -> String {
    let supported = is_supported_operator(operator);
    let positive_tests = if supported {
        format!(r#""{operator}_executes""#)
    } else {
        String::new()
    };
    let adversarial_tests = if supported {
        String::new()
    } else {
        r#""tests/adversarial.rs::unsupported_anchor""#.to_string()
    };
    let diagnostic = if supported {
        "null".to_string()
    } else {
        r#""unsupported_vm_feature""#.to_string()
    };
    let parse = if operator.starts_with("deprecated_") {
        "rejected"
    } else {
        "supported"
    };
    let vm = if supported {
        "supported"
    } else if operator.starts_with("deprecated_") {
        "not-applicable"
    } else {
        "unsupported"
    };
    format!(
        r#"    {{
      "id": "{operator}",
      "spelling": "{operator}",
      "kind": "fixture",
      "parse": "{parse}",
      "format": "supported",
      "typecheck": "supported",
      "core_ir": "supported",
      "target_profile": "supported",
      "vm": "{vm}",
      "js": "supported",
      "diagnostic": {diagnostic},
      "positive_tests": [{positive_tests}],
      "adversarial_tests": [{adversarial_tests}],
      "source_fragments": ["src/operators.rs::supported_anchor"]
    }}"#
    )
}

/// Returns whether a fixture operator is currently VM-supported.
fn is_supported_operator(operator: &str) -> bool {
    !matches!(
        operator,
        "rem_keyword"
            | "bang_not"
            | "pipe_forward"
            | "index_access"
            | "index_assignment"
            | "deprecated_strict_eq"
            | "deprecated_slash_not_eq"
            | "deprecated_exact_not_eq"
    )
}

/// Returns the number of VM-supported fixture operators.
fn supported_operator_count() -> usize {
    REQUIRED_OPERATORS
        .iter()
        .filter(|operator| is_supported_operator(operator))
        .count()
}

/// Creates a unique temporary repository fixture path.
fn temp_repo(name: &str) -> PathBuf {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_millis();
    let path = std::env::temp_dir().join(format!("{name}_{millis}_{}", std::process::id()));
    fs::create_dir_all(&path).expect("create fixture root");
    path
}

/// Writes a fixture file, creating parent directories as needed.
fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create fixture parent");
    }
    fs::write(path, text).expect("write fixture file");
}
