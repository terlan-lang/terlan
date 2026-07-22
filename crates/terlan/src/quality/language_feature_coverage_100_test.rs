use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

/// Verifies the language feature coverage gate accepts a complete fixture.
///
/// Inputs:
/// - A matrix with every required language feature row.
/// - Positive Terlan test anchors for VM-supported rows.
/// - Source and adversarial references for every row.
///
/// Output:
/// - Summary counts proving the fixture is accepted.
///
/// Transformation:
/// - Locks the gate contract without depending on repository-global language
///   files.
#[test]
fn language_feature_coverage_100_accepts_complete_fixture() {
    let root = temp_repo("language_feature_coverage_accepts");
    write_fixture(&root);

    let summary = run_language_feature_coverage_100(&root).expect("complete fixture should pass");

    assert_eq!(summary.feature_count, REQUIRED_FEATURES.len());
    assert_eq!(summary.positive_test_count, supported_feature_count());
    assert_eq!(
        summary.adversarial_reference_count,
        REQUIRED_FEATURES.len() - supported_feature_count()
    );
    assert_eq!(summary.source_fragment_count, REQUIRED_FEATURES.len());
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies missing required feature rows are rejected.
///
/// Inputs:
/// - A matrix with the `lambda_inline` row removed.
///
/// Output:
/// - Diagnostic naming the missing feature.
///
/// Transformation:
/// - Prevents new feature inventory edits from silently losing a required
///   language feature row.
#[test]
fn language_feature_coverage_100_rejects_missing_feature() {
    let root = temp_repo("language_feature_coverage_missing_feature");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(LANGUAGE_FEATURE_MATRIX)).expect("matrix");
    let mut matrix: serde_json::Value = serde_json::from_str(&text).expect("matrix JSON");
    matrix["features"]
        .as_array_mut()
        .expect("features")
        .retain(|feature| {
            feature.get("id").and_then(serde_json::Value::as_str) != Some("lambda_inline")
        });
    write(
        &root,
        LANGUAGE_FEATURE_MATRIX,
        &serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    );

    let error = run_language_feature_coverage_100(&root).expect_err("missing feature should fail");

    assert!(error.contains("missing feature `lambda_inline`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies stale positive test references are rejected.
///
/// Inputs:
/// - A supported feature row that references a missing `@test` function.
///
/// Output:
/// - Diagnostic naming the stale positive test.
///
/// Transformation:
/// - Keeps supported feature rows tied to executable Terlan tests.
#[test]
fn language_feature_coverage_100_rejects_missing_positive_test() {
    let root = temp_repo("language_feature_coverage_missing_positive");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(LANGUAGE_FEATURE_MATRIX)).expect("matrix");
    let mut matrix: serde_json::Value = serde_json::from_str(&text).expect("matrix JSON");
    matrix["features"][0]["positive_tests"][0] =
        serde_json::Value::String("missing_language_feature_test".to_string());
    write(
        &root,
        LANGUAGE_FEATURE_MATRIX,
        &serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    );

    let error = run_language_feature_coverage_100(&root).expect_err("missing test should fail");

    assert!(error.contains("missing positive test `missing_language_feature_test`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies non-supported rows require diagnostics and adversarial references.
///
/// Inputs:
/// - A partial `receiver_methods` row with no rejection contract.
///
/// Output:
/// - Diagnostics naming the missing adversarial reference and diagnostic code.
///
/// Transformation:
/// - Prevents parsed-but-not-executable language shapes from becoming
///   undocumented VM gaps.
#[test]
fn language_feature_coverage_100_rejects_partial_feature_without_contract() {
    let root = temp_repo("language_feature_coverage_missing_adversarial");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(LANGUAGE_FEATURE_MATRIX)).expect("matrix");
    let mut matrix: serde_json::Value = serde_json::from_str(&text).expect("matrix JSON");
    for feature in matrix["features"].as_array_mut().expect("features") {
        if feature.get("id").and_then(serde_json::Value::as_str) == Some("receiver_methods") {
            feature["adversarial_tests"] = serde_json::Value::Array(Vec::new());
            feature["diagnostic"] = serde_json::Value::Null;
        }
    }
    write(
        &root,
        LANGUAGE_FEATURE_MATRIX,
        &serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    );

    let error = run_language_feature_coverage_100(&root).expect_err("partial row should fail");

    assert!(error.contains("non-supported feature `receiver_methods` must list adversarial tests"));
    assert!(error.contains("non-supported feature `receiver_methods` must declare a diagnostic"));
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
fn language_feature_coverage_100_rejects_stale_source_fragment() {
    let root = temp_repo("language_feature_coverage_stale_source");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(LANGUAGE_FEATURE_MATRIX)).expect("matrix");
    let mut matrix: serde_json::Value = serde_json::from_str(&text).expect("matrix JSON");
    matrix["features"][0]["source_fragments"][0] =
        serde_json::Value::String("src/features.rs::missing_anchor".to_string());
    write(
        &root,
        LANGUAGE_FEATURE_MATRIX,
        &serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    );

    let error = run_language_feature_coverage_100(&root).expect_err("stale source should fail");

    assert!(error.contains("source fragment reference `src/features.rs::missing_anchor`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Writes a complete minimal fixture for the gate.
fn write_fixture(root: &Path) {
    let rows = REQUIRED_FEATURES
        .iter()
        .map(|feature| feature_row(feature))
        .collect::<Vec<_>>()
        .join(",\n");
    write(
        root,
        LANGUAGE_FEATURE_MATRIX,
        &format!(
            r#"{{
  "schema": "terlan.language-feature-coverage.v1",
  "positive_test_files": [
    "tests/language/LanguageFeatureCoverageTest.terl"
  ],
  "features": [
{rows}
  ]
}}
"#
        ),
    );

    let test_body = REQUIRED_FEATURES
        .iter()
        .filter(|feature| is_supported_feature(feature))
        .map(|feature| {
            format!(
                "@test\npub {feature}_executes(): Bool ->\n    true.\n",
                feature = feature
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    write(
        root,
        "tests/language/LanguageFeatureCoverageTest.terl",
        &format!("module tests.language.LanguageFeatureCoverageTest.\n\n{test_body}"),
    );
    write(
        root,
        "src/features.rs",
        "supported_anchor\nunsupported_anchor\n",
    );
    write(root, "tests/adversarial.rs", "unsupported_anchor\n");
}

/// Returns one feature matrix row for a fixture feature.
fn feature_row(feature: &str) -> String {
    let supported = is_supported_feature(feature);
    let positive_tests = if supported {
        format!(r#""{feature}_executes""#)
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
    let vm = if supported { "supported" } else { "partial" };
    format!(
        r#"    {{
      "id": "{feature}",
      "feature": "{feature}",
      "parse": "supported",
      "format": "supported",
      "typecheck": "supported",
      "core_ir": "supported",
      "target_profile": "supported",
      "vm": "{vm}",
      "js": "supported",
      "lsp": "supported",
      "diagnostic": {diagnostic},
      "positive_tests": [{positive_tests}],
      "adversarial_tests": [{adversarial_tests}],
      "source_fragments": ["src/features.rs::supported_anchor"]
    }}"#
    )
}

/// Returns whether a fixture feature is currently VM-supported.
fn is_supported_feature(feature: &str) -> bool {
    !matches!(
        feature,
        "struct_declaration"
            | "receiver_methods"
            | "trait_impl_dispatch"
            | "pipe_forward"
            | "index_assignment"
            | "diagnostics"
    )
}

/// Returns the number of VM-supported fixture features.
fn supported_feature_count() -> usize {
    REQUIRED_FEATURES
        .iter()
        .filter(|feature| is_supported_feature(feature))
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
