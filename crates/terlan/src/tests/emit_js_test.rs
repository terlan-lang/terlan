use super::*;

/// Verifies `emit-js` writes JavaScript artifacts, optional TypeScript
/// declarations, and preserves basic type-mapping helpers.
///
/// Inputs:
/// - A temporary `.terl` source file with public and private functions plus
///   exported algebraic type aliases.
/// - CLI arguments for `emit-js --declarations` and a direct no-declarations
///   command invocation.
///
/// Output:
/// - Test success when the JavaScript file is emitted, Oxc accepts it,
///   declarations are emitted only when requested, and helper type conversions
///   return the expected strings.
///
/// Transformation:
/// - Parses emit-js arguments, runs the public CLI dispatcher, inspects emitted
///   JavaScript/declaration files, then checks direct helper conversions.
#[test]
fn run_emit_js_writes_js_and_declarations() {
    let dir = make_temp_dir("emit_js_success");
    let path = fixture(
            &dir,
            "module js_demo.\n\npub type Option[T] =\n      none\n    | {some, T}.\n\npub type Result[T, E] =\n      {ok, T}\n    | {error, E}.\n\ntype PrivateAlias = Int.\n\npub validate_age(Age: Int): Result[Int, invalid_age] ->\n    case Age >= 0 {\n        true -> {ok, Age};\n        false -> {error, invalid_age}\n    }.\n\nprivate_flag(Name: Text): Bool ->\n    Name >= \"a\".\n",
        );
    let out_dir = dir.join("js");
    let parsed = commands::emit_js::parse_emit_js_args(&[path.clone(), "--declarations".into()])
        .expect("parse emit-js args");
    assert_eq!(
        parsed,
        commands::emit_js::EmitJsArgs {
            path: path.clone(),
            declarations: true,
        }
    );

    let exit = run_cli(vec![
        "emit-js".into(),
        path.clone(),
        "--out-dir".into(),
        out_dir.to_string_lossy().into(),
        "--declarations".into(),
    ]);
    assert_eq!(exit, ExitCode::SUCCESS);

    let js = fs::read_to_string(out_dir.join("js_demo.js")).expect("read js");
    commands::emit_js::assert_oxc_accepts_js_artifact(&out_dir.join("js_demo.js"), &js);
    assert!(js.contains("export function validate_age"));
    assert!(!js.contains("private_flag"));
    let declarations = fs::read_to_string(out_dir.join("js_demo.d.ts")).expect("read declarations");
    assert!(declarations.contains("Result<number"));

    let out_dir_no_declarations = dir.join("js_no_declarations");
    let exit = commands::emit_js::run(
        &[path],
        &CliState {
            out_dir: out_dir_no_declarations.clone(),
            ..Default::default()
        },
    );
    assert_eq!(exit, ExitCode::SUCCESS);
    assert!(out_dir_no_declarations.join("js_demo.js").exists());
    assert!(!out_dir_no_declarations.join("js_demo.d.ts").exists());

    assert_eq!(
        commands::emit_js::typer_type_to_typescript("Float"),
        "number"
    );
    assert_eq!(
        commands::emit_js::typer_type_to_typescript("Binary"),
        "string"
    );
    assert_eq!(
        commands::emit_js::typer_type_to_typescript("Bool"),
        "boolean"
    );
    assert_eq!(
        commands::emit_js::typer_type_to_typescript("Dynamic"),
        "unknown"
    );
    assert_eq!(commands::emit_js::typer_type_to_typescript("ok"), "\"ok\"");
    assert_eq!(commands::emit_js::typer_type_to_typescript("User"), "User");
    assert_eq!(commands::emit_js::typer_type_to_typescript(""), "");
    assert_eq!(
        commands::emit_js::typer_type_to_typescript("Result[Int]"),
        "Result[Int]"
    );
    assert_eq!(
        commands::emit_js::typer_type_to_typescript("Result[Int"),
        "Result[Int"
    );
    assert_eq!(
        commands::emit_js::split_top_level_args("Result[Int, invalid_age], List[Text]"),
        vec!["Result[Int, invalid_age]", "List[Text]"]
    );
}

/// Verifies `emit-js` reports stable failure exit codes for invalid inputs and
/// filesystem write failures.
///
/// Inputs:
/// - Missing arguments, an unknown emit-js option, a missing source path, an
///   unparsable source file, and output paths that intentionally conflict with
///   directories/files.
///
/// Output:
/// - Test success when usage errors return exit code `2` and parse/read/write
///   failures return exit code `1`.
///
/// Transformation:
/// - Runs the emit-js command through direct command entry points against each
///   malformed or blocked input shape and asserts the expected exit code.
#[test]
fn run_emit_js_reports_errors() {
    let missing_arg = commands::emit_js::run(&[], &CliState::default());
    assert_eq!(missing_arg, ExitCode::from(2));

    assert!(commands::emit_js::parse_emit_js_args(&["m.terl".into(), "--bad".into()]).is_err());

    let read_error = commands::emit_js::run(
        &["/tmp/terlan_missing_emit_js.terl".into()],
        &CliState::default(),
    );
    assert_eq!(read_error, ExitCode::from(1));

    let dir = make_temp_dir("emit_js_errors");
    let parse_error_path = fixture(&dir, "module broken\n");
    let parse_error = commands::emit_js::run(&[parse_error_path], &CliState::default());
    assert_eq!(parse_error, ExitCode::from(1));

    let source_path = fixture(&dir, "module js_error.\n\npub value(): Int ->\n    1.\n");
    let blocked_out_dir = dir.join("blocked_js_out");
    fs::write(&blocked_out_dir, "not a directory").expect("write blocked output");
    let create_dir_error = commands::emit_js::run(
        &[source_path.clone()],
        &CliState {
            out_dir: blocked_out_dir,
            ..Default::default()
        },
    );
    assert_eq!(create_dir_error, ExitCode::from(1));

    let write_js_dir = dir.join("write_js");
    fs::create_dir_all(&write_js_dir).expect("create js output");
    fs::create_dir_all(write_js_dir.join("js_error.js")).expect("create conflicting js dir");
    let write_js_error = commands::emit_js::run(
        &[source_path.clone()],
        &CliState {
            out_dir: write_js_dir,
            ..Default::default()
        },
    );
    assert_eq!(write_js_error, ExitCode::from(1));

    let write_dts_dir = dir.join("write_dts");
    fs::create_dir_all(&write_dts_dir).expect("create dts output");
    fs::create_dir_all(write_dts_dir.join("js_error.d.ts")).expect("create conflicting dts dir");
    let write_dts_error = commands::emit_js::run(
        &[source_path, "--declarations".into()],
        &CliState {
            out_dir: write_dts_dir,
            ..Default::default()
        },
    );
    assert_eq!(write_dts_error, ExitCode::from(1));
}
