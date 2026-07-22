use std::fs;
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::{CliCommand, CliState};

use super::{
    parse_debug_args, parse_debug_script, render_debug_error_json, render_debug_error_text,
    render_native_debug_report_json, render_native_debug_report_text,
    script::RESERVED_SCRIPT_COMMANDS,
    session::{open_native_debug_session, DebugBreakpointResolution, NativeDebugSessionReport},
    DebugCliError,
};

/// Returns one stable admitted-image report for renderer tests.
fn native_debug_report() -> NativeDebugSessionReport {
    NativeDebugSessionReport {
        target: "build/app.tvm".to_string(),
        format: "elf".to_string(),
        architecture: "x86_64".to_string(),
        target_triple: "x86_64-unknown-linux-gnu".to_string(),
        compiler: "terlc 0.0.7".to_string(),
        build: "build-1".to_string(),
        package: "app".to_string(),
        module: "app.Main".to_string(),
        descriptor_digest: "ab".repeat(32),
        exports: vec!["app.Main.main/0".to_string()],
        continuation_ids: vec![101, 102],
        source_record_count: 3,
        breakpoints: vec![DebugBreakpointResolution {
            spec: "app.Main.main".to_string(),
            functions: vec!["app.Main.main/0@src/Main.terl:10..30".to_string()],
        }],
        script_commands: Some(2),
        json_events: true,
    }
}

/// Allocates one unique command-level debugger fixture directory.
fn debug_fixture_dir(name: &str) -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock after Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "terlan-debug-command-{}-{nonce}-{name}",
        std::process::id()
    ))
}

