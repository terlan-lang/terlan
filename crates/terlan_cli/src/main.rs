use std::path::PathBuf;
use std::process::ExitCode;

mod formal_pipeline;
mod support;
mod validation;
use validation::native_policy::NativePolicy;
use validation::target_profile::TargetProfile;

mod commands;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiagnosticFormat {
    Text { color: ColorChoice },
    Json,
}

impl Default for DiagnosticFormat {
    fn default() -> Self {
        Self::Text {
            color: ColorChoice::Auto,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum DocFormat {
    Markdown,
    #[default]
    Html,
    Json,
}

#[derive(Default, Clone)]
struct CliState {
    no_emit: bool,
    incremental: bool,
    out_dir: PathBuf,
    cache_dir: Option<PathBuf>,
    trace_invalidation: bool,
    diagnostic_format: DiagnosticFormat,
    doc_format: DocFormat,
    native_policy: NativePolicy,
    target_profile: TargetProfile,
}

#[derive(Default)]
struct CliCommand {
    verb: Option<String>,
    args: Vec<String>,
}

fn print_usage() {
    println!("terlc help [command]");
    println!("terlc init [project-name]");
    println!("terlc bind rust --crate <crate-name> --out <dir>");
    println!("terlc check <file.terl|file.terli|dir> [--emit-phase-manifest <path>]");
    println!("terlc build [file.terl|dir] [--target erlang] [--out-dir <dir>]");
    println!("terlc emit <file.terl> [--out-dir <dir>] [--no-emit] [--incremental]");
    println!(
        "terlc emit-static <file.terl> [--out-dir <dir>] [--validate-output] [--asset-include <pattern>] [--asset-exclude <pattern>]"
    );
    println!(
        "terlc serve-static <file.terl> [--out-dir <dir>] [--host <host>] [--port <port>] [--poll-ms <ms>] [--source-dir <dir>] [--validate-output]"
    );
    println!("terlc emit-js <file.terl> [--out-dir <dir>] [--declarations]");
    println!(
        "terlc test [file.terl|dir] [--target erlang] [--emit-test-manifest <path>] [--emit-test-result-manifest <path>]"
    );
    println!("terlc interface <file.terli> [--out-dir <dir>]");
    println!(
        "terlc doc <file.terl|dir> [--format html|markdown|json] [--out-dir <dir>] [--check] [--missing-docs]"
    );
    println!("terlc doctest <file.terl>");
    println!("terlc emit-native-metadata <file.terl> [--out-dir <dir>]");
    println!("terlc repl [--help] [<file.terl|project-dir>]");
    println!("terlc fmt <file.terl>");
    println!("terlc hover <file.terl> --line <line> (--column|--col) <column>");
    println!("terlc lsp --stdio");
    println!("terlc version | terlc --version | terlc -V");
    println!("Maintainer/release tooling:");
    println!("terlc syntax-contract [--fingerprint] [--out <path>]");
    println!("terlc syntax-contract --check <path>");
    println!("Global options: --diagnostic-format text|json --color auto|always|never");
    println!(
        "               --native-policy pure|safe_native_optional|safe_native_required [--target-profile erlang|a0-erlang|a0.1-erlang|a0.2-erlang|a0.3-erlang|a0.4-erlang|a0.5-erlang|a0.6-erlang|a0.7-erlang|a0.8-erlang|a0.9-erlang|a0.10-erlang|a0.11-erlang|a0.12-erlang|a0.13-erlang|a0.14-erlang|a0.15-erlang|a0.16-erlang|a0.17-erlang|a0.18-erlang|a0.19-erlang|a0.20-erlang|a0.21-erlang|core-v0]"
    );
}

#[cfg(not(test))]
fn main() -> ExitCode {
    run_cli(std::env::args().skip(1).collect())
}

fn run_cli(args: Vec<String>) -> ExitCode {
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
        "check" => commands::check::run(cmd, state),
        "emit" => commands::emit::run(cmd, state),
        "emit-static" => commands::static_site::run_emit_static(cmd, state),
        "serve-static" => commands::static_site::run_serve_static(cmd, state),
        "emit-js" => commands::emit_js::run(&cmd.args, &state),
        "test" => commands::test::run(cmd, state),
        "interface" => commands::interface::run(&cmd.args, &state),
        "doc" => commands::doc::run(cmd, state),
        "doctest" => commands::doc::run_doctest(cmd, state),
        "emit-native-metadata" => commands::emit_native_metadata::run(cmd, state),
        "repl" => commands::repl::run(cmd, state),
        "fmt" => commands::fmt::run(&cmd.args),
        "hover" => commands::hover::run(cmd, state),
        "lsp" => commands::lsp::run(&cmd.args),
        "syntax-contract" => commands::syntax_contract::run(&cmd.args),
        "version" => run_version_command(&cmd),
        unknown => {
            eprintln!("unknown command: {}", unknown);
            print_usage();
            ExitCode::from(2)
        }
    }
}

/// Returns whether the raw CLI arguments request top-level help.
///
/// Inputs:
/// - `args`: raw command-line arguments after the executable name.
///
/// Output:
/// - `true` when the invocation is exactly `help`, `--help`, `-h`,
///   `help --help`, or `help -h`.
/// - `false` for command-local help such as `repl --help`, which must be
///   routed to the command implementation.
///
/// Transformation:
/// - Performs exact help-shape matching with no side effects.
fn is_help_request(args: &[String]) -> bool {
    matches!(
        args,
        [arg] if matches!(arg.as_str(), "help" | "--help" | "-h")
    ) || matches!(
        args,
        [command, flag]
            if command == "help" && matches!(flag.as_str(), "--help" | "-h")
    )
}

/// Returns whether the raw CLI arguments request top-level version output.
///
/// Inputs:
/// - `args`: raw command-line arguments after the executable name.
///
/// Output:
/// - `true` when the invocation is exactly `--version` or `-V`.
/// - `false` for all command-local arguments and non-version commands.
///
/// Transformation:
/// - Performs an exact single-argument match with no side effects.
fn is_version_request(args: &[String]) -> bool {
    args.len() == 1 && matches!(args[0].as_str(), "--version" | "-V")
}

/// Prints the compiler version in the public CLI format.
///
/// Inputs:
/// - None; the version is read from Cargo package metadata at compile time.
///
/// Output:
/// - Writes `terlc <version>` to standard output.
///
/// Transformation:
/// - Formats the compile-time package version without mutating CLI state.
fn print_version() {
    println!("terlc {}", env!("CARGO_PKG_VERSION"));
}

/// Executes the `version` CLI command.
///
/// Inputs:
/// - `cmd`: parsed version command with command-local arguments.
///
/// Output:
/// - `ExitCode::SUCCESS` when printing the compiler version or version command
///   help.
/// - `ExitCode::from(2)` when unexpected arguments are supplied.
///
/// Transformation:
/// - Treats bare `terlc version` as version output, `terlc version --help` and
///   `terlc version -h` as command usage, and all other arguments as malformed
///   command invocations.
fn run_version_command(cmd: &CliCommand) -> ExitCode {
    match cmd.args.as_slice() {
        [] => {
            print_version();
            ExitCode::SUCCESS
        }
        [arg] if matches!(arg.as_str(), "--help" | "-h") => {
            print_command_usage("version");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("terlc version does not accept arguments");
            print_command_usage("version");
            ExitCode::from(2)
        }
    }
}

/// Returns the command requested by `terlc help <command>`.
///
/// Inputs:
/// - `args`: raw command-line arguments after the executable name.
///
/// Output:
/// - `Some(command)` when the invocation has exactly the `help <command>`
///   shape.
/// - `None` for top-level help, command-local help, and other invocations.
///
/// Transformation:
/// - Inspects the argument vector without validating whether the command name
///   is known; validation is owned by `print_command_help`.
fn command_help_request(args: &[String]) -> Option<&str> {
    if args.len() == 2 && args[0] == "help" {
        Some(args[1].as_str())
    } else {
        None
    }
}

/// Prints help for one known command and returns the matching exit code.
///
/// Inputs:
/// - `command`: command name supplied after `terlc help`.
///
/// Output:
/// - `ExitCode::SUCCESS` when command usage was printed.
/// - `ExitCode::from(2)` when the command is unknown.
///
/// Transformation:
/// - Delegates known command text to `print_command_usage`; unknown commands
///   emit a stable error before the global usage summary.
fn print_command_help(command: &str) -> ExitCode {
    if print_command_usage(command) {
        ExitCode::SUCCESS
    } else {
        eprintln!("unknown command: {}", command);
        print_usage();
        ExitCode::from(2)
    }
}

/// Prints usage for one known command.
///
/// Inputs:
/// - `command`: command name to describe.
///
/// Output:
/// - `true` when the command is known and usage was printed.
/// - `false` when the command is unknown.
///
/// Transformation:
/// - Maps public command names to concise usage lines without parsing command
///   arguments or touching the filesystem.
fn print_command_usage(command: &str) -> bool {
    match command {
        "help" => println!("terlc help [command]"),
        "init" => println!("terlc init [project-name]"),
        "bind" => println!("terlc bind rust --crate <crate-name> --out <dir>"),
        "check" => println!("terlc check <file.terl|file.terli|dir> [--emit-phase-manifest <path>]"),
        "build" => println!("terlc build [file.terl|dir] [--target erlang] [--out-dir <dir>]"),
        "emit" => println!("terlc emit <file.terl> [--out-dir <dir>] [--no-emit] [--incremental]"),
        "emit-static" => println!(
            "terlc emit-static <file.terl> [--out-dir <dir>] [--validate-output] [--asset-include <pattern>] [--asset-exclude <pattern>]"
        ),
        "serve-static" => println!(
            "terlc serve-static <file.terl> [--out-dir <dir>] [--host <host>] [--port <port>] [--poll-ms <ms>] [--source-dir <dir>] [--validate-output]"
        ),
        "emit-js" => println!("terlc emit-js <file.terl> [--out-dir <dir>] [--declarations]"),
        "test" => println!(
            "terlc test [file.terl|dir] [--target erlang] [--emit-test-manifest <path>] [--emit-test-result-manifest <path>]"
        ),
        "interface" => println!("terlc interface <file.terli> [--out-dir <dir>]"),
        "doc" => println!(
            "terlc doc <file.terl|dir|std> [--format html|markdown|json] [--out-dir <dir>] [--check] [--missing-docs]"
        ),
        "doctest" => println!("terlc doctest <file.terl>"),
        "emit-native-metadata" => {
            println!("terlc emit-native-metadata <file.terl> [--out-dir <dir>]")
        }
        "repl" => {
            println!("terlc repl [--help|-h] [<file.terl|project-dir>]");
            println!("Interactive mode accepts normal Terlan entries terminated with '.'.");
            println!("Available commands: :help, :quit, :reset, :load <file.terl|project-dir>");
        }
        "fmt" => println!("terlc fmt <file.terl>"),
        "hover" => println!("terlc hover <file.terl> --line <line> (--column|--col) <column>"),
        "lsp" => println!("terlc lsp --stdio"),
        "version" => println!("terlc version | terlc --version | terlc -V"),
        "syntax-contract" => {
            println!("terlc syntax-contract [--fingerprint] [--out <path>]");
            println!("terlc syntax-contract --check <path>");
        }
        _ => return false,
    }
    true
}

/// Returns whether a command has registered usage text.
///
/// Inputs:
/// - `command`: command name to classify.
///
/// Output:
/// - `true` when `print_command_usage` can render command-local usage.
/// - `false` when the command is unknown to the public dispatcher.
///
/// Transformation:
/// - Classifies command names without printing or parsing command arguments.
fn command_has_usage(command: &str) -> bool {
    matches!(
        command,
        "help"
            | "init"
            | "bind"
            | "check"
            | "build"
            | "emit"
            | "emit-static"
            | "serve-static"
            | "emit-js"
            | "test"
            | "interface"
            | "doc"
            | "doctest"
            | "emit-native-metadata"
            | "repl"
            | "fmt"
            | "hover"
            | "lsp"
            | "version"
            | "syntax-contract"
    )
}

/// Handles parsed help and version requests after global options are removed.
///
/// Inputs:
/// - `cmd`: parsed command verb and command-local arguments.
///
/// Output:
/// - `Some(exit_code)` when the parsed command is a help or version request
///   that should stop normal command execution.
/// - `None` when the parsed command should continue to its normal handler.
///
/// Transformation:
/// - Re-applies the same help/version contract used by raw fast paths after
///   `parse_args` has stripped global options such as `--color never`.
fn run_parsed_help_request(cmd: &CliCommand) -> Option<ExitCode> {
    let verb = cmd.verb.as_deref()?;
    if matches!(verb, "--help" | "-h") && cmd.args.is_empty() {
        print_usage();
        return Some(ExitCode::SUCCESS);
    }
    if matches!(verb, "--version" | "-V") && cmd.args.is_empty() {
        print_version();
        return Some(ExitCode::SUCCESS);
    }
    if verb == "help" {
        return Some(match cmd.args.as_slice() {
            [] => {
                print_usage();
                ExitCode::SUCCESS
            }
            [arg] if matches!(arg.as_str(), "--help" | "-h") => {
                print_usage();
                ExitCode::SUCCESS
            }
            [command] => print_command_help(command),
            _ => {
                eprintln!("terlc help accepts at most one command");
                print_command_usage("help");
                ExitCode::from(2)
            }
        });
    }
    if cmd.args.len() == 1
        && matches!(cmd.args[0].as_str(), "--help" | "-h")
        && command_has_usage(verb)
    {
        print_command_usage(verb);
        return Some(ExitCode::SUCCESS);
    }
    None
}

/// Returns the known command that asked for command-local help.
///
/// Inputs:
/// - `args`: raw command-line arguments after the executable name.
///
/// Output:
/// - `Some(command)` for a known command followed by `--help` or `-h`.
/// - `None` for unknown commands, non-help arguments, or malformed shapes.
///
/// Transformation:
/// - Performs an exact two-argument match so help requests do not enter
///   command parsers that would otherwise report them as invalid options.
fn command_local_help_request(args: &[String]) -> Option<&str> {
    if args.len() == 2
        && matches!(args[1].as_str(), "--help" | "-h")
        && command_has_usage(args[0].as_str())
    {
        Some(args[0].as_str())
    } else {
        None
    }
}

fn parse_args(args: Vec<String>) -> (CliState, CliCommand) {
    let mut state = CliState {
        no_emit: false,
        incremental: false,
        out_dir: PathBuf::from("_build"),
        cache_dir: None,
        trace_invalidation: false,
        diagnostic_format: DiagnosticFormat::default(),
        doc_format: DocFormat::Html,
        native_policy: NativePolicy::SafeNativeOptional,
        target_profile: TargetProfile::Erlang,
    };

    let mut cmd = CliCommand::default();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--no-emit" => {
                state.no_emit = true;
                i += 1;
            }
            "--incremental" => {
                state.incremental = true;
                i += 1;
            }
            "--trace-invalidation" => {
                state.trace_invalidation = true;
                i += 1;
            }
            "--validate-output" => {
                cmd.args.push(args[i].clone());
                i += 1;
            }
            "--out-dir" => {
                if i + 1 >= args.len() {
                    eprintln!("--out-dir requires a value");
                    return (
                        CliState::default(),
                        CliCommand {
                            verb: None,
                            args: vec![],
                        },
                    );
                }
                state.out_dir = PathBuf::from(&args[i + 1]);
                i += 2;
            }
            "--cache-dir" => {
                if i + 1 >= args.len() {
                    eprintln!("--cache-dir requires a value");
                    return (
                        CliState::default(),
                        CliCommand {
                            verb: None,
                            args: vec![],
                        },
                    );
                }
                state.cache_dir = Some(PathBuf::from(&args[i + 1]));
                i += 2;
            }
            "--diagnostic-format" => {
                if i + 1 >= args.len() {
                    eprintln!("--diagnostic-format requires a value");
                    return (
                        CliState::default(),
                        CliCommand {
                            verb: None,
                            args: vec![],
                        },
                    );
                }
                state.diagnostic_format = match args[i + 1].as_str() {
                    "text" => DiagnosticFormat::Text {
                        color: support::diagnostic_color(state.diagnostic_format),
                    },
                    "json" => DiagnosticFormat::Json,
                    other => {
                        eprintln!("unsupported diagnostic format: {}", other);
                        return (
                            CliState::default(),
                            CliCommand {
                                verb: None,
                                args: vec![],
                            },
                        );
                    }
                };
                i += 2;
            }
            "--color" => {
                if i + 1 >= args.len() {
                    eprintln!("--color requires a value");
                    return (
                        CliState::default(),
                        CliCommand {
                            verb: None,
                            args: vec![],
                        },
                    );
                }
                let color = match args[i + 1].as_str() {
                    "auto" => ColorChoice::Auto,
                    "always" => ColorChoice::Always,
                    "never" => ColorChoice::Never,
                    other => {
                        eprintln!("unsupported color mode: {}", other);
                        return (
                            CliState::default(),
                            CliCommand {
                                verb: None,
                                args: vec![],
                            },
                        );
                    }
                };
                if matches!(state.diagnostic_format, DiagnosticFormat::Text { .. }) {
                    state.diagnostic_format = DiagnosticFormat::Text { color };
                }
                i += 2;
            }
            "--format" => {
                if i + 1 >= args.len() {
                    eprintln!("--format requires a value");
                    return (
                        CliState::default(),
                        CliCommand {
                            verb: None,
                            args: vec![],
                        },
                    );
                }
                state.doc_format = match args[i + 1].as_str() {
                    "markdown" => DocFormat::Markdown,
                    "html" => DocFormat::Html,
                    "json" => DocFormat::Json,
                    other => {
                        eprintln!("unsupported doc format: {}", other);
                        return (
                            CliState::default(),
                            CliCommand {
                                verb: None,
                                args: vec![],
                            },
                        );
                    }
                };
                i += 2;
            }
            "--native-policy" => {
                if i + 1 >= args.len() {
                    eprintln!("--native-policy requires a value");
                    return (
                        CliState::default(),
                        CliCommand {
                            verb: None,
                            args: vec![],
                        },
                    );
                }
                state.native_policy = match args[i + 1].as_str() {
                    "pure" => NativePolicy::Pure,
                    "safe_native_optional" => NativePolicy::SafeNativeOptional,
                    "safe_native_required" => NativePolicy::SafeNativeRequired,
                    other => {
                        eprintln!("unsupported native policy: {}", other);
                        return (
                            CliState::default(),
                            CliCommand {
                                verb: None,
                                args: vec![],
                            },
                        );
                    }
                };
                i += 2;
            }
            "--target-profile" => {
                if i + 1 >= args.len() {
                    eprintln!("--target-profile requires a value");
                    return (
                        CliState::default(),
                        CliCommand {
                            verb: None,
                            args: vec![],
                        },
                    );
                }
                state.target_profile = match args[i + 1].as_str() {
                    "erlang" => TargetProfile::Erlang,
                    "a0-erlang" => TargetProfile::A0Erlang,
                    "a0.1-erlang" => TargetProfile::A01Erlang,
                    "a0.2-erlang" => TargetProfile::A02Erlang,
                    "a0.3-erlang" => TargetProfile::A03Erlang,
                    "a0.4-erlang" => TargetProfile::A04Erlang,
                    "a0.5-erlang" => TargetProfile::A05Erlang,
                    "a0.6-erlang" => TargetProfile::A06Erlang,
                    "a0.7-erlang" => TargetProfile::A07Erlang,
                    "a0.8-erlang" => TargetProfile::A08Erlang,
                    "a0.9-erlang" => TargetProfile::A09Erlang,
                    "a0.10-erlang" => TargetProfile::A010Erlang,
                    "a0.11-erlang" => TargetProfile::A011Erlang,
                    "a0.12-erlang" => TargetProfile::A012Erlang,
                    "a0.13-erlang" => TargetProfile::A013Erlang,
                    "a0.14-erlang" => TargetProfile::A014Erlang,
                    "a0.15-erlang" => TargetProfile::A015Erlang,
                    "a0.16-erlang" => TargetProfile::A016Erlang,
                    "a0.17-erlang" => TargetProfile::A017Erlang,
                    "a0.18-erlang" => TargetProfile::A018Erlang,
                    "a0.19-erlang" => TargetProfile::A019Erlang,
                    "a0.20-erlang" => TargetProfile::A020Erlang,
                    "a0.21-erlang" => TargetProfile::A021Erlang,
                    "core-v0" => TargetProfile::CoreV0,
                    other => {
                        eprintln!("unsupported target profile: {}", other);
                        return (
                            CliState::default(),
                            CliCommand {
                                verb: None,
                                args: vec![],
                            },
                        );
                    }
                };
                i += 2;
            }
            "--stdlib" => {
                if i + 1 >= args.len() {
                    eprintln!("--stdlib requires a value");
                    return (
                        CliState::default(),
                        CliCommand {
                            verb: None,
                            args: vec![],
                        },
                    );
                }
                i += 2;
            }
            _ => {
                if cmd.verb.is_none() {
                    cmd.verb = Some(args[i].clone());
                } else {
                    cmd.args.push(args[i].clone());
                }
                i += 1;
            }
        }
    }

    (state, cmd)
}

#[cfg(test)]
mod main_test;
