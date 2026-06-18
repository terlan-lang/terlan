use std::fs;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use super::{
    is_repl_help_args, parse_repl_value_binding, render_repl_json_event,
    repl_expression_with_bindings, repl_load_sources, ReplValueBinding,
};

/// Verifies REPL command-local help aliases are recognized.
///
/// Inputs:
/// - Synthetic command-local argument vectors for `--help` and `-h`.
///
/// Output:
/// - Test assertions only; no files are read or written.
///
/// Transformation:
/// - Exercises the REPL help detector without starting the interactive
///   command loop.
#[test]
fn repl_help_args_accept_long_and_short_help() {
    assert!(is_repl_help_args(&["--help".to_string()]));
    assert!(is_repl_help_args(&["-h".to_string()]));
}

/// Verifies REPL help detection does not consume seed paths.
///
/// Inputs:
/// - Synthetic command-local argument vectors for empty args, one seed
///   path, and malformed extra arguments.
///
/// Output:
/// - Test assertions only; no files are read or written.
///
/// Transformation:
/// - Keeps REPL help routing exact so normal seed loading and malformed
///   argument diagnostics remain owned by the main command path.
#[test]
fn repl_help_args_reject_non_help_invocations() {
    assert!(!is_repl_help_args(&[]));
    assert!(!is_repl_help_args(&["src/main.terl".to_string()]));
    assert!(!is_repl_help_args(&[
        "--help".to_string(),
        "src/main.terl".to_string()
    ]));
}

/// Verifies that REPL binding syntax captures a simple persistent name.
///
/// Inputs:
/// - A terminator-stripped REPL entry with shape `let name = expr`.
///
/// Output:
/// - Parsed name and value expression.
///
/// Transformation:
/// - Exercises the REPL-only binding parser without invoking the full
///   interactive command loop.
#[test]
fn repl_value_binding_parser_accepts_simple_binding() {
    let binding = parse_repl_value_binding("let total = 1 + 2").unwrap();

    assert_eq!(binding.name, "total");
    assert_eq!(binding.value, "1 + 2");
}

/// Verifies that full Terlan `let` expressions are left to source parsing.
///
/// Inputs:
/// - A terminator-stripped source `let` expression containing `;`.
///
/// Output:
/// - `None`, indicating the REPL did not treat the entry as persistent
///   session state.
///
/// Transformation:
/// - Keeps the REPL-only `let name = expr.` entry form separate from normal
///   Terlan let expressions such as `let x = 1; x + 1`.
#[test]
fn repl_value_binding_parser_rejects_source_let_expression() {
    assert!(parse_repl_value_binding("let x = 1; x + 1").is_none());
}

/// Verifies that persisted bindings lower through ordinary Terlan `let`.
///
/// Inputs:
/// - Current expression source and two persisted REPL value bindings.
///
/// Output:
/// - Generated source expression with persisted bindings evaluated first.
///
/// Transformation:
/// - Converts REPL state into the compiler-owned source form used before
///   parsing, typechecking, CoreIR lowering, and evaluator execution.
#[test]
fn repl_expression_with_bindings_builds_source_let_expression() {
    let expression = repl_expression_with_bindings(
        "x + y",
        &[
            ReplValueBinding {
                name: "x".to_string(),
                value: "1".to_string(),
            },
            ReplValueBinding {
                name: "y".to_string(),
                value: "2".to_string(),
            },
        ],
    );

    assert_eq!(expression, "let x = 1; y = 2; x + y");
}

/// Verifies JSON REPL events are valid without optional fields.
///
/// Inputs:
/// - Event kind and text without extra field payload.
///
/// Output:
/// - Parsed JSON with schema, kind, and text fields.
///
/// Transformation:
/// - Renders the event through the same helper used by the REPL command and
///   parses it back through `serde_json`.
#[test]
fn repl_json_event_without_extra_fields_is_valid_json() {
    let event = render_repl_json_event("ready", None, "REPL ready");
    let value: serde_json::Value = serde_json::from_str(&event).expect("parse repl event");

    assert_eq!(value["schema"], "terlan-repl-event-v1");
    assert_eq!(value["kind"], "ready");
    assert_eq!(value["text"], "REPL ready");
}

