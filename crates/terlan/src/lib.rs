// `terlc serve` hosts generated scheduler owners and therefore includes the
// VM capability event pump. Unused low-level client modes remain VM-internal.
macro_rules! vm_capability_component {
    ($($item:item)*) => {
        $($item)*
    };
}

#[cfg(any(test, feature = "benchmark-tools"))]
macro_rules! vm_map_profile_component {
    ($($item:item)*) => {
        $(#[cfg(any(test, feature = "benchmark-tools"))] $item)*
    };
}

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
macro_rules! vm_code_server_test_component {
    ($($item:item)*) => {
        $(#[cfg(test)] $item)*
    };
}

pub mod accelerator_contract;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub mod backends;
#[cfg(feature = "benchmark-tools")]
pub mod benchmark;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub mod compiler;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) mod database_schema;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub mod formal_pipeline;
pub mod html;
#[cfg(feature = "editor-lsp")]
pub mod lsp;
pub mod native_worker;
pub mod package_registry;
#[cfg(feature = "quality-tools")]
pub mod quality;
pub mod runtime;
pub(crate) mod service_foundation;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub mod support;
#[cfg(all(
    feature = "serve-runtime-bin",
    not(test),
    not(feature = "native-codegen")
))]
#[path = "support_runtime.rs"]
pub mod support;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) mod template_inputs;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub mod validation;
pub mod vm;

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use compiler::hir as terlan_hir;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use compiler::purity as terlan_purity;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use compiler::syntax as terlan_syntax;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use compiler::typeck as terlan_typeck;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use compiler::value_lifecycle;
pub(crate) use html as terlan_html;
#[cfg(feature = "editor-lsp")]
pub(crate) use lsp as terlan_lsp;
#[cfg(feature = "quality-tools")]
pub(crate) use quality as terlan_quality;
pub(crate) use runtime::native as terlan_native;
pub(crate) use runtime::native_boundary as terlan_native_boundary;

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use std::path::PathBuf;
use std::process::ExitCode;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use validation::native_policy::NativePolicy;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use validation::target_profile::TargetProfile;

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod cli_dispatch;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
mod cli_usage;
mod commands;
mod web_route;

#[cfg(any(not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use cli_dispatch::run_cli;
#[cfg(test)]
use cli_dispatch::run_cli_with_repl;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
#[cfg(test)]
use cli_usage::debug_usage_lines;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use cli_usage::{print_command_usage, public_usage_lines};

/// Terminal color selection for human-readable diagnostics.
///
/// Inputs:
/// - Parsed from the global `--color` option.
///
/// Output:
/// - Color policy consumed by diagnostic rendering.
///
/// Transformation:
/// - Keeps terminal-color behavior separate from diagnostic format selection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

/// Top-level diagnostic serialization mode.
///
/// Inputs:
/// - Parsed from `--diagnostic-format` and `--color`.
///
/// Output:
/// - Text or JSON diagnostic mode shared by command handlers.
///
/// Transformation:
/// - Bundles text color policy with the text format while keeping JSON output
///   deterministic and color-free.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) enum DiagnosticFormat {
    Text { color: ColorChoice },
    Json,
}

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
impl Default for DiagnosticFormat {
    /// Returns the default diagnostic format for CLI invocations.
    ///
    /// Inputs:
    /// - None.
    ///
    /// Output:
    /// - Text diagnostics with automatic terminal color selection.
    ///
    /// Transformation:
    /// - Provides a stable default so command parsing can start from a complete
    ///   CLI state before global options are processed.
    fn default() -> Self {
        Self::Text {
            color: ColorChoice::Auto,
        }
    }
}

/// Documentation renderer retained for command-local `terlc doc` rendering.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
enum DocFormat {
    Markdown,
    #[default]
    Html,
    Json,
}

/// Parsed global CLI state shared with command handlers.
///
/// Inputs:
/// - Raw command-line arguments before the command verb and command-local args.
///
/// Output:
/// - Normalized global options such as output dirs, diagnostics, native policy,
///   and target profile.
///
/// Transformation:
/// - Separates command-independent flags from the verb-specific argument list.
#[derive(Clone)]
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
struct CliState {
    no_emit: bool,
    incremental: bool,
    timings: bool,
    experimental: bool,
    out_dir: PathBuf,
    cache_dir: Option<PathBuf>,
    trace_invalidation: bool,
    diagnostic_format: DiagnosticFormat,
    doc_format: DocFormat,
    native_policy: NativePolicy,
    target_profile: TargetProfile,
}

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
impl Default for CliState {
    /// Returns the baseline CLI state used when no global flags override it.
    fn default() -> Self {
        Self {
            no_emit: false,
            incremental: false,
            timings: false,
            experimental: false,
            out_dir: PathBuf::from("_build"),
            cache_dir: None,
            trace_invalidation: false,
            diagnostic_format: DiagnosticFormat::default(),
            doc_format: DocFormat::Html,
            native_policy: NativePolicy::NativeBoundaryOptional,
            target_profile: TargetProfile::Vm,
        }
    }
}

