use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

/// Verifies a complete JS type emission contract fixture passes.
///
/// Inputs:
/// - Type mapping manifest with all required mapping categories.
/// - Generated bindings manifest referencing source, summary, and contract
///   test artifacts.
/// - Skipped declaration manifest with stable `ts_bindgen.*` reasons.
///
/// Output:
/// - Summary counts for mapping categories, generated outputs, and skipped
///   declarations.
///
/// Transformation:
/// - Proves the quality gate accepts the intended committed-manifest shape.
#[test]
fn js_type_emission_contract_accepts_complete_fixture() {
    let root = temp_repo("js_type_emission_contract_accepts");
    write_complete_fixture(&root);

    let summary = run_js_type_emission_contract(&root).expect("fixture should pass");

    assert_eq!(
        summary.mapping_category_count,
        REQUIRED_TYPE_MAPPING_CATEGORIES.len()
    );
    assert_eq!(summary.generated_output_count, 1);
    assert_eq!(summary.skipped_declaration_count, 1);
}

/// Verifies missing required mapping categories are rejected.
///
/// Inputs:
/// - Type mapping manifest omitting `promise`.
///
/// Output:
/// - Diagnostic naming the missing category.
///
/// Transformation:
/// - Prevents generator changes from silently dropping a promised JS mapping
///   family.
#[test]
fn js_type_emission_contract_rejects_missing_mapping_category() {
    let root = temp_repo("js_type_emission_contract_missing_category");
    write_complete_fixture(&root);
    write(
        &root,
        TYPE_MAPPING_MANIFEST,
        &mapping_manifest_except("promise"),
    );

    let error = run_js_type_emission_contract(&root).expect_err("fixture should fail");

    assert!(error.contains("missing required mapping category `promise`"));
}

/// Verifies generated source files must declare their promised module.
///
/// Inputs:
/// - Bindings manifest that promises `std.js.Array`.
/// - Source file declaring a different module.
///
/// Output:
/// - Diagnostic naming the stale generated source.
///
/// Transformation:
/// - Locks generated source references to the manifest surface.
#[test]
fn js_type_emission_contract_rejects_stale_generated_source_module() {
    let root = temp_repo("js_type_emission_contract_stale_source");
    write_complete_fixture(&root);
    write(
        &root,
        "std/js/Array.terl",
        "module std.js.NotArray.\n\npub opaque type Array[T].\n",
    );

    let error = run_js_type_emission_contract(&root).expect_err("fixture should fail");

    assert!(error.contains("generated source does not declare `module std.js.Array.`"));
}

/// Verifies generated contracts must not be executable `@test` functions.
///
/// Inputs:
/// - Generated test artifact whose `generated_surface_contract` is annotated
///   with `@test`.
///
/// Output:
/// - Diagnostic explaining generated contracts stay outside test accounting.
///
/// Transformation:
/// - Keeps generated interface conformance separate from executable std
///   behavior coverage.
#[test]
fn js_type_emission_contract_rejects_generated_contract_marked_test() {
    let root = temp_repo("js_type_emission_contract_contract_test");
    write_complete_fixture(&root);
    write(
        &root,
        "std/js/ArrayTest.terl",
        "module std.js.ArrayTest.\n\n@test\npub generated_surface_contract(): Bool ->\n    true.\n",
    );

    let error = run_js_type_emission_contract(&root).expect_err("fixture should fail");

    assert!(error.contains("must not be annotated with `@test`"));
}

/// Verifies skipped TypeScript declarations require stable reasons.
///
/// Inputs:
/// - Skipped declaration manifest with a reason outside the `ts_bindgen.*`
///   namespace.
///
/// Output:
/// - Diagnostic naming the required reason prefix.
///
/// Transformation:
/// - Prevents unsupported TypeScript shapes from being silently classified by
///   informal free text.
#[test]
fn js_type_emission_contract_rejects_unstable_skip_reason() {
    let root = temp_repo("js_type_emission_contract_bad_skip");
    write_complete_fixture(&root);
    write(
        &root,
        SKIPPED_DECLARATIONS_MANIFEST,
        "{\n  \"schema\": \"terlan.std.js.skipped-declarations.v1\",\n  \"skipped\": [\n    { \"source\": \"std.js.eval\", \"reason\": \"unsupported\", \"detail\": \"top-level functions are not emitted yet\" }\n  ]\n}\n",
    );

    let error = run_js_type_emission_contract(&root).expect_err("fixture should fail");

    assert!(error.contains("reason must start with `ts_bindgen.`"));
}