/// Verifies JSON REPL events are valid with optional fields.
///
/// Inputs:
/// - Event kind, field payload, and human-readable text.
///
/// Output:
/// - Parsed JSON containing both the payload field and text field.
///
/// Transformation:
/// - Confirms optional field insertion preserves comma separation before
///   the final `text` property.
#[test]
fn repl_json_event_with_extra_fields_is_valid_json() {
    let event = render_repl_json_event(
        "result",
        Some("\"value\":\"Unit\",\"message\":\"ok\""),
        "Unit",
    );
    let value: serde_json::Value = serde_json::from_str(&event).expect("parse repl event");

    assert_eq!(value["schema"], "terlan-repl-event-v1");
    assert_eq!(value["kind"], "result");
    assert_eq!(value["value"], "Unit");
    assert_eq!(value["message"], "ok");
    assert_eq!(value["text"], "Unit");
}

/// Verifies project loads follow manifest source roots.
///
/// Inputs:
/// - A temporary project with `src` and `lib` source roots plus unrelated
///   `.terl` files outside those roots.
///
/// Output:
/// - Loaded source paths from `src` and `lib` only.
///
/// Transformation:
/// - Reads `terlan.toml`, resolves `[build] source_roots`, recursively
///   collects Terlan files under those roots, and ignores unrelated project
///   directories such as `_build`.
#[test]
fn repl_load_sources_uses_project_manifest_source_roots() {
    let root = make_repl_test_dir("manifest_source_roots");
    fs::create_dir_all(root.join("src/app")).expect("create src");
    fs::create_dir_all(root.join("lib/app")).expect("create lib");
    fs::create_dir_all(root.join("_build/src")).expect("create ignored build dir");
    fs::create_dir_all(root.join("misc")).expect("create ignored misc dir");
    fs::write(
            root.join("terlan.toml"),
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\", \"lib\"]\nartifact = \"beam-thin\"\n",
        )
        .expect("write manifest");
    fs::write(root.join("src/app/Main.terl"), "module app.Main.\n").expect("write src");
    fs::write(root.join("lib/app/Util.terl"), "module app.Util.\n").expect("write lib");
    fs::write(
        root.join("_build/src/generated.terl"),
        "module ignored.Generated.\n",
    )
    .expect("write ignored build source");
    fs::write(root.join("misc/Other.terl"), "module ignored.Other.\n")
        .expect("write ignored misc source");

    let sources = repl_load_sources(&root).expect("load project sources");
    let paths = sources
        .iter()
        .map(|(path, _)| path.replace('\\', "/"))
        .collect::<Vec<_>>();

    assert_eq!(sources.len(), 2);
    assert!(paths.iter().any(|path| path.ends_with("lib/app/Util.terl")));
    assert!(paths.iter().any(|path| path.ends_with("src/app/Main.terl")));
    assert!(!paths.iter().any(|path| path.contains("_build")));
    assert!(!paths.iter().any(|path| path.contains("/misc/")));

    fs::remove_dir_all(root).expect("remove test project");
}

/// Creates a unique temporary directory for REPL unit tests.
///
/// Inputs:
/// - `label`: stable readable prefix for the test directory name.
///
/// Output:
/// - Path to a newly created directory under the OS temporary directory.
///
/// Transformation:
/// - Combines the label, process id, and current time to avoid collisions,
///   removes any stale directory with that exact name, then creates it.
fn make_repl_test_dir(label: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let path = std::env::temp_dir().join(format!(
        "terlan_repl_{label}_{}_{}",
        std::process::id(),
        nanos
    ));
    let _ = fs::remove_dir_all(&path);
    fs::create_dir_all(&path).expect("create repl test dir");
    path
}