/// Parsed command verb and command-local arguments.
///
/// Inputs:
/// - Raw command-line arguments after global option extraction.
///
/// Output:
/// - Optional verb plus remaining args forwarded to the command module.
///
/// Transformation:
/// - Preserves command-local options for the top-level dispatcher.
#[derive(Default, Clone)]
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
struct CliCommand {
    verb: Option<String>,
    args: Vec<String>,
}

/// Prints the public `terlc` command summary.
///
/// Inputs:
/// - None; command list is owned by `public_usage_lines`.
///
/// Output:
/// - Usage lines written to stdout.
///
/// Transformation:
/// - Emits only public release commands and hides private compiler helpers.
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
fn print_usage() {
    public_usage_lines()
        .iter()
        .for_each(|line| println!("{line}"));
}

/// Runs the `terlc` process entrypoint through the library-owned dispatcher.
///
/// Inputs:
/// - Process arguments from `std::env::args`.
///
/// Output:
/// - Process exit code returned by the selected command.
///
/// Transformation:
/// - Drops the executable path and delegates all CLI behavior to `run_cli`.
#[cfg(any(not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub fn run_cli_from_env() -> ExitCode {
    let args = std::env::args().skip(1).collect();
    let command_stack_bytes = if std::env::var_os("TERLAN_SERVE_RUNTIME_ONLY").is_some() {
        2 * 1024 * 1024
    } else {
        32 * 1024 * 1024
    };
    match std::thread::Builder::new()
        .name("terlc-command".to_string())
        .stack_size(command_stack_bytes)
        .spawn(move || run_cli(args))
    {
        Ok(command) => match command.join() {
            Ok(exit_code) => exit_code,
            Err(_) => {
                eprintln!("terlc command worker panicked");
                ExitCode::from(1)
            }
        },
        Err(error) => {
            eprintln!("cannot start terlc command worker: {error}");
            ExitCode::from(1)
        }
    }
}

/// Runs only the persisted-image serve command in the compiler-free binary.
pub fn run_serve_runtime(mut args: Vec<String>) -> ExitCode {
    if args.first().is_some_and(|argument| argument == "serve") {
        args.remove(0);
    }
    std::env::set_var("TERLAN_SERVE_RUNTIME_ONLY", "1");
    commands::serve::run_serve_runtime(args)
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
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
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
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
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
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
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
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
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
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
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
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
fn print_command_help(command: &str) -> ExitCode {
    if print_command_usage(command) {
        ExitCode::SUCCESS
    } else {
        eprintln!("unknown command: {}", command);
        print_usage();
        ExitCode::from(2)
    }
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
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
fn command_has_usage(command: &str) -> bool {
    matches!(
        command,
        "help"
            | "init"
            | "bind"
            | "check"
            | "build"
            | "run"
            | "scripts"
            | "package"
            | "clean"
            | "doctor"
            | "inspect"
            | "serve"
            | "integration-test"
            | "static"
            | "support"
            | "emit-js"
            | "test"
            | "interface"
            | "doc"
            | "api"
            | "db"
            | "debug"
            | "doctest"
            | "emit-native-metadata"
            | "repl"
            | "fmt"
            | "lint"
            | "migrate"
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
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
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
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
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

/// Parses global options and separates the command-local argument tail.
///
/// Inputs:
/// - `args`: raw command-line arguments after the executable name.
///
/// Output:
/// - Parsed `CliState` and `CliCommand`.
///
/// Transformation:
/// - Consumes known global flags until the first command verb, forwarding
///   unknown or command-local options to the selected command.
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
fn parse_args(args: Vec<String>) -> (CliState, CliCommand) {
    let mut state = CliState {
        no_emit: false,
        incremental: false,
        timings: false,
        experimental: false,
        out_dir: PathBuf::from("_build"),
        cache_dir: None,
        trace_invalidation: false,
        diagnostic_format: DiagnosticFormat::default(),
        doc_format: DocFormat::Html,
        native_policy: NativePolicy::NativeBoundaryOptional,
        target_profile: TargetProfile::Vm,
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
            "--timings" => {
                state.timings = true;
                i += 1;
            }
            "--experimental" => {
                state.experimental = true;
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
                state.native_policy = match NativePolicy::from_cli(args[i + 1].as_str()) {
                    Some(policy) => policy,
                    None => {
                        let other = &args[i + 1];
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
                    value if value == "erlang" || value.ends_with("-erlang") => {
                        eprintln!(
                            "target profile `{}` was removed from the public CLI; the compiler selects the VM profile by default",
                            value
                        );
                        return (
                            CliState::default(),
                            CliCommand {
                                verb: None,
                                args: vec![],
                            },
                        );
                    }
                    "vm" | "terlan-vm" => TargetProfile::Vm,
                    "js" | "js.shared" => TargetProfile::JsShared,
                    "js.browser" => TargetProfile::JsBrowser,
                    "js.worker" => TargetProfile::JsWorker,
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
mod support_test;
#[cfg(test)]
mod tests;
