use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::UNIX_EPOCH;

use super::{
    evaluate_repl_prompt_inputs, is_repl_help_args, parse_repl_command_args,
    parse_repl_value_binding, render_repl_json_event, repl_expression_with_bindings,
    repl_json_field, repl_load_sources, run_repl_expression_with_output, ReplRuntime,
    ReplValueBinding,
};
use crate::validation::native_policy::NativePolicy;
use crate::validation::target_profile::TargetProfile;
use crate::{ColorChoice, DiagnosticFormat};

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

/// Verifies REPL runtime selection parses stable and experimental modes.
///
/// Inputs:
/// - Command-local REPL arguments containing `--runtime beam` or
///   `--runtime vm`.
///
/// Output:
/// - Parsed runtime enum and seed path assertions.
///
/// Transformation:
/// - Keeps runtime choice explicit at the command parser boundary before the
///   interactive loop starts.
#[test]
fn repl_command_args_parse_runtime_selection() {
    let beam = parse_repl_command_args(&["--runtime".into(), "beam".into()], false)
        .expect("parse beam runtime");
    assert_eq!(beam.runtime, ReplRuntime::Beam);
    assert_eq!(beam.seed_path, None);

    let vm = parse_repl_command_args(
        &["--runtime".into(), "vm".into(), "src/Main.terl".into()],
        true,
    )
    .expect("parse vm runtime");
    assert_eq!(vm.runtime, ReplRuntime::Vm);
    assert_eq!(vm.seed_path.as_deref(), Some("src/Main.terl"));
}

/// Verifies experimental VM selection is gated.
///
/// Inputs:
/// - Command-local REPL arguments selecting `--runtime vm`.
/// - Experimental flag disabled.
///
/// Output:
/// - Usage error text.
///
/// Transformation:
/// - Prevents the hidden Rust VM path from becoming the default public REPL
///   runtime before the VM coverage is complete.
#[test]
fn repl_command_args_reject_vm_runtime_without_experimental_flag() {
    let error = parse_repl_command_args(&["--runtime".into(), "vm".into()], false)
        .expect_err("vm requires experimental flag");

    assert!(error.contains("experimental"));
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

    assert_eq!(binding.pattern, "total");
    assert_eq!(binding.value, "1 + 2");
}

/// Verifies that REPL binding syntax accepts destructuring patterns.
///
/// Inputs:
/// - A terminator-stripped REPL entry with a tuple pattern on the left side.
///
/// Output:
/// - Parsed pattern text and value expression.
///
/// Transformation:
/// - Keeps the REPL persistence parser broad enough for formal pattern
///   validation to happen through the compiler path instead of local name
///   filtering.
#[test]
fn repl_value_binding_parser_accepts_tuple_pattern() {
    let binding = parse_repl_value_binding("let {head, _} = pair").unwrap();

    assert_eq!(binding.pattern, "{head, _}");
    assert_eq!(binding.value, "pair");
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
                pattern: "x".to_string(),
                value: "1".to_string(),
            },
            ReplValueBinding {
                pattern: "y".to_string(),
                value: "2".to_string(),
            },
        ],
    );

    assert_eq!(expression, "let x = (1); y = (2); x + y");
}

/// Verifies persisted lambda bindings remain separated from the later body.
///
/// Inputs:
/// - Current function-value call expression and one persisted lambda binding.
///
/// Output:
/// - Generated source expression with the lambda parenthesized.
///
/// Transformation:
/// - Protects anonymous-function bodies from consuming the hidden REPL
///   semicolon separator when prompt state is converted into a Terlan `let`.
#[test]
fn repl_expression_with_bindings_parenthesizes_lambda_binding_values() {
    let expression = repl_expression_with_bindings(
        "a.(10)",
        &[ReplValueBinding {
            pattern: "a".to_string(),
            value: "(x) -> x + x".to_string(),
        }],
    );

    assert_eq!(expression, "let a = ((x) -> x + x); a.(10)");
}

/// Verifies REPL prompt evaluation supports persisted function values.
///
/// Inputs:
/// - One REPL binding prompt defining a lambda value.
/// - One later prompt invoking the persisted function value.
///
/// Output:
/// - First prompt evaluates to `Unit`; second prompt evaluates to `20`.
///
/// Transformation:
/// - Exercises the full prompt path used by docs and non-interactive REPL
///   checks: parse prompt terminators, persist `let` bindings, compile through
///   CoreIR, and evaluate function-value invocation without BEAM execution.
#[test]
fn repl_prompt_inputs_apply_persisted_lambda_binding() {
    let outputs = evaluate_repl_prompt_inputs(
        &["let a = (x) -> x + x.".to_string(), "a.(10).".to_string()],
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        NativePolicy::SafeNativeOptional,
        TargetProfile::Erlang,
    )
    .expect("evaluate repl prompts");

    assert_eq!(
        outputs,
        vec![vec!["Unit".to_string()], vec!["20".to_string()]]
    );
}