#[test]
fn native_debug_session_admits_built_image_and_resolves_breakpoint() {
    let dir = debug_fixture_dir("admission");
    let source = dir.join("debug_session.terl");
    let out_dir = dir.join("build");
    fs::create_dir_all(&dir).expect("create debugger fixture directory");
    fs::write(
        &source,
        "module debug_session.\n\npub main(): Int -> helper(41).\n\nhelper(value: Int): Int -> value + 1.\n",
    )
    .expect("write debugger source fixture");
    let state = CliState {
        out_dir: out_dir.clone(),
        ..CliState::default()
    };
    assert_eq!(
        crate::commands::build::run(
            CliCommand {
                verb: Some("build".to_string()),
                args: vec![source.display().to_string()],
            },
            state,
        ),
        ExitCode::SUCCESS
    );
    let image = out_dir.join("vm/debug_session.tvm");
    let args = parse_debug_args(&[
        image.display().to_string(),
        "--break".to_string(),
        "debug_session.main".to_string(),
        "--json-events".to_string(),
    ])
    .expect("parse native debugger invocation");

    let report = open_native_debug_session(&args, None)
        .expect("built native image should be admitted by debugger");

    assert_eq!(report.target, image.display().to_string());
    assert_eq!(report.module, "debug_session");
    assert_eq!(report.source_record_count, 2);
    assert!(report.exports.iter().any(|export| export.contains("main")));
    assert_eq!(report.breakpoints.len(), 1);
    assert!(report.breakpoints[0].functions[0].contains("debug_session.main/0"));
    assert!(report.json_events);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn native_debug_session_rejects_source_and_renamed_json_targets() {
    let dir = debug_fixture_dir("rejection");
    fs::create_dir_all(&dir).expect("create debugger rejection fixture");
    let source = dir.join("not-an-image.terl");
    fs::write(&source, "module not_an_image.\n").expect("write source target");
    let source_args = parse_debug_args(&[source.display().to_string()]).expect("parse source path");
    let source_error = open_native_debug_session(&source_args, None)
        .expect_err("source target must not enter native debugger");
    assert_eq!(source_error.code, "debug_target_not_native_image");

    let renamed_json = dir.join("renamed.tvm");
    fs::write(&renamed_json, b"{\"format\":\"legacy\"}").expect("write renamed JSON");
    let json_args =
        parse_debug_args(&[renamed_json.display().to_string()]).expect("parse renamed JSON path");
    let json_error = open_native_debug_session(&json_args, None)
        .expect_err("renamed JSON must not enter native debugger");
    assert_eq!(json_error.code, "debug_native_image_rejected");
    assert!(json_error.message.contains("JSON is not a TVM image"));
    let _ = fs::remove_dir_all(dir);
}

/// Verifies debugger admission has no selectable compatibility runtime.
#[test]
fn debug_args_reject_runtime_fallback_selection() {
    let error = parse_debug_args(&[
        "app.tvm".to_string(),
        "--runtime".to_string(),
        "interpreter".to_string(),
    ])
    .expect_err("runtime fallback selector must be rejected");

    assert_eq!(error.code, "debug_unknown_option");
    assert_eq!(error.message, "unknown terlc debug option: --runtime");
}

#[test]
fn debug_args_requires_target_or_script() {
    let err = parse_debug_args(&[]).expect_err("debug without target should fail");

    assert_eq!(err.code, "debug_missing_target");
    assert_eq!(
        err.message,
        "terlc debug requires a native image or --script"
    );
}

#[test]
fn debug_args_accepts_target_breakpoint_script_and_json_events() {
    let args = parse_debug_args(&[
        "app".to_string(),
        "--break".to_string(),
        "app.Main.main".to_string(),
        "--break".to_string(),
        "src/Main.terl:12".to_string(),
        "--script".to_string(),
        "debug.terldbg".to_string(),
        "--json-events".to_string(),
    ])
    .expect("debug args should parse");

    assert_eq!(args.target.unwrap().display().to_string(), "app");
    assert_eq!(
        args.breakpoints,
        vec!["app.Main.main".to_string(), "src/Main.terl:12".to_string()]
    );
    assert_eq!(args.script.unwrap().display().to_string(), "debug.terldbg");
    assert!(args.json_events);
}

#[test]
fn debug_args_accepts_nested_module_and_file_line_breakpoints() {
    let args = parse_debug_args(&[
        "app".to_string(),
        "--break".to_string(),
        "workspace.app.Main.run_test".to_string(),
        "--break".to_string(),
        "src/app/Main.terl:42".to_string(),
    ])
    .expect("valid breakpoint specs should parse");

    assert_eq!(
        args.breakpoints,
        vec![
            "workspace.app.Main.run_test".to_string(),
            "src/app/Main.terl:42".to_string()
        ]
    );
}

#[test]
fn debug_args_accepts_conditional_breakpoints() {
    let args = parse_debug_args(&[
        "app".to_string(),
        "--break".to_string(),
        "app.Main.main where user.id == 10".to_string(),
        "--break".to_string(),
        "src/app/Main.terl:42 where process.status == \"blocked\"".to_string(),
    ])
    .expect("conditional breakpoint specs should parse");

    assert_eq!(
        args.breakpoints,
        vec![
            "app.Main.main where user.id == 10".to_string(),
            "src/app/Main.terl:42 where process.status == \"blocked\"".to_string()
        ]
    );
}

#[test]
fn debug_args_rejects_invalid_breakpoint_spec() {
    let err = parse_debug_args(&["app".to_string(), "--break".to_string(), "Main".to_string()])
        .expect_err("invalid breakpoint should fail");

    assert_eq!(err.code, "debug_invalid_breakpoint");
    assert_eq!(
        err.message,
        "invalid breakpoint spec `Main`; expected module.function or file:line"
    );
}

#[test]
fn debug_args_rejects_empty_breakpoint_condition() {
    let err = parse_debug_args(&[
        "app".to_string(),
        "--break".to_string(),
        "app.Main.main where ".to_string(),
    ])
    .expect_err("empty breakpoint condition should fail");

    assert_eq!(err.code, "debug_invalid_breakpoint_condition");
    assert_eq!(
        err.message,
        "breakpoint `app.Main.main where ` must include a breakpoint and condition"
    );
}

#[test]
fn debug_args_rejects_zero_line_breakpoint() {
    let err = parse_debug_args(&[
        "app".to_string(),
        "--break".to_string(),
        "src/Main.terl:0".to_string(),
    ])
    .expect_err("zero line breakpoint should fail");

    assert_eq!(err.code, "debug_invalid_breakpoint_line");
    assert_eq!(
        err.message,
        "breakpoint `src/Main.terl:0` must use a positive line number"
    );
}

#[test]
fn debug_args_rejects_unknown_option() {
    let err = parse_debug_args(&["app".to_string(), "--step".to_string()])
        .expect_err("unknown option should fail");

    assert_eq!(err.code, "debug_unknown_option");
    assert_eq!(err.message, "unknown terlc debug option: --step");
}

#[test]
fn debug_args_rejects_missing_breakpoint_value() {
    let err = parse_debug_args(&["app".to_string(), "--break".to_string()])
        .expect_err("missing breakpoint value should fail");

    assert_eq!(err.code, "debug_missing_option_value");
    assert_eq!(err.message, "--break requires a breakpoint spec");
}

#[test]
fn debug_args_rejects_missing_script_value() {
    let err = parse_debug_args(&["app".to_string(), "--script".to_string()])
        .expect_err("missing script value should fail");

    assert_eq!(err.code, "debug_missing_option_value");
    assert_eq!(err.message, "--script requires a debugger script path");
}

#[test]
fn debug_args_rejects_duplicate_script() {
    let err = parse_debug_args(&[
        "--script".to_string(),
        "one.terldbg".to_string(),
        "--script".to_string(),
        "two.terldbg".to_string(),
    ])
    .expect_err("duplicate script should fail");

    assert_eq!(err.code, "debug_duplicate_script");
    assert_eq!(err.message, "terlc debug accepts at most one --script");
}

#[test]
fn debug_args_rejects_too_many_targets() {
    let err = parse_debug_args(&["app".to_string(), "other".to_string()])
        .expect_err("extra target should fail");

    assert_eq!(err.code, "debug_too_many_targets");
    assert_eq!(err.message, "terlc debug accepts at most one native image");
}

#[test]
fn debug_script_accepts_reserved_commands_and_breakpoints() {
    let commands = parse_debug_script(
        "\
# comment
break app.Main.main
break src/Main.terl:12 where counter == 3
help
list
enable 1
disable app.Main.main
remove src/Main.terl:12
pause
frame 1
print user.id
continue
quit
",
    )
    .expect("reserved debugger script should parse");

    assert_eq!(commands.len(), 12);
    assert_eq!(commands[0].line, 2);
    assert_eq!(commands[0].name, "break");
    assert_eq!(commands[0].argument.as_deref(), Some("app.Main.main"));
    assert_eq!(commands[1].line, 3);
    assert_eq!(
        commands[1].argument.as_deref(),
        Some("src/Main.terl:12 where counter == 3")
    );
    assert_eq!(commands[2].name, "help");
    assert_eq!(commands[2].argument, None);
    assert_eq!(commands[3].name, "list");
    assert_eq!(commands[4].name, "enable");
    assert_eq!(commands[4].argument.as_deref(), Some("1"));
    assert_eq!(commands[5].name, "disable");
    assert_eq!(commands[5].argument.as_deref(), Some("app.Main.main"));
    assert_eq!(commands[6].name, "remove");
    assert_eq!(commands[6].argument.as_deref(), Some("src/Main.terl:12"));
    assert_eq!(commands[7].name, "pause");
    assert_eq!(commands[8].name, "frame");
    assert_eq!(commands[8].argument.as_deref(), Some("1"));
    assert_eq!(commands[11].name, "quit");
    assert_eq!(commands[11].argument, None);
}

#[test]
fn debug_script_command_inventory_matches_parser() {
    for command in RESERVED_SCRIPT_COMMANDS {
        let line = match *command {
            "break" => "break app.Main.main",
            "remove" => "remove 1",
            "enable" => "enable 1",
            "disable" => "disable 1",
            "frame" => "frame 1",
            "process" => "process 1",
            "print" => "print user.id",
            "eval" => "eval counter + 1",
            "trace" => "trace app.Main.main",
            "untrace" => "untrace app.Main.main",
            "restart" => "restart retry",
            "use" => "use fallback",
            name => name,
        };
        let parsed = parse_debug_script(line)
            .unwrap_or_else(|err| panic!("reserved command `{command}` should parse: {err:?}"));

        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].name, *command);
    }
}

