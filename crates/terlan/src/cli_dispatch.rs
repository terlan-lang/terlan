use super::*;

/// Runs the Terlan CLI dispatcher.
///
/// Inputs:
/// - `args`: command-line arguments after the executable name.
///
/// Output:
/// - Exit code from help/version handling, argument parsing, or command module
///   execution.
///
/// Transformation:
/// - Handles top-level help/version fast paths, parses global options, then
///   routes the command to the implementation module.
#[cfg(any(not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(super) fn run_cli(args: Vec<String>) -> ExitCode {
    run_cli_with_repl(args, commands::repl::run)
}

/// Runs the CLI dispatcher with an explicit REPL entry point.
///
/// The indirection keeps ordinary command routing identical while allowing
/// tests and embedders to supply bounded input instead of inheriting ambient
/// process stdin.
#[cfg(any(not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(super) fn run_cli_with_repl<F>(args: Vec<String>, repl: F) -> ExitCode
where
    F: FnOnce(CliCommand, CliState) -> ExitCode,
{
    if args.is_empty() {
        print_usage();
        return ExitCode::from(2);
    }
    if is_help_request(&args) {
        print_usage();
        return ExitCode::SUCCESS;
    }
    if is_version_request(&args) {
        print_version();
        return ExitCode::SUCCESS;
    }
    if let Some(command) = command_help_request(&args) {
        return print_command_help(command);
    }
    if let Some(command) = command_local_help_request(&args) {
        print_command_usage(command);
        return ExitCode::SUCCESS;
    }

    let (state, cmd) = parse_args(args);
    if cmd.verb.is_none() {
        print_usage();
        return ExitCode::from(2);
    }
    if let Some(exit_code) = run_parsed_help_request(&cmd) {
        return exit_code;
    }

    let verb = cmd
        .verb
        .as_deref()
        .expect("internal parser error: command missing");

    match verb {
        "init" => commands::init::run(cmd),
        "bind" => commands::bind::run(cmd),
        "build" => commands::build::run(cmd, state),
        "run" => commands::run::run(cmd, state),
        "scripts" => commands::scripts::run(cmd),
        "package" => commands::build::run_package_command(cmd, state),
        "clean" => commands::clean::run(cmd),
        "doctor" => commands::doctor::run(cmd),
        "inspect" => commands::inspect::run(cmd),
        "serve" => commands::serve::run(cmd, state),
        "integration-test" => commands::integration_test::run(cmd, state),
        "static" => commands::static_site::run(cmd, state),
        "support" => commands::support_bundle::run(&cmd.args),
        "check" => commands::check::run(cmd, state),
        "emit-static" => commands::static_site::run_emit_static(cmd, state),
        "serve-static" => commands::static_site::run_serve_static(cmd, state),
        "emit-js" => commands::emit_js::run(&cmd.args, &state),
        "test" => commands::test::run(cmd, state),
        "interface" => commands::interface::run(&cmd.args, &state),
        "doc" => commands::doc::run(cmd, state),
        "api" => commands::api::run(cmd, state),
        "deploy" => commands::deploy::run(cmd, state),
        "vm" => commands::vm::run(cmd, state),
        "db" => commands::db::run(cmd),
        "debug" => commands::debug::run(cmd, state.diagnostic_format),
        "doctest" => commands::doc::run_doctest(cmd, state),
        "emit-native-metadata" => commands::emit_native_metadata::run(cmd, state),
        "repl" => repl(cmd, state),
        "fmt" => commands::fmt::run(&cmd.args),
        "lint" => commands::lint::run(&cmd.args),
        "migrate" => commands::migrate::run(cmd),
        "hover" => commands::hover::run(cmd, state),
        "lsp" => commands::lsp::run(&cmd.args),
        "syntax-contract" => commands::syntax_contract::run(&cmd.args),
        "__sql-runtime" => commands::sql_runtime::run(&cmd.args),
        "__native-vector-runtime" => commands::native_vector_runtime::run(&cmd.args),
        "version" => run_version_command(&cmd),
        unknown => {
            eprintln!("unknown command: {unknown}");
            print_usage();
            ExitCode::from(2)
        }
    }
}