/// Verifies REPL prompt evaluation supports persisted destructuring bindings.
///
/// Inputs:
/// - One prompt binding a tuple value.
/// - One prompt destructuring that tuple.
/// - One prompt reading the destructured variable.
///
/// Output:
/// - `Unit` for each binding and the destructured value for the final prompt.
///
/// Transformation:
/// - Exercises the full REPL prompt pipeline for pattern bindings so
///   persistent session state stays aligned with ordinary Terlan `let`
///   semantics.
#[test]
fn repl_prompt_inputs_support_destructuring_binding() {
    let outputs = evaluate_repl_prompt_inputs(
        &[
            "let a = {1, 3}.".to_string(),
            "let {b, _} = a.".to_string(),
            "b.".to_string(),
        ],
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        NativePolicy::SafeNativeOptional,
        TargetProfile::Erlang,
    )
    .expect("evaluate repl prompts");

    assert_eq!(
        outputs,
        vec![
            vec!["Unit".to_string()],
            vec!["Unit".to_string()],
            vec!["1".to_string()]
        ]
    );
}

/// Verifies REPL prompt evaluation renders standalone lambdas as functions.
///
/// Inputs:
/// - One REPL expression prompt containing a lambda value.
///
/// Output:
/// - `"<function>"`, proving the prompt path no longer reports unsupported
///   `Lam` for anonymous function values.
///
/// Transformation:
/// - Locks the user-facing REPL behavior separately from the lower evaluator
///   unit tests so prompt parsing and generated module wrapping stay aligned.
#[test]
fn repl_prompt_inputs_render_standalone_lambda_value() {
    let outputs = evaluate_repl_prompt_inputs(
        &["(x) -> x + x.".to_string()],
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        NativePolicy::SafeNativeOptional,
        TargetProfile::Erlang,
    )
    .expect("evaluate repl prompt");

    assert_eq!(outputs, vec![vec!["<function>".to_string()]]);
}

/// Verifies REPL expressions can execute through the BEAM runtime path.
///
/// Inputs:
/// - One generated expression that prints text and returns `Unit`.
///
/// Output:
/// - Captured stdout line and rendered return value.
///
/// Transformation:
/// - Exercises the selectable regular runtime path by lowering the generated
///   prompt module to Erlang and running it through `erl`. The test skips when
///   Erlang tooling is not installed in the local environment.
#[test]
fn repl_expression_can_run_through_beam_runtime_when_tooling_exists() {
    if Command::new("erlc").arg("-version").output().is_err()
        || Command::new("erl").arg("-version").output().is_err()
    {
        return;
    }
    let root = make_repl_test_dir("beam_runtime");
    let mut output = Vec::new();
    let value = run_repl_expression_with_output(
        "std.io.Console.println(\"hello from BEAM\")",
        &[],
        &[],
        "repl_beam_test",
        "repl_eval_1",
        &root,
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        NativePolicy::SafeNativeOptional,
        TargetProfile::Erlang,
        ReplRuntime::Beam,
        &mut |line| output.push(line.to_string()),
    )
    .expect("run beam repl expression");

    assert_eq!(output, vec!["hello from BEAM".to_string()]);
    assert_eq!(value, "Unit");
    fs::remove_dir_all(root).expect("remove temp dir");
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
    let event = render_repl_json_event("ready", &[], "REPL ready");
    let value: serde_json::Value = serde_json::from_str(&event).expect("parse repl event");

    assert_eq!(value["schema"], "terlan-repl-event-v1");
    assert_eq!(value["kind"], "ready");
    assert_eq!(value["text"], "REPL ready");
}

/// Verifies JSON REPL events are valid with optional fields.
///
/// Inputs:
/// - Event kind, structured field payload, and human-readable text.
///
/// Output:
/// - Parsed JSON containing both the payload field and text field.
///
/// Transformation:
/// - Confirms optional field insertion delegates object and array encoding to
///   `serde_json`.
#[test]
fn repl_json_event_with_extra_fields_is_valid_json() {
    let event = render_repl_json_event(
        "result",
        &[
            repl_json_field("value", "Unit"),
            repl_json_field("commands", serde_json::json!([":help", ":quit"])),
        ],
        "Unit",
    );
    let value: serde_json::Value = serde_json::from_str(&event).expect("parse repl event");

    assert_eq!(value["schema"], "terlan-repl-event-v1");
    assert_eq!(value["kind"], "result");
    assert_eq!(value["value"], "Unit");
    assert_eq!(value["commands"], serde_json::json!([":help", ":quit"]));
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
