use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

/// Verifies the pattern matrix gate accepts a complete fixture.
///
/// Inputs:
/// - A minimal matrix with all required families.
/// - A positive executable test anchor containing all referenced names.
/// - A Rust adversarial test file for unsupported rows.
///
/// Output:
/// - Summary counts proving the fixture is accepted.
///
/// Transformation:
/// - Locks the quality gate contract without depending on repository-global
///   pattern files.
#[test]
fn pattern_matching_support_accepts_complete_fixture() {
    let root = temp_repo("pattern_matching_support_accepts");
    write_fixture(&root);

    let summary = run_pattern_matching_support(&root).expect("complete fixture should pass");

    assert_eq!(summary.family_count, REQUIRED_FAMILIES.len());
    assert_eq!(summary.long_tail_context_count, LONG_TAIL_CONTEXTS.len());
    assert_eq!(
        summary.shape_synonym_context_count,
        SHAPE_SYNONYM_CONTEXTS.len()
    );
    assert_eq!(
        summary.positive_test_count,
        REQUIRED_FAMILIES.len() - 2 + SHAPE_SYNONYM_CONTEXTS.len()
    );
    assert_eq!(
        summary.adversarial_test_count,
        3 + SHAPE_SYNONYM_CONTEXTS.len()
    );
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies missing required families are rejected.
///
/// Inputs:
/// - A matrix with one required family removed.
///
/// Output:
/// - Diagnostic naming the missing family.
///
/// Transformation:
/// - Prevents syntax additions or matrix edits from silently losing a pattern
///   family row.
#[test]
fn pattern_matching_support_rejects_missing_family() {
    let root = temp_repo("pattern_matching_support_missing_family");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(PATTERN_MATRIX)).expect("matrix");
    let mut matrix: serde_json::Value = serde_json::from_str(&text).expect("matrix JSON");
    matrix["families"]
        .as_array_mut()
        .expect("families")
        .retain(|family| {
            family.get("id").and_then(serde_json::Value::as_str) != Some("bare_match_expression")
        });
    write(
        &root,
        PATTERN_MATRIX,
        &serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    );

    let error = run_pattern_matching_support(&root).expect_err("missing family should fail");

    assert!(error.contains("missing family `bare_match_expression`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies stale positive test references are rejected.
///
/// Inputs:
/// - A matrix that references a missing positive `@test` function.
///
/// Output:
/// - Diagnostic naming the stale test reference.
///
/// Transformation:
/// - Keeps the matrix tied to executable Terlan tests instead of prose.
#[test]
fn pattern_matching_support_rejects_missing_positive_test() {
    let root = temp_repo("pattern_matching_support_missing_positive");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(PATTERN_MATRIX)).expect("matrix");
    let mut matrix: serde_json::Value = serde_json::from_str(&text).expect("matrix JSON");
    matrix["families"][0]["positive_tests"][0] =
        serde_json::Value::String("missing_wildcard_test".to_string());
    write(
        &root,
        PATTERN_MATRIX,
        &serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    );

    let error = run_pattern_matching_support(&root).expect_err("missing test should fail");

    assert!(error.contains("missing positive test `missing_wildcard_test`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies positive test anchors can be split across multiple Terlan files.
///
/// Inputs:
/// - A matrix using `positive_test_files`.
/// - A row whose positive anchor exists only in the second listed file.
///
/// Output:
/// - Successful validation with the same row count as the complete fixture.
///
/// Transformation:
/// - Keeps long-tail pattern fixtures tied to the support matrix without
///   forcing every executable pattern test into one oversized file.
#[test]
fn pattern_matching_support_accepts_multiple_positive_test_files() {
    let root = temp_repo("pattern_matching_support_multiple_files");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(PATTERN_MATRIX)).expect("matrix");
    let mut matrix: serde_json::Value = serde_json::from_str(&text).expect("matrix JSON");
    matrix
        .as_object_mut()
        .expect("matrix object")
        .remove("positive_test_file");
    matrix["positive_test_files"] = serde_json::json!([
        "tests/pattern/PatternMatchingTest.terl",
        "tests/pattern/StringPatternLongTailTest.terl"
    ]);
    matrix["families"][0]["positive_tests"] = serde_json::json!(["long_tail_anchor"]);
    write(
        &root,
        PATTERN_MATRIX,
        &serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    );
    write(
        &root,
        "tests/pattern/StringPatternLongTailTest.terl",
        "module tests.pattern.StringPatternLongTailTest.\n\n@test\npub long_tail_anchor(): Bool ->\n    true.\n",
    );

    let summary = run_pattern_matching_support(&root).expect("multi-file fixture should pass");

    assert_eq!(summary.family_count, REQUIRED_FAMILIES.len());
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies positive support evidence can reference exact Rust test anchors.
///
/// Inputs:
/// - A matrix row whose positive evidence is a `path::test_name` reference.
///
/// Output:
/// - Successful validation.
///
/// Transformation:
/// - Allows parser/typechecker/runtime exact tests to serve as support evidence
///   alongside executable Terlan `@test` anchors.
#[test]
fn pattern_matching_support_accepts_positive_exact_test_reference() {
    let root = temp_repo("pattern_matching_support_positive_exact_reference");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(PATTERN_MATRIX)).expect("matrix");
    let mut matrix: serde_json::Value = serde_json::from_str(&text).expect("matrix JSON");
    matrix["families"][0]["positive_tests"] = serde_json::json!([
        "crates/terlan/src/compiler/syntax/parser_decl_test.rs::parses_typed_function_head_pattern_parameter"
    ]);
    write(
        &root,
        PATTERN_MATRIX,
        &serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    );
    write(
        &root,
        "crates/terlan/src/compiler/syntax/parser_decl_test.rs",
        "#[test]\nfn parses_typed_function_head_pattern_parameter() {}\n",
    );

    run_pattern_matching_support(&root).expect("positive exact reference should pass");
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies supported rows must declare at least one positive test.
///
/// Inputs:
/// - A matrix whose first VM-supported family has an empty positive test list.
///
/// Output:
/// - Diagnostic naming the missing positive coverage.
///
/// Transformation:
/// - Keeps VM-supported pattern families tied to executable Terlan coverage.
#[test]
fn pattern_matching_support_rejects_supported_family_without_positive_test() {
    let root = temp_repo("pattern_matching_support_empty_positive");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(PATTERN_MATRIX)).expect("matrix");
    let mut matrix: serde_json::Value = serde_json::from_str(&text).expect("matrix JSON");
    matrix["families"][0]["positive_tests"] = serde_json::Value::Array(Vec::new());
    write(
        &root,
        PATTERN_MATRIX,
        &serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    );

    let error = run_pattern_matching_support(&root).expect_err("empty positive should fail");

    assert!(error.contains("supported family `wildcard` must list positive tests"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies unsupported rows require adversarial coverage and a diagnostic.
///
/// Inputs:
/// - A matrix row for `record_struct` with no adversarial tests and no
///   diagnostic code.
///
/// Output:
/// - Diagnostics naming the missing rejection contract.
///
/// Transformation:
/// - Prevents unsupported pattern families from becoming undocumented gaps.
#[test]
fn pattern_matching_support_rejects_unsupported_family_without_adversarial_contract() {
    let root = temp_repo("pattern_matching_support_missing_adversarial");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(PATTERN_MATRIX)).expect("matrix");
    write(
        &root,
        PATTERN_MATRIX,
        &text.replace(
            &family_row(
                "record_struct",
                &[],
                &["crates/terlan/src/runtime/vm/patterns_test.rs::record_pattern_rejects"],
                "unsupported",
                "unsupported_vm_feature",
            ),
            &family_row("record_struct", &[], &[], "unsupported", ""),
        ),
    );

    let error = run_pattern_matching_support(&root).expect_err("unsupported row should fail");

    assert!(error.contains("unsupported family `record_struct` must list adversarial tests"));
    assert!(error.contains("unsupported family `record_struct` must declare a diagnostic code"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies the function-head pattern row keeps explicit JS rejection evidence.
///
/// Inputs:
/// - A matrix whose function-head pattern row omits the JS rejection diagnostic
///   and adversarial anchor.
///
/// Output:
/// - Diagnostics naming the missing cross-target contract.
///
/// Transformation:
/// - Prevents function-head parser/typecheck support from being mistaken for
///   JS backend support.
#[test]
fn pattern_matching_support_rejects_function_head_row_without_js_contract() {
    let root = temp_repo("pattern_matching_support_function_head_js_contract");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(PATTERN_MATRIX)).expect("matrix");
    write(
        &root,
        PATTERN_MATRIX,
        &text.replace(
            &family_row(
                "function_head_pattern_parameter",
                &["function_head_pattern_parameter_test".to_string()],
                &["crates/terlan/src/commands/build/build_test/tests/js_target_diagnostics_test.rs::build_command_rejects_function_head_pattern_for_js_target"],
                "supported",
                "target_profile_unsupported",
            ),
            &family_row(
                "function_head_pattern_parameter",
                &["function_head_pattern_parameter_test".to_string()],
                &[],
                "supported",
                "",
            ),
        ),
    );

    let error = run_pattern_matching_support(&root).expect_err("missing JS contract should fail");

    assert!(error.contains("must declare diagnostic `target_profile_unsupported`"));
    assert!(error.contains("must reference the JS target rejection anchor"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies the long-tail matrix cannot omit one user-facing capture context.
#[test]
fn pattern_matching_support_rejects_missing_long_tail_context() {
    let root = temp_repo("pattern_matching_support_missing_long_tail_context");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(PATTERN_MATRIX)).expect("matrix");
    let mut matrix: serde_json::Value = serde_json::from_str(&text).expect("matrix JSON");
    let family = matrix["families"]
        .as_array_mut()
        .expect("families")
        .iter_mut()
        .find(|family| family["id"] == "string_pattern_long_tail")
        .expect("long-tail family");
    family["contexts"]
        .as_array_mut()
        .expect("contexts")
        .retain(|context| context["id"] != "template");
    write(
        &root,
        PATTERN_MATRIX,
        &serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    );

    let error = run_pattern_matching_support(&root).expect_err("missing context should fail");

    assert!(error.contains("missing string-pattern context `template`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies blocked semantic contexts carry an executable rejection contract.
#[test]
fn pattern_matching_support_rejects_unexplained_blocked_long_tail_context() {
    let root = temp_repo("pattern_matching_support_unexplained_blocked_context");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(PATTERN_MATRIX)).expect("matrix");
    let mut matrix: serde_json::Value = serde_json::from_str(&text).expect("matrix JSON");
    let family = matrix["families"]
        .as_array_mut()
        .expect("families")
        .iter_mut()
        .find(|family| family["id"] == "string_pattern_long_tail")
        .expect("long-tail family");
    let shape = family["contexts"]
        .as_array_mut()
        .expect("contexts")
        .iter_mut()
        .find(|context| context["id"] == "shape")
        .expect("shape context");
    shape["diagnostic"] = serde_json::Value::Null;
    shape["adversarial_tests"] = serde_json::json!([]);
    write(
        &root,
        PATTERN_MATRIX,
        &serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    );

    let error = run_pattern_matching_support(&root).expect_err("unexplained block should fail");

    assert!(error.contains("blocked string-pattern context `shape` must declare a diagnostic"));
    assert!(error.contains("blocked string-pattern context `shape` must list adversarial"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies Tree-sitter support cannot be asserted without corpus evidence.
#[test]
fn pattern_matching_support_rejects_long_tail_context_without_tree_sitter_anchor() {
    let root = temp_repo("pattern_matching_support_missing_tree_sitter_anchor");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(PATTERN_MATRIX)).expect("matrix");
    let mut matrix: serde_json::Value = serde_json::from_str(&text).expect("matrix JSON");
    let family = matrix["families"]
        .as_array_mut()
        .expect("families")
        .iter_mut()
        .find(|family| family["id"] == "string_pattern_long_tail")
        .expect("long-tail family");
    let route = family["contexts"]
        .as_array_mut()
        .expect("contexts")
        .iter_mut()
        .find(|context| context["id"] == "route")
        .expect("route context");
    route["positive_tests"] = serde_json::json!(["string_pattern_long_tail_test"]);
    write(
        &root,
        PATTERN_MATRIX,
        &serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    );

    let error = run_pattern_matching_support(&root).expect_err("missing anchor should fail");

    assert!(error.contains("context `route` claims Tree-sitter support without a corpus anchor"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies public docs cannot silently lose the inferred capture form.
#[test]
fn pattern_matching_support_rejects_missing_inferred_string_capture_docs() {
    let root = temp_repo("pattern_matching_support_missing_inferred_docs");
    write_fixture(&root);
    write_string_pattern_docs(&root, TYPED_STRING_CAPTURE_EXAMPLE);

    let error = run_pattern_matching_support(&root).expect_err("missing inferred docs should fail");

    assert!(error.contains("missing canonical inferred string-capture example"));
    assert!(error.contains("README.md"));
    assert!(error.contains("docs/grammar/README.md"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies public docs cannot silently lose the typed capture form.
#[test]
fn pattern_matching_support_rejects_missing_typed_string_capture_docs() {
    let root = temp_repo("pattern_matching_support_missing_typed_docs");
    write_fixture(&root);
    write_string_pattern_docs(&root, INFERRED_STRING_CAPTURE_EXAMPLE);

    let error = run_pattern_matching_support(&root).expect_err("missing typed docs should fail");

    assert!(error.contains("missing canonical typed string-capture example"));
    assert!(error.contains("README.md"));
    assert!(error.contains("docs/grammar/README.md"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies every required shape-synonym context remains explicit.
#[test]
fn pattern_matching_support_rejects_missing_shape_synonym_context() {
    let root = temp_repo("pattern_matching_support_missing_shape_context");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(PATTERN_MATRIX)).expect("matrix");
    let mut matrix: serde_json::Value = serde_json::from_str(&text).expect("matrix JSON");
    matrix["shape_synonyms"]["contexts"]
        .as_array_mut()
        .expect("shape contexts")
        .retain(|context| context["id"] != "tooling");
    write(
        &root,
        PATTERN_MATRIX,
        &serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    );

    let error = run_pattern_matching_support(&root).expect_err("missing context should fail");

    assert!(error.contains("missing shape-synonym context `tooling`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies each context carries executable rejection evidence.
#[test]
fn pattern_matching_support_rejects_shape_context_without_adversarial_evidence() {
    let root = temp_repo("pattern_matching_support_shape_context_without_adversarial");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(PATTERN_MATRIX)).expect("matrix");
    let mut matrix: serde_json::Value = serde_json::from_str(&text).expect("matrix JSON");
    matrix["shape_synonyms"]["contexts"][0]["adversarial_tests"] = serde_json::json!([]);
    write(
        &root,
        PATTERN_MATRIX,
        &serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    );

    let error = run_pattern_matching_support(&root).expect_err("missing evidence should fail");

    assert!(error.contains("shape-synonym context `local` must list adversarial evidence"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Verifies shape stage classifications use the closed vocabulary.
#[test]
fn pattern_matching_support_rejects_unknown_shape_context_stage() {
    let root = temp_repo("pattern_matching_support_unknown_shape_stage");
    write_fixture(&root);
    let text = fs::read_to_string(root.join(PATTERN_MATRIX)).expect("matrix");
    let mut matrix: serde_json::Value = serde_json::from_str(&text).expect("matrix JSON");
    matrix["shape_synonyms"]["contexts"][0]["vm"] = serde_json::json!("almost");
    write(
        &root,
        PATTERN_MATRIX,
        &serde_json::to_string_pretty(&matrix).expect("serialize matrix"),
    );

    let error = run_pattern_matching_support(&root).expect_err("unknown stage should fail");

    assert!(error.contains("shape-synonym context `local` has unsupported `vm` stage `almost`"));
    fs::remove_dir_all(root).expect("remove fixture");
}

/// Writes a complete minimal fixture for the gate.
fn write_fixture(root: &Path) {
    let mut rows = String::new();
    for family in REQUIRED_FAMILIES {
        let positives = if *family == "record_struct" || *family == "bare_match_expression" {
            vec![]
        } else {
            vec![format!("{family}_test")]
        };
        let adversarials = match *family {
            "function_head_pattern_parameter" => {
                vec!["crates/terlan/src/commands/build/build_test/tests/js_target_diagnostics_test.rs::build_command_rejects_function_head_pattern_for_js_target"]
            }
            "record_struct" => {
                vec!["crates/terlan/src/runtime/vm/patterns_test.rs::record_pattern_rejects"]
            }
            "bare_match_expression" => {
                vec!["crates/terlan/src/tests/reject_test.rs::bare_match_rejects"]
            }
            _ => vec![],
        };
        let vm = if positives.is_empty() {
            "unsupported"
        } else {
            "supported"
        };
        let diagnostic = match *family {
            "function_head_pattern_parameter" => "target_profile_unsupported",
            _ if positives.is_empty() => "unsupported_vm_feature",
            _ => "",
        };
        rows.push_str(&family_row(
            family,
            &positives,
            &adversarials,
            vm,
            diagnostic,
        ));
    }

    write(
        root,
        PATTERN_MATRIX,
        &format!(
            r#"{{
  "schema": "terlan.pattern-matching-support.v1",
  "positive_test_file": "tests/pattern/PatternMatchingTest.terl",
  "shape_synonyms": {{"contexts": {}}},
  "families": [
{}
  ]
}}
"#,
            shape_synonym_contexts_json(),
            rows.trim_start_matches(",\n")
        ),
    );

    let test_names = REQUIRED_FAMILIES
        .iter()
        .filter(|family| **family != "record_struct" && **family != "bare_match_expression")
        .map(|family| format!("@test\npub {family}_test(): Bool ->\n    true.\n"))
        .collect::<String>();
    write(
        root,
        "tests/pattern/PatternMatchingTest.terl",
        &format!("module tests.pattern.PatternMatchingTest.\n\n{test_names}"),
    );
    write(
        root,
        "crates/terlan/src/commands/build/build_test/tests/js_target_diagnostics_test.rs",
        "#[test]\nfn build_command_rejects_function_head_pattern_for_js_target() {}\n",
    );
    write(
        root,
        "crates/terlan/src/runtime/vm/patterns_test.rs",
        "#[test]\nfn record_pattern_rejects() {}\n",
    );
    write(
        root,
        "crates/terlan/src/tests/reject_test.rs",
        "#[test]\nfn bare_match_rejects() {}\n",
    );
    let shape_positive_tests = SHAPE_SYNONYM_CONTEXTS
        .iter()
        .map(|id| format!("@test\npub {id}_positive(): Bool -> true.\n"))
        .collect::<String>();
    write(
        root,
        "tests/pattern/ShapeSynonymTest.terl",
        &format!("module tests.pattern.ShapeSynonymTest.\n\n{shape_positive_tests}"),
    );
    let shape_adversarial_tests = SHAPE_SYNONYM_CONTEXTS
        .iter()
        .map(|id| format!("#[test]\nfn {id}_adversarial() {{}}\n"))
        .collect::<String>();
    write(
        root,
        "crates/terlan/src/compiler/syntax/syntax_output/shapes_test.rs",
        &shape_adversarial_tests,
    );
    write(root, TREE_SITTER_CORPUS, &LONG_TAIL_CONTEXTS.join("\n"));
    write_string_pattern_docs(
        root,
        &format!("{INFERRED_STRING_CAPTURE_EXAMPLE}\n{TYPED_STRING_CAPTURE_EXAMPLE}\n"),
    );
}

/// Builds the synthetic shape-synonym cross-surface contract used by gate tests.
fn shape_synonym_contexts_json() -> String {
    let contexts = SHAPE_SYNONYM_CONTEXTS
        .iter()
        .map(|id| {
            serde_json::json!({
                "id": id,
                "parse": "supported",
                "format": "supported",
                "typecheck": "supported",
                "core_ir": "supported",
                "vm": "supported",
                "js": "supported",
                "tree_sitter": "not-applicable",
                "lsp": "not-applicable",
                "docs": "not-applicable",
                "diagnostic": null,
                "positive_tests": [
                    format!("tests/pattern/ShapeSynonymTest.terl::{id}_positive")
                ],
                "adversarial_tests": [
                    format!("crates/terlan/src/compiler/syntax/syntax_output/shapes_test.rs::{id}_adversarial")
                ]
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&contexts).expect("serialize shape-synonym contexts")
}

/// Writes both public string-pattern documentation fixtures.
fn write_string_pattern_docs(root: &Path, text: &str) {
    for relative in STRING_PATTERN_DOCS {
        write(root, relative, text);
    }
}

/// Builds one JSON family row.
fn family_row(
    family: &str,
    positives: &[String],
    adversarials: &[&str],
    vm: &str,
    diagnostic: &str,
) -> String {
    let positives = positives
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let adversarials = adversarials
        .iter()
        .map(|name| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ");
    let diagnostic = if diagnostic.is_empty() {
        "null".to_string()
    } else {
        format!("\"{diagnostic}\"")
    };
    let semantic_stage = if family == "string_pattern_long_tail" {
        "partial"
    } else {
        "supported"
    };
    let contexts = if family == "string_pattern_long_tail" {
        format!(",\n      \"contexts\": {}", long_tail_contexts_json())
    } else {
        String::new()
    };
    let vm_stage = if family == "string_pattern_long_tail" {
        semantic_stage
    } else {
        vm
    };
    format!(
        r#",
    {{
      "id": "{family}",
      "surface": "{family}",
      "parse": "supported",
      "format": "supported",
      "typecheck": "{semantic_stage}",
      "core_ir": "{semantic_stage}",
      "target_profile": "supported",
      "vm": "{vm_stage}",
      "js": "unsupported",
      "diagnostic": {diagnostic},
      "positive_tests": [{positives}],
      "adversarial_tests": [{adversarials}]{contexts}
    }}"#
    )
}

/// Builds the synthetic six-context long-tail contract used by gate tests.
fn long_tail_contexts_json() -> String {
    let contexts = LONG_TAIL_CONTEXTS
        .iter()
        .map(|id| {
            let blocked = *id == "shape";
            serde_json::json!({
                "id": id,
                "parse": "supported",
                "typecheck": if blocked { "blocked" } else { "supported" },
                "core_ir": if blocked { "blocked" } else { "supported" },
                "vm": if blocked { "blocked" } else { "supported" },
                "js": "unsupported",
                "tree_sitter": "supported",
                "lsp": if blocked { "diagnostic-only" } else { "supported" },
                "stdlib_test": if blocked { "syntax-only" } else { "supported" },
                "diagnostic": if blocked { Some("shape_synonym_expansion_required") } else { None },
                "positive_tests": [
                    "string_pattern_long_tail_test",
                    format!("{TREE_SITTER_CORPUS}::{id}")
                ],
                "adversarial_tests": if blocked {
                    vec!["crates/terlan/src/runtime/vm/patterns_test.rs::record_pattern_rejects"]
                } else {
                    Vec::<&str>::new()
                }
            })
        })
        .collect::<Vec<_>>();
    serde_json::to_string(&contexts).expect("serialize long-tail contexts")
}

/// Writes text to a repository-relative path.
fn write(root: &Path, relative: &str, text: &str) {
    let path = root.join(relative);
    fs::create_dir_all(path.parent().expect("parent")).expect("create parent");
    fs::write(path, text).expect("write fixture");
}

/// Creates a unique temporary repository path.
fn temp_repo(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("terlan_{label}_{nanos}"));
    fs::create_dir_all(&root).expect("create temp repo");
    root
}