/// Verifies skipped TypeScript declarations reject placeholder details.
///
/// Inputs:
/// - Skipped declaration manifest with a stable reason but placeholder detail.
///
/// Output:
/// - Diagnostic requiring non-placeholder detail metadata.
///
/// Transformation:
/// - Prevents unsupported TypeScript shapes from being parked behind fake
///   explanatory text.
#[test]
fn js_type_emission_contract_rejects_placeholder_skip_detail() {
    let root = temp_repo("js_type_emission_contract_placeholder_skip");
    write_complete_fixture(&root);
    write(
        &root,
        SKIPPED_DECLARATIONS_MANIFEST,
        "{\n  \"schema\": \"terlan.std.js.skipped-declarations.v1\",\n  \"skipped\": [\n    { \"source\": \"std.js.eval\", \"reason\": \"ts_bindgen.unsupported_top_level_function\", \"detail\": \"fixme before release\" }\n  ]\n}\n",
    );

    let error = run_js_type_emission_contract(&root).expect_err("fixture should fail");

    assert!(error.contains("missing `detail`"));
}

/// Writes a complete generated JS contract fixture.
fn write_complete_fixture(root: &Path) {
    write(root, TYPE_MAPPING_MANIFEST, &mapping_manifest_except(""));
    write(
        root,
        GENERATED_BINDINGS_MANIFEST,
        "{\n  \"outputs\": [\n    {\n      \"module\": \"std.js.Array\",\n      \"source\": \"std/js/Array.terl\",\n      \"summary\": \"std/summaries/std.js.Array.typi\",\n      \"test\": \"std/js/ArrayTest.terl\"\n    }\n  ]\n}\n",
    );
    write(
        root,
        SKIPPED_DECLARATIONS_MANIFEST,
        "{\n  \"schema\": \"terlan.std.js.skipped-declarations.v1\",\n  \"skipped\": [\n    { \"source\": \"std.js.eval\", \"reason\": \"ts_bindgen.unsupported_top_level_function\", \"detail\": \"top-level functions are not emitted yet\" }\n  ]\n}\n",
    );
    write(
        root,
        "std/js/Array.terl",
        "module std.js.Array.\n\npub opaque type Array[T].\n",
    );
    write(
        root,
        "std/summaries/std.js.Array.typi",
        "module std.js.Array.\n",
    );
    write(
        root,
        "std/js/ArrayTest.terl",
        "module std.js.ArrayTest.\n\npub generated_surface_contract(): Bool ->\n    true.\n",
    );
}

/// Builds a compact type mapping manifest, optionally omitting one category.
fn mapping_manifest_except(omit: &str) -> String {
    let mut text =
        String::from("{\n  \"schema\": \"terlan.std.js.type-mapping.v1\",\n  \"categories\": [\n");
    let mut first = true;
    for category in REQUIRED_TYPE_MAPPING_CATEGORIES {
        if *category == omit {
            continue;
        }
        if !first {
            text.push_str(",\n");
        }
        first = false;
        let status = if *category == "unsupported-shape" {
            "unsupported"
        } else {
            "supported"
        };
        text.push_str(&format!(
            "    {{ \"id\": \"{category}\", \"typescript_shape\": \"{category}\", \"terlan_surface\": \"std.js\", \"status\": \"{status}\""
        ));
        if status == "unsupported" {
            text.push_str(", \"unsupported_policy\": \"recorded in std_js_skipped.json\"");
        }
        text.push_str(" }");
    }
    text.push_str("\n  ]\n}\n");
    text
}

/// Writes one fixture file, creating parents first.
fn write(root: &Path, path: &str, text: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().expect("fixture path has parent")).expect("create parent");
    fs::write(path, text).expect("write fixture");
}

/// Creates an isolated temporary repository fixture directory.
fn temp_repo(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("{name}_{}_{}", std::process::id(), nanos));
    fs::create_dir_all(&path).expect("create temp repo");
    path
}