#[test]
fn debug_script_rejects_unknown_command() {
    let err = parse_debug_script("teleport now\n").expect_err("unknown script command should fail");

    assert_eq!(err.code, "debug_script_unknown_command");
    assert_eq!(
        err.message,
        "unknown debugger script command on line 1: `teleport`"
    );
}

#[test]
fn debug_script_rejects_missing_command_argument() {
    let err = parse_debug_script("break\n").expect_err("missing argument should fail");

    assert_eq!(err.code, "debug_script_missing_argument");
    assert_eq!(
        err.message,
        "debugger script command `break` on line 1 needs an argument"
    );
}

#[test]
fn debug_script_rejects_invalid_frame_selector() {
    let err = parse_debug_script("frame zero\n").expect_err("invalid frame selector should fail");

    assert_eq!(err.code, "debug_script_invalid_argument");
    assert_eq!(
        err.message,
        "debugger script command `frame` on line 1 requires a positive integer"
    );
}

#[test]
fn debug_script_rejects_invalid_breakpoint_selector() {
    let err =
        parse_debug_script("disable 0\n").expect_err("invalid breakpoint selector should fail");

    assert_eq!(err.code, "debug_script_invalid_breakpoint_selector");
    assert_eq!(
        err.message,
        "debugger script command `disable` on line 1 needs a breakpoint id or spec"
    );
}

