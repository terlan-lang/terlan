use std::process::ExitCode;

use crate::{CliCommand, CliState};

use super::{run_emit_static, run_serve_static};

/// Executes the public `static` CLI command group.
///
/// Inputs:
/// - `cmd`: parsed CLI command whose first command-local argument selects the
///   static subcommand.
/// - `state`: global CLI state shared with the underlying static runners.
///
/// Output:
/// - Success when the selected static subcommand succeeds.
/// - Exit code `2` when the subcommand is missing or unknown.
///
/// Transformation:
/// - Adapts the public `terlc static <emit|serve|check>` surface to the older
///   internal static emit and serve command implementations without exposing
///   those internal verbs in release-facing help.
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    let Some((subcommand, rest)) = cmd.args.split_first() else {
        print_static_usage();
        return ExitCode::from(2);
    };
    match subcommand.as_str() {
        "emit" => run_emit_static(
            CliCommand {
                verb: Some("static".to_string()),
                args: rest.to_vec(),
            },
            state,
        ),
        "serve" => run_serve_static(
            CliCommand {
                verb: Some("static".to_string()),
                args: rest.to_vec(),
            },
            state,
        ),
        "check" => run_serve_static(
            CliCommand {
                verb: Some("static".to_string()),
                args: static_check_args(rest),
            },
            state,
        ),
        "help" | "--help" | "-h" => {
            print_static_usage();
            ExitCode::SUCCESS
        }
        unknown => {
            eprintln!("unknown static subcommand: {unknown}");
            print_static_usage();
            ExitCode::from(2)
        }
    }
}

/// Builds command-local args for `terlc static check`.
///
/// Inputs:
/// - `args`: arguments after `static check`.
///
/// Output:
/// - Arguments accepted by the underlying static serve check-only runner.
///
/// Transformation:
/// - Preserves user arguments while appending `--check` and
///   `--validate-output` when they were not supplied explicitly.
pub(crate) fn static_check_args(args: &[String]) -> Vec<String> {
    static_check_args_impl(args)
}

/// Shared implementation for static check argument construction.
///
/// Inputs:
/// - `args`: arguments after `static check`.
///
/// Output:
/// - Arguments with required check/validation flags present.
///
/// Transformation:
/// - Keeps test-visible and release-private wrappers behavior-identical without
///   widening the release API.
fn static_check_args_impl(args: &[String]) -> Vec<String> {
    let mut next = args.to_vec();
    push_flag_if_missing(&mut next, "--check");
    push_flag_if_missing(&mut next, "--validate-output");
    next
}

/// Prints usage for the public static-site command group.
///
/// Inputs:
/// - No runtime input.
///
/// Output:
/// - Writes static command usage to stdout.
///
/// Transformation:
/// - Keeps public static workflow help in one place while delegating execution
///   to the existing static-site runners.
fn print_static_usage() {
    println!(
        "terlc static emit <file.terl> [--out-dir <dir>] [--validate-output] [--base-path <path>] [--asset-include <pattern>] [--asset-exclude <pattern>]"
    );
    println!(
        "terlc static serve <file.terl> [--out-dir <dir>] [--host <host>] [--port <port>] [--poll-ms <ms>] [--source-dir <dir>] [--validate-output] [--base-path <path>]"
    );
    println!(
        "terlc static check <file.terl> [--out-dir <dir>] [--base-path <path>] [--asset-include <pattern>] [--asset-exclude <pattern>]"
    );
}

/// Adds a flag to an argument vector when it is absent.
///
/// Inputs:
/// - `args`: mutable command-local argument vector.
/// - `flag`: flag spelling to require.
///
/// Output:
/// - No return value; `args` may be extended by one item.
///
/// Transformation:
/// - Performs exact string comparison so command construction remains
///   deterministic and does not inspect flag values.
fn push_flag_if_missing(args: &mut Vec<String>, flag: &str) {
    if !args.iter().any(|arg| arg == flag) {
        args.push(flag.to_string());
    }
}
