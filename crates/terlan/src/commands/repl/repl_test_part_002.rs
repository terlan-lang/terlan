
/// Verifies REPL declarations use the same AOT pure worker as VM artifacts.
#[test]
fn repl_prompt_inputs_execute_pure_integer_function_through_aot_artifact() {
    let outputs = evaluate_repl_prompt_inputs(
        &[
            "add(x: Int, y: Int): Int -> x + y.".to_string(),
            "add(20, 22).".to_string(),
        ],
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
    )
    .expect("compile and execute REPL pure artifact");

    assert_eq!(
        outputs,
        vec![vec!["Unit".to_string()], vec!["42".to_string()]]
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
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
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

/// Verifies persisted integer bindings compare against other literals.
///
/// Inputs:
/// - One prompt binding `a` to integer literal `0`.
/// - One prompt binding `b` to integer literal `2`.
/// - Later expressions comparing persisted bindings to a literal and to each
///   other.
///
/// Output:
/// - Both comparisons evaluate to `false` and the binding prompt returns
///   `Unit`.
///
/// Transformation:
/// - Exercises the prompt path that rebuilds REPL state as a generated `let`
///   expression, preventing persisted literal values from creating exact
///   literal-vs-literal type mismatches.
#[test]
fn repl_prompt_inputs_compare_persisted_integer_literal_binding() {
    let outputs = evaluate_repl_prompt_inputs(
        &[
            "let a = 0.".to_string(),
            "let b = 2.".to_string(),
            "if { a > 1 -> true; _ -> false }.".to_string(),
            "a > b.".to_string(),
        ],
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
    )
    .expect("evaluate repl prompts");

    assert_eq!(
        outputs,
        vec![
            vec!["Unit".to_string()],
            vec!["Unit".to_string()],
            vec!["false".to_string()],
            vec!["false".to_string()]
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
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
    )
    .expect("evaluate repl prompt");

    assert_eq!(outputs, vec![vec!["<function>".to_string()]]);
}

/// Verifies mutable receiver prompts persist updated REPL bindings.
///
/// Inputs:
/// - One prompt binding an empty list.
/// - One prompt mutating that list through receiver syntax.
/// - One prompt reading the binding again.
///
/// Output:
/// - `Unit` for the binding and mutation prompts.
/// - The updated list value for the final prompt.
///
/// Transformation:
/// - Exercises the prompt-level state rewrite used by interactive REPL
///   sessions so mutable receiver methods update persisted bindings instead
///   of being discarded between prompts.
#[test]
fn repl_prompt_inputs_persist_mutable_receiver_updates() {
    let outputs = evaluate_repl_prompt_inputs(
        &[
            "let c = [].".to_string(),
            "c.push(1).".to_string(),
            "c.".to_string(),
        ],
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
    )
    .expect("evaluate repl prompts");

    assert_eq!(
        outputs,
        vec![
            vec!["Unit".to_string()],
            vec!["Unit".to_string()],
            vec!["[1]".to_string()]
        ]
    );
}

/// Verifies mutable map receiver prompts persist source-renderable values.
///
/// Inputs:
/// - Imports for the portable map constructor and type.
/// - One prompt binding an empty map.
/// - One prompt mutating that map through receiver syntax.
/// - One prompt reading the binding again.
///
/// Output:
/// - `Map()` before mutation and `Map({"a", 1})` after mutation.
///
/// Transformation:
/// - Locks map rendering to constructor syntax instead of Erlang-style map
///   syntax so persisted REPL bindings can be parsed in later prompts.
#[test]
fn repl_prompt_inputs_persist_mutable_map_receiver_updates() {
    let outputs = evaluate_repl_prompt_inputs(
        &[
            "import std.collections.Map.".to_string(),
            "import type std.collections.Map.Map.".to_string(),
            "let m = Map().".to_string(),
            "m.".to_string(),
            "m.put(\"a\", 1).".to_string(),
            "m.".to_string(),
        ],
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        NativePolicy::NativeBoundaryOptional,
        TargetProfile::Vm,
    )
    .expect("evaluate repl prompts");

    assert_eq!(
        outputs,
        vec![
            vec!["Unit".to_string()],
            vec!["Unit".to_string()],
            vec!["Unit".to_string()],
            vec!["Map()".to_string()],
            vec!["Unit".to_string()],
            vec!["Map({\"a\", 1})".to_string()]
        ]
    );
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
            "[package]\nname = \"demo\"\nversion = \"0.0.1\"\n\n[build]\nsource_roots = [\"src\", \"lib\"]\nartifact = \"terlan-vm\"\n",
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
