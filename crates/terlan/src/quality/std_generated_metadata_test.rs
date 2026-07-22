use std::fs;
use std::path::PathBuf;

use super::run_std_generated_metadata;

fn generated_header(kind: &str) -> String {
    format!(
        "/**\n * @generated true\n * @do-not-edit true\n * @generator terlc\n * @generator-version 0.0.7\n * @generator-profile typescript-standard-js-dom\n * @artifact-kind {kind}\n * @input-manifest std/js/manifests/std_js_dom_inputs.json\n * @source-package typescript@5.9.3\n * @source-input std/js/fixtures/lib.es5.d.ts\n * @source-interface ArrayBuffer\n */\n\n"
    )
}

fn make_quality_temp_dir(name: &str) -> PathBuf {
    let mut root = std::env::temp_dir();
    root.push(format!(
        "terlan_quality_{name}_{}_{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
    ));
    if root.exists() {
        fs::remove_dir_all(&root).expect("remove stale temp dir");
    }
    fs::create_dir_all(&root).expect("create temp dir");
    root
}

#[test]
fn std_generated_metadata_accepts_complete_generated_provenance() {
    let root = make_quality_temp_dir("std_generated_metadata_accepts");
    fs::create_dir_all(root.join("std/js")).expect("create std js");
    fs::create_dir_all(root.join("std/summaries")).expect("create summaries");
    fs::write(
        root.join("std/js/ArrayBuffer.terl"),
        format!("{}module std.js.ArrayBuffer.\n", generated_header("source")),
    )
    .expect("write source");
    fs::write(
        root.join("std/summaries/std.js.ArrayBuffer.typi"),
        format!(
            "{}module std.js.ArrayBuffer.\n",
            generated_header("summary")
        ),
    )
    .expect("write summary");

    let summary = run_std_generated_metadata(&root).expect("complete metadata should pass");

    assert_eq!(summary.checked_file_count, 2);
    fs::remove_dir_all(root).expect("remove temp dir");
}

#[test]
fn std_generated_metadata_rejects_missing_and_duplicate_fields() {
    let root = make_quality_temp_dir("std_generated_metadata_rejects");
    fs::create_dir_all(root.join("std/js")).expect("create std js");
    fs::create_dir_all(root.join("std/summaries")).expect("create summaries");
    fs::write(
        root.join("std/js/ArrayBuffer.terl"),
        "/**\n * @generated true\n * @generated true\n * @do-not-edit true\n */\n\nmodule std.js.ArrayBuffer.\n",
    )
    .expect("write source");

    let error = run_std_generated_metadata(&root).expect_err("invalid metadata should fail");

    assert!(error.contains("std/js/ArrayBuffer.terl"));
    assert!(error.contains("appears 2 times"));
    assert!(error.contains("@generator"));
    assert!(error.contains("is missing"));
    fs::remove_dir_all(root).expect("remove temp dir");
}

#[test]
fn std_generated_metadata_accepts_generated_js_interfaces() {
    let root = make_quality_temp_dir("std_generated_metadata_rejects_terli");
    fs::create_dir_all(root.join("std/js")).expect("create std js");
    fs::create_dir_all(root.join("std/summaries")).expect("create summaries");
    fs::write(
        root.join("std/js/ArrayBuffer.terli"),
        format!(
            "{}module std.js.ArrayBuffer.\n",
            generated_header("interface")
        ),
    )
    .expect("write source");

    let summary = run_std_generated_metadata(&root).expect("generated terli should pass");

    assert_eq!(summary.checked_file_count, 1);
    fs::remove_dir_all(root).expect("remove temp dir");
}

#[test]
fn std_generated_metadata_rejects_mismatched_artifact_kind() {
    let root = make_quality_temp_dir("std_generated_metadata_kind_mismatch");
    fs::create_dir_all(root.join("std/js")).expect("create std js");
    fs::create_dir_all(root.join("std/summaries")).expect("create summaries");
    fs::write(
        root.join("std/js/ArrayBuffer.terli"),
        format!("{}module std.js.ArrayBuffer.\n", generated_header("source")),
    )
    .expect("write source");

    let error = run_std_generated_metadata(&root).expect_err("wrong artifact kind should fail");

    assert!(error.contains("generated artifact kind must be `interface`"));
    fs::remove_dir_all(root).expect("remove temp dir");
}