#[test]
fn debug_script_rejects_unexpected_list_argument() {
    let err = parse_debug_script("list all\n").expect_err("list argument should fail");

    assert_eq!(err.code, "debug_script_unexpected_argument");
    assert_eq!(
        err.message,
        "unexpected argument on debugger script line 1: `list all`"
    );
}

#[test]
fn debug_script_rejects_unexpected_help_argument() {
    let err = parse_debug_script("help all\n").expect_err("help argument should fail");

    assert_eq!(err.code, "debug_script_unexpected_argument");
    assert_eq!(
        err.message,
        "unexpected argument on debugger script line 1: `help all`"
    );
}

#[test]
fn debug_script_rejects_empty_scripts() {
    let err = parse_debug_script("\n# only comments\n").expect_err("empty script should fail");

    assert_eq!(err.code, "debug_script_empty");
    assert_eq!(
        err.message,
        "debugger script must contain at least one command"
    );
}

#[test]
fn debug_native_image_text_report_is_stable() {
    let text = render_native_debug_report_text(&native_debug_report());

    assert!(text.starts_with("Terlan native debugger image admitted\n"));
    assert!(text.contains("  target: build/app.tvm\n"));
    assert!(text.contains("  exports: app.Main.main/0\n"));
    assert!(text.contains("  continuations: 2\n"));
    assert!(text.contains("  breakpoints: app.Main.main => app.Main.main/0@src/Main.terl:10..30\n"));
}

#[test]
fn debug_native_image_json_report_is_stable() {
    let value: Value =
        serde_json::from_str(&render_native_debug_report_json(&native_debug_report()))
            .expect("valid JSON report");

    assert_eq!(value["command"], "debug");
    assert_eq!(value["kind"], "native_image_admitted");
    assert_eq!(value["target"], "build/app.tvm");
    assert_eq!(value["module"], "app.Main");
    assert_eq!(value["continuation_ids"], serde_json::json!([101, 102]));
    assert_eq!(value["source_record_count"], 3);
    assert_eq!(value["script_commands"], 2);
    assert_eq!(value["json_events"], true);
    assert_eq!(value["live_execution"], false);
}

#[test]
fn debug_native_image_text_report_includes_script_command_count() {
    let mut report = native_debug_report();
    report.script_commands = Some(3);

    assert!(render_native_debug_report_text(&report).contains("  script_commands: 3\n"));
}

#[test]
fn debug_error_renderers_are_stable() {
    let err = DebugCliError {
        code: "debug_unknown_option",
        message: "unknown terlc debug option: --bad".to_string(),
    };
    let json: Value =
        serde_json::from_str(&render_debug_error_json(&err)).expect("valid JSON diagnostic");

    assert_eq!(
        render_debug_error_text(&err),
        "error[debug_unknown_option]: unknown terlc debug option: --bad"
    );
    assert_eq!(json["code"], "debug_unknown_option");
    assert_eq!(json["message"], "unknown terlc debug option: --bad");
    assert_eq!(json["kind"], "error");
}
