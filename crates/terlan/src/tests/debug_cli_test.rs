use super::*;

const DEBUG_SCRIPT_COMMANDS: &[&str] = &[
    "help",
    "run",
    "list",
    "break",
    "remove",
    "enable",
    "disable",
    "pause",
    "continue",
    "step",
    "next",
    "finish",
    "bt",
    "frame",
    "locals",
    "args",
    "print",
    "eval",
    "processes",
    "process",
    "mailbox",
    "resources",
    "trace",
    "untrace",
    "restarts",
    "restart",
    "use",
    "abort",
    "quit",
];

/// Verifies debugger help documents the command vocabulary.
///
/// Inputs:
/// - Static debugger usage lines.
///
/// Output:
/// - Test assertions only; no command is executed.
///
/// Transformation:
/// - Locks command-local help to the same reserved debugger script vocabulary
///   enforced by the parser.
#[test]
fn debug_usage_documents_script_commands() {
    let usage = debug_usage_lines().join("\n");
    let script_line = debug_usage_lines()
        .iter()
        .find(|line| line.starts_with("Script commands: "))
        .expect("debug usage should include script command line");
    let documented_commands = script_line
        .strip_prefix("Script commands: ")
        .expect("script command line prefix should be stable")
        .trim_end_matches('.')
        .split(", ")
        .collect::<Vec<_>>();

    assert!(usage.contains("`where <condition>`"));
    assert_eq!(documented_commands, DEBUG_SCRIPT_COMMANDS);
}

/// Verifies `terlc help debug` routes to debugger command-local help.
///
/// Inputs:
/// - Synthetic `terlc help debug` arguments.
///
/// Output:
/// - Successful exit code.
///
/// Transformation:
/// - Runs through the public help dispatcher so debugger documentation is
///   covered by the same path users and editor integrations call.
#[test]
fn run_cli_routes_help_debug_to_debug_usage() {
    assert_eq!(
        run_cli(vec!["help".to_string(), "debug".to_string()]),
        ExitCode::SUCCESS
    );
}

/// Verifies `terlc debug --help` routes to debugger command-local help.
///
/// Inputs:
/// - Synthetic `terlc debug --help` arguments.
///
/// Output:
/// - Successful exit code.
///
/// Transformation:
/// - Proves command-local help is handled before debugger argument validation
///   treats `--help` as an unknown debugger option.
#[test]
fn run_cli_routes_debug_help_to_debug_usage() {
    assert_eq!(
        run_cli(vec!["debug".to_string(), "--help".to_string()]),
        ExitCode::SUCCESS
    );
}

/// Verifies the reserved debugger command routes through the top-level CLI.
///
/// Inputs:
/// - Synthetic `terlc debug app --break app.Main.main --json-events`
///   arguments.
///
/// Output:
/// - Reserved-debugger exit code.
///
/// Transformation:
/// - Runs the public dispatcher instead of helper parsers so the gate proves
///   the user-facing command verb remains wired to the debugger module.
#[test]
fn run_cli_routes_debug_command_to_native_admission() {
    assert_eq!(
        run_cli(vec![
            "debug".to_string(),
            "app".to_string(),
            "--break".to_string(),
            "app.Main.main".to_string(),
            "--json-events".to_string(),
        ]),
        ExitCode::from(1)
    );
}

/// Verifies JSON diagnostic mode reaches the reserved debugger command.
///
/// Inputs:
/// - Synthetic global JSON diagnostic flag plus debugger script arguments.
///
/// Output:
/// - Reserved-debugger exit code.
///
/// Transformation:
/// - Routes through global option parsing before `debug` dispatch so editor
///   integrations can depend on the command accepting machine-readable
///   diagnostics.
#[test]
fn run_cli_routes_debug_command_after_json_diagnostic_flag() {
    let root = make_temp_dir("debug_json_script");
    let script = root.join("session.terldbg");
    std::fs::write(&script, "run\nquit\n").expect("write debugger script");

    assert_eq!(
        run_cli(vec![
            "--diagnostic-format".to_string(),
            "json".to_string(),
            "debug".to_string(),
            "--script".to_string(),
            script.to_string_lossy().to_string(),
        ]),
        ExitCode::from(1)
    );
}

/// Verifies REPL debugger mode enters the interactive VM debugger surface.
///
/// Inputs:
/// - Synthetic `terlc repl --debug` arguments.
///
/// Output:
/// - Successful EOF exit after debugger mode is enabled.
///
/// Transformation:
/// - Runs the public dispatcher so the gate proves REPL debugger mode fails
///   fast before entering the interactive prompt.
#[test]
fn run_cli_routes_repl_debug_to_vm_surface() {
    assert_eq!(
        run_cli(vec!["repl".to_string(), "--debug".to_string()]),
        ExitCode::SUCCESS
    );
}

/// Verifies JSON diagnostic mode reaches REPL debugger mode.
///
/// Inputs:
/// - Synthetic global JSON diagnostic flag plus `repl --debug`.
///
/// Output:
/// - Successful EOF exit after debugger mode is enabled.
///
/// Transformation:
/// - Routes through global option parsing before REPL dispatch so editor
///   integrations can depend on the same machine-readable debugger event.
#[test]
fn run_cli_routes_repl_debug_after_json_diagnostic_flag() {
    assert_eq!(
        run_cli(vec![
            "--diagnostic-format".to_string(),
            "json".to_string(),
            "repl".to_string(),
            "--debug".to_string(),
        ]),
        ExitCode::SUCCESS
    );
}

