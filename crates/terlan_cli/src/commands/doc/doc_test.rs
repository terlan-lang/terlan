use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::UNIX_EPOCH;

use super::run;
use crate::{CliCommand, CliState};

/// Verifies `doc --check` accepts matching REPL examples.
///
/// Inputs:
/// - Temporary Terlan source file with one runnable `@example` prompt.
///
/// Output:
/// - Successful command exit code.
///
/// Transformation:
/// - Runs the public doc command path so source parsing, documentation
///   validation, REPL prompt extraction, REPL execution, and output
///   comparison are all exercised together.
#[test]
fn doc_check_accepts_matching_repl_example() {
    let dir = make_doc_command_test_dir("matching_repl_example");
    let path = dir.join("DocExample.terl");
    fs::write(
        &path,
        r#"module doc_examples.

/**
 * Adds numbers.
 *
 * @example
 * > 1 + 2.
 * 3
 */
pub add(x: Int): Int ->
    x + 1.
"#,
    )
    .expect("write source");

    let exit = run(
        CliCommand {
            verb: Some("doc".to_string()),
            args: vec![path.to_string_lossy().to_string(), "--check".to_string()],
        },
        CliState::default(),
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    fs::remove_dir_all(dir).expect("remove test dir");
}

/// Verifies `doc --check` rejects mismatched REPL examples.
///
/// Inputs:
/// - Temporary Terlan source file with one runnable `@example` prompt whose
///   expected output is wrong.
///
/// Output:
/// - Failing command exit code.
///
/// Transformation:
/// - Runs the public doc command path and confirms the REPL-backed doctest
///   gate fails before documentation output is written.
#[test]
fn doc_check_rejects_mismatched_repl_example() {
    let dir = make_doc_command_test_dir("mismatched_repl_example");
    let path = dir.join("DocExample.terl");
    fs::write(
        &path,
        r#"module doc_examples.

/**
 * Adds numbers.
 *
 * @example
 * > 1 + 2.
 * 4
 */
pub add(x: Int): Int ->
    x + 1.
"#,
    )
    .expect("write source");

    let exit = run(
        CliCommand {
            verb: Some("doc".to_string()),
            args: vec![path.to_string_lossy().to_string(), "--check".to_string()],
        },
        CliState::default(),
    );

    assert_eq!(exit, ExitCode::from(1));
    fs::remove_dir_all(dir).expect("remove test dir");
}

/// Verifies `doc --format json` writes a compiler-owned JSON model.
///
/// Inputs:
/// - Temporary Terlan source file with one documented public function.
///
/// Output:
/// - Successful command exit code and parseable JSON documentation output.
///
/// Transformation:
/// - Runs the public doc command path with JSON format and parses the
///   generated artifact as the initial Terlan documentation model.
#[test]
fn doc_command_writes_json_model() {
    let dir = make_doc_command_test_dir("json_model");
    let path = dir.join("DocExample.terl");
    let out_dir = dir.join("docs");
    fs::write(
        &path,
        r#"module doc_examples.

/**
 * Adds numbers.
 */
pub add(x: Int): Int ->
    x + 1.
"#,
    )
    .expect("write source");

    let exit = run(
        CliCommand {
            verb: Some("doc".to_string()),
            args: vec![path.to_string_lossy().to_string()],
        },
        CliState {
            out_dir: out_dir.clone(),
            doc_format: crate::DocFormat::Json,
            ..CliState::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    let module_json =
        fs::read_to_string(out_dir.join("doc_examples.json")).expect("read module json docs");
    let module_value: serde_json::Value =
        serde_json::from_str(&module_json).expect("parse module docs json");
    assert_eq!(module_value["schema"], "terlan-doc-module-v1");
    assert_eq!(module_value["declarations"][0]["name"], "add");
    let project_json =
        fs::read_to_string(out_dir.join("model.json")).expect("read project docs json");
    let project_value: serde_json::Value =
        serde_json::from_str(&project_json).expect("parse project docs json");
    assert_eq!(project_value["schema"], "terlan-doc-project-v1");
    assert_eq!(project_value["modules"][0]["module"], "doc_examples");
    fs::remove_dir_all(dir).expect("remove test dir");
}

/// Verifies `doc` writes default HTML documentation with an aggregate index.
///
/// Inputs:
/// - Temporary Terlan source file with one documented public function.
///
/// Output:
/// - Successful command exit code, module HTML page, and `index.html`.
///
/// Transformation:
/// - Runs the public doc command path with the default HTML format and
///   validates the generated static documentation entry point links to the
///   module page.
#[test]
fn doc_command_defaults_to_html_index() {
    let dir = make_doc_command_test_dir("html_index");
    let path = dir.join("DocExample.terl");
    let out_dir = dir.join("docs");
    fs::write(
        &path,
        r#"module std.core.DocExample.

/**
 * Adds numbers.
 */
pub add(x: Int): Int ->
    x + 1.
"#,
    )
    .expect("write source");

    let exit = run(
        CliCommand {
            verb: Some("doc".to_string()),
            args: vec![path.to_string_lossy().to_string()],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    let module_html =
        fs::read_to_string(out_dir.join("std.core.DocExample.html")).expect("read module html");
    assert!(module_html.contains("std.core.DocExample documentation"));
    assert!(module_html.contains("Functions"));
    let index_html = fs::read_to_string(out_dir.join("index.html")).expect("read index html");
    assert!(index_html.contains("Terlan documentation"));
    assert!(index_html.contains("std.core"));
    assert!(index_html.contains("std.core.DocExample.html"));
    fs::remove_dir_all(dir).expect("remove test dir");
}

/// Verifies `doc std --check` validates the public stdlib documentation.
///
/// Inputs:
/// - The scratch workspace `std` source tree.
///
/// Output:
/// - Successful command exit code.
///
/// Transformation:
/// - Runs the public documentation validation path over the release-owned
///   standard-library source modules without writing output artifacts.
#[test]
fn doc_check_accepts_std_reference() {
    let exit = run(
        CliCommand {
            verb: Some("doc".to_string()),
            args: vec!["std".to_string(), "--check".to_string()],
        },
        CliState::default(),
    );

    assert_eq!(exit, ExitCode::SUCCESS);
}

/// Verifies `doc std` generates a navigable stdlib HTML reference.
///
/// Inputs:
/// - The scratch workspace `std` source tree and a temporary output
///   directory.
///
/// Output:
/// - Successful command exit code plus generated index and module pages.
///
/// Transformation:
/// - Runs the public documentation generation path over stdlib source and
///   confirms representative public modules are linked and rendered.
#[test]
fn doc_command_generates_std_html_reference() {
    let dir = make_doc_command_test_dir("std_html_reference");
    let out_dir = dir.join("docs");

    let exit = run(
        CliCommand {
            verb: Some("doc".to_string()),
            args: vec!["std".to_string()],
        },
        CliState {
            out_dir: out_dir.clone(),
            ..CliState::default()
        },
    );

    assert_eq!(exit, ExitCode::SUCCESS);
    let index_html = fs::read_to_string(out_dir.join("index.html")).expect("read index html");
    assert!(index_html.contains("std.core"));
    assert!(index_html.contains("std.collections"));
    assert!(index_html.contains("std.core.String.html"));
    assert!(index_html.contains("std.collections.Map.html"));
    assert!(out_dir.join("std.core.String.html").exists());
    assert!(out_dir.join("std.collections.Map.html").exists());
    fs::remove_dir_all(dir).expect("remove test dir");
}

/// Creates a unique temporary directory for doc command tests.
///
/// Inputs:
/// - `label`: readable test label included in the directory name.
///
/// Output:
/// - Created temporary directory path.
///
/// Transformation:
/// - Combines the label, process id, and clock time to avoid collisions,
///   then recreates the directory under the OS temporary directory.
fn make_doc_command_test_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let path = std::env::temp_dir().join(format!(
        "terlan_doc_command_{label}_{}_{}",
        std::process::id(),
        nanos
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create doc command test dir");
    path
}
