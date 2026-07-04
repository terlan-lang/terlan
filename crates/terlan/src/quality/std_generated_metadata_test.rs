use std::fs;
use std::path::PathBuf;

use super::run_std_generated_metadata;

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
fn std_generated_metadata_accepts_minimal_generated_headers() {
    let root = make_quality_temp_dir("std_generated_metadata_accepts");
    fs::create_dir_all(root.join("std/js")).expect("create std js");
    fs::create_dir_all(root.join("std/summaries")).expect("create summaries");
    fs::write(
        root.join("std/js/ArrayBuffer.terl"),
        "/**\n * @generated true\n * @do-not-edit true\n */\n\nmodule std.js.ArrayBuffer.\n",
    )
    .expect("write source");
    fs::write(
        root.join("std/summaries/std.js.ArrayBuffer.typi"),
        "/**\n * @generated true\n * @do-not-edit true\n */\n\nmodule std.js.ArrayBuffer.\n",
    )
    .expect("write summary");

    let summary = run_std_generated_metadata(&root).expect("minimal metadata should pass");

    assert_eq!(summary.checked_file_count, 2);
    fs::remove_dir_all(root).expect("remove temp dir");
}

#[test]
fn std_generated_metadata_rejects_redundant_generated_header_fields() {
    let root = make_quality_temp_dir("std_generated_metadata_rejects");
    fs::create_dir_all(root.join("std/js")).expect("create std js");
    fs::create_dir_all(root.join("std/summaries")).expect("create summaries");
    fs::write(
        root.join("std/js/ArrayBuffer.terli"),
        "/**\n * @generated true\n * @do-not-edit true\n * @generator terlc\n * @generator-version 0.0.7\n * @input-manifest std/js/manifests/std_js_dom_inputs.json\n */\n\nmodule std.js.ArrayBuffer.\n",
    )
    .expect("write source");

    let error = run_std_generated_metadata(&root).expect_err("redundant metadata should fail");

    assert!(error.contains("std/js/ArrayBuffer.terli"));
    assert!(error.contains("@generator"));
    assert!(error.contains("@generator-version"));
    assert!(error.contains("@input-manifest"));
    fs::remove_dir_all(root).expect("remove temp dir");
}

#[test]
fn std_generated_metadata_rejects_generated_js_terli_mirrors() {
    let root = make_quality_temp_dir("std_generated_metadata_rejects_terli");
    fs::create_dir_all(root.join("std/js")).expect("create std js");
    fs::create_dir_all(root.join("std/summaries")).expect("create summaries");
    fs::write(
        root.join("std/js/ArrayBuffer.terli"),
        "/**\n * @generated true\n * @do-not-edit true\n */\n\nmodule std.js.ArrayBuffer.\n",
    )
    .expect("write source");

    let error = run_std_generated_metadata(&root).expect_err("generated terli should fail");

    assert!(error.contains("std/js/ArrayBuffer.terli"));
    assert!(error.contains("generated JS `.terli` mirror is redundant"));
    fs::remove_dir_all(root).expect("remove temp dir");
}