/// Verifies invalid breakpoint specs fail at the public debugger path.
///
/// Inputs:
/// - Synthetic `terlc debug app --break Main` invocation.
///
/// Output:
/// - Invalid-usage exit code.
///
/// Transformation:
/// - Confirms the dispatcher reaches debugger breakpoint validation before
///   emitting the reserved unimplemented-debugger report.
#[test]
fn run_cli_rejects_debug_command_invalid_breakpoint_spec() {
    assert_eq!(
        run_cli(vec![
            "debug".to_string(),
            "app".to_string(),
            "--break".to_string(),
            "Main".to_string(),
        ]),
        ExitCode::from(2)
    );
}

/// Verifies conditional breakpoint validation runs on the public path.
///
/// Inputs:
/// - Synthetic `terlc debug app --break "app.Main.main where "` invocation.
///
/// Output:
/// - Invalid-usage exit code.
///
/// Transformation:
/// - Confirms empty breakpoint conditions are rejected before the reserved
///   debugger report is emitted.
#[test]
fn run_cli_rejects_debug_command_empty_breakpoint_condition() {
    assert_eq!(
        run_cli(vec![
            "debug".to_string(),
            "app".to_string(),
            "--break".to_string(),
            "app.Main.main where ".to_string(),
        ]),
        ExitCode::from(2)
    );
}

/// Verifies missing debugger scripts fail before the reserved report.
///
/// Inputs:
/// - Synthetic `terlc debug --script missing.terldbg` invocation.
///
/// Output:
/// - Invalid-usage exit code.
///
/// Transformation:
/// - Confirms script validation checks the file boundary instead of silently
///   claiming an absent script was accepted.
#[test]
fn run_cli_rejects_debug_command_missing_script_file() {
    let root = make_temp_dir("debug_missing_script");
    let missing = root.join("missing.terldbg");

    assert_eq!(
        run_cli(vec![
            "debug".to_string(),
            "--script".to_string(),
            missing.to_string_lossy().to_string(),
        ]),
        ExitCode::from(2)
    );
}

/// Verifies valid breakpoint-management scripts reach the reserved surface.
///
/// Inputs:
/// - Temporary debugger script using `list`, `enable`, `disable`, `remove`,
///   and `pause`.
///
/// Output:
/// - Reserved-debugger exit code.
///
/// Transformation:
/// - Runs through the public CLI so script command validation is proven at
///   the boundary editor tooling invokes.
#[test]
fn run_cli_routes_debug_breakpoint_management_script_to_native_admission() {
    let root = make_temp_dir("debug_breakpoint_management_script");
    let script = root.join("session.terldbg");
    let contents = concat!(
        "break app.Main.main\n",
        "list\n",
        "enable 1\n",
        "disable app.Main.main\n",
        "remove 1\n",
        "pause\n",
        "quit\n"
    );
    std::fs::write(&script, contents).expect("write debugger script");

    assert_eq!(
        run_cli(vec![
            "debug".to_string(),
            "--script".to_string(),
            script.to_string_lossy().to_string(),
        ]),
        ExitCode::from(1)
    );
}

/// Verifies malformed breakpoint-management scripts fail on the public path.
///
/// Inputs:
/// - Temporary debugger script containing `disable 0`.
///
/// Output:
/// - Invalid-usage exit code.
///
/// Transformation:
/// - Confirms scripted breakpoint selector validation runs before the
///   reserved debugger report.
#[test]
fn run_cli_rejects_debug_script_invalid_breakpoint_selector() {
    let root = make_temp_dir("debug_invalid_breakpoint_selector");
    let script = root.join("session.terldbg");
    std::fs::write(&script, "disable 0\n").expect("write debugger script");

    assert_eq!(
        run_cli(vec![
            "debug".to_string(),
            "--script".to_string(),
            script.to_string_lossy().to_string(),
        ]),
        ExitCode::from(2)
    );
}

/// Verifies malformed debugger invocations fail at the public command path.
///
/// Inputs:
/// - Synthetic bare `terlc debug` invocation.
///
/// Output:
/// - Invalid-usage exit code.
///
/// Transformation:
/// - Confirms the dispatcher reaches debugger argument validation and does not
///   silently treat a missing target as a successful no-op.
#[test]
fn run_cli_rejects_debug_command_without_target_or_script() {
    assert_eq!(run_cli(vec!["debug".to_string()]), ExitCode::from(2));
}

/// Verifies duplicate debugger scripts fail on the public command path.
///
/// Inputs:
/// - Synthetic `terlc debug --script one --script two` invocation.
///
/// Output:
/// - Invalid-usage exit code.
///
/// Transformation:
/// - Proves duplicate script rejection happens before file IO or the reserved
///   runtime report.
#[test]
fn run_cli_rejects_debug_command_duplicate_script() {
    assert_eq!(
        run_cli(vec![
            "debug".to_string(),
            "--script".to_string(),
            "one.terldbg".to_string(),
            "--script".to_string(),
            "two.terldbg".to_string(),
        ]),
        ExitCode::from(2)
    );
}

/// Verifies extra debugger targets fail on the public command path.
///
/// Inputs:
/// - Synthetic `terlc debug app other` invocation.
///
/// Output:
/// - Invalid-usage exit code.
///
/// Transformation:
/// - Keeps the command boundary unambiguous for future editor integrations.
#[test]
fn run_cli_rejects_debug_command_too_many_targets() {
    assert_eq!(
        run_cli(vec![
            "debug".to_string(),
            "app".to_string(),
            "other".to_string(),
        ]),
        ExitCode::from(2)
    );
}
