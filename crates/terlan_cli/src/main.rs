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
    #[default]
    Markdown,
    Html,
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
    println!("terlc init [project-name]");
    println!("terlc check <file.tl|file.tli|dir> [--emit-phase-manifest <path>]");
    println!("terlc build [file.tl|dir] [--target erlang] [--out-dir <dir>]");
    println!("terlc emit <file.tl> [--out-dir <dir>] [--no-emit] [--incremental]");
    println!(
        "terlc emit-static <file.tl> [--out-dir <dir>] [--validate-output] [--asset-include <pattern>] [--asset-exclude <pattern>]"
    );
    println!(
        "terlc serve-static <file.tl> [--out-dir <dir>] [--host <host>] [--port <port>] [--poll-ms <ms>] [--source-dir <dir>] [--validate-output]"
    );
    println!("terlc emit-js <file.tl> [--out-dir <dir>] [--declarations]");
    println!(
        "terlc test <file.tl> [--target erlang] [--emit-test-manifest <path>] [--emit-test-result-manifest <path>]"
    );
    println!("terlc interface <file.tli> [--out-dir <dir>]");
    println!(
        "terlc doc <file.tl|dir> [--format markdown|html] [--out-dir <dir>] [--check] [--missing-docs]"
    );
    println!("terlc doctest <file.tl>");
    println!("terlc emit-native-metadata <file.tl> [--out-dir <dir>]");
    println!("terlc repl [--help] [<file.tl>]");
    println!("terlc fmt <file.tl>");
    println!("terlc hover <file.tl> --line <line> (--column|--col) <column>");
    println!("terlc lsp --stdio");
    println!("terlc version");
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

    let (state, cmd) = parse_args(args);
    if cmd.verb.is_none() {
        print_usage();
        return ExitCode::from(2);
    }

    let verb = cmd
        .verb
        .as_deref()
        .expect("internal parser error: command missing");

    match verb {
        "init" => commands::init::run(cmd),
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
        "version" => {
            println!("terlc {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        unknown => {
            eprintln!("unknown command: {}", unknown);
            print_usage();
            ExitCode::from(2)
        }
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
        doc_format: DocFormat::Markdown,
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
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;
    use crate::commands::static_site::*;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};
    use terlan_hir::resolve_syntax_module_output_with_interfaces;
    use terlan_syntax::{
        parse_module_as_syntax_output, SyntaxDeclarationPayload, SyntaxModuleOutput,
    };

    use crate::validation::template_contract::type_check_syntax_module_output_with_templates;

    fn make_temp_dir(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        path.push(format!(
            "terlan_cli_tests_{}_{}_{}",
            name,
            std::process::id(),
            now
        ));
        fs::create_dir_all(&path).expect("create temp dir");
        path
    }

    fn fixture(path: &Path, contents: &str) -> String {
        let file = path.join("fixture.tl");
        fs::write(&file, contents).expect("write fixture");
        file.to_string_lossy().to_string()
    }

    /// Verifies CLI argument parsing uses the public default build directory.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing a build command with no
    ///   `--out-dir` override.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the global output directory is
    ///   `_build`, matching the 0.0.1 first-run artifact contract.
    #[test]
    fn parse_args_defaults_output_directory_to_build() {
        let (state, cmd) = parse_args(vec!["build".into()]);

        assert_eq!(state.out_dir, PathBuf::from("_build"));
        assert_eq!(cmd.verb.as_deref(), Some("build"));
        assert!(cmd.args.is_empty());
    }

    /// Verifies CLI argument parsing accepts the portable CoreIR v0 target
    /// profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile core-v0`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::CoreV0` while preserving the command and source path.
    #[test]
    fn parse_args_accepts_core_v0_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "core-v0".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::CoreV0);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the frozen A0 Erlang artifact
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A0Erlang` while preserving the command and source path.
    #[test]
    fn parse_args_accepts_a0_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A0Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.1 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.1-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A01Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_1_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.1-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A01Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.2 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.2-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A02Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_2_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.2-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A02Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.3 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.3-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A03Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_3_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.3-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A03Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.4 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.4-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A04Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_4_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.4-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A04Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.5 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.5-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A05Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_5_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.5-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A05Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.6 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.6-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A06Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_6_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.6-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A06Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.7 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.7-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A07Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_7_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.7-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A07Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.8 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.8-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A08Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_8_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.8-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A08Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.9 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.9-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A09Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_9_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.9-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A09Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.10 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.10-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A010Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_10_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.10-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A010Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.11 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.11-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A011Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_11_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.11-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A011Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.12 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.12-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A012Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_12_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.12-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A012Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.13 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.13-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A013Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_13_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.13-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A013Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.14 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.14-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A014Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_14_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.14-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A014Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.15 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.15-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A015Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_15_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.15-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A015Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.16 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.16-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A016Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_16_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.16-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A016Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.17 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.17-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A017Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_17_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.17-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A017Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.18 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.18-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A018Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_18_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.18-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A018Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.19 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.19-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A019Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_19_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.19-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A019Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.20 Erlang successor
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.20-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A020Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_20_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.20-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A020Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    /// Verifies CLI argument parsing accepts the named A0.21 Erlang diagnostic
    /// target profile.
    ///
    /// Inputs:
    /// - Synthetic CLI arguments containing `--target-profile a0.21-erlang`.
    ///
    /// Output:
    /// - Test assertion only; no files are read or written.
    ///
    /// Transformation:
    /// - Parses the argument vector and asserts the command state carries
    ///   `TargetProfile::A021Erlang` while preserving the command and source
    ///   path.
    #[test]
    fn parse_args_accepts_a0_21_erlang_target_profile() {
        let (state, cmd) = parse_args(vec![
            "check".into(),
            "src/example.tl".into(),
            "--target-profile".into(),
            "a0.21-erlang".into(),
        ]);

        assert_eq!(state.target_profile, TargetProfile::A021Erlang);
        assert_eq!(cmd.verb.as_deref(), Some("check"));
        assert_eq!(cmd.args, vec!["src/example.tl".to_string()]);
    }

    struct PhaseContractFixture {
        module_name: &'static str,
        source_path: &'static str,
    }

    fn phase_contract_fixtures() -> Vec<PhaseContractFixture> {
        vec![
            PhaseContractFixture {
                module_name: "phase_basic",
                source_path: "phase_basic.tl",
            },
            PhaseContractFixture {
                module_name: "phase_binary_eq",
                source_path: "phase_binary_eq.tl",
            },
            PhaseContractFixture {
                module_name: "phase_binary_lt",
                source_path: "phase_binary_lt.tl",
            },
            PhaseContractFixture {
                module_name: "phase_binary_lte",
                source_path: "phase_binary_lte.tl",
            },
            PhaseContractFixture {
                module_name: "phase_binary_gt",
                source_path: "phase_binary_gt.tl",
            },
            PhaseContractFixture {
                module_name: "phase_binary_gte",
                source_path: "phase_binary_gte.tl",
            },
            PhaseContractFixture {
                module_name: "phase_binary_mul",
                source_path: "phase_binary_mul.tl",
            },
            PhaseContractFixture {
                module_name: "phase_binary_sub",
                source_path: "phase_binary_sub.tl",
            },
            PhaseContractFixture {
                module_name: "phase_core_lean",
                source_path: "phase_core_lean.tl",
            },
            PhaseContractFixture {
                module_name: "phase_int_literal",
                source_path: "phase_int_literal.tl",
            },
            PhaseContractFixture {
                module_name: "phase_atom_literal",
                source_path: "phase_atom_literal.tl",
            },
            PhaseContractFixture {
                module_name: "phase_binary_literal",
                source_path: "phase_binary_literal.tl",
            },
            PhaseContractFixture {
                module_name: "phase_tuple_literal",
                source_path: "phase_tuple_literal.tl",
            },
            PhaseContractFixture {
                module_name: "phase_list_literal",
                source_path: "phase_list_literal.tl",
            },
            PhaseContractFixture {
                module_name: "phase_named_call",
                source_path: "phase_named_call.tl",
            },
            PhaseContractFixture {
                module_name: "phase_core_lambda",
                source_path: "phase_core_lambda.tl",
            },
            PhaseContractFixture {
                module_name: "phase_unary_operator",
                source_path: "phase_unary_operator.tl",
            },
            PhaseContractFixture {
                module_name: "phase_list_cons",
                source_path: "phase_list_cons.tl",
            },
            PhaseContractFixture {
                module_name: "phase_if_expr",
                source_path: "phase_if_expr.tl",
            },
            PhaseContractFixture {
                module_name: "phase_field_access",
                source_path: "phase_field_access.tl",
            },
            PhaseContractFixture {
                module_name: "phase_literal_pattern_case",
                source_path: "phase_literal_pattern_case.tl",
            },
            PhaseContractFixture {
                module_name: "phase_no_expressions",
                source_path: "phase_no_expressions.tl",
            },
            PhaseContractFixture {
                module_name: "phase_summary_type_debt",
                source_path: "phase_summary_type_debt.tl",
            },
            PhaseContractFixture {
                module_name: "phase_template",
                source_path: "phase_template.tl",
            },
            PhaseContractFixture {
                module_name: "phase_constructor_resolution",
                source_path: "phase_constructor_resolution.tl",
            },
            PhaseContractFixture {
                module_name: "phase_constructor_pattern_resolution",
                source_path: "phase_constructor_pattern_resolution.tl",
            },
            PhaseContractFixture {
                module_name: "phase_constructor_chain_resolution",
                source_path: "phase_constructor_chain_resolution.tl",
            },
            PhaseContractFixture {
                module_name: "phase_trait",
                source_path: "phase_trait.tl",
            },
        ]
    }

    fn phase_contract_fixture_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/phase_contract")
    }

    fn read_phase_contract_expected(name: &str, stage: &str) -> String {
        let path = phase_contract_fixture_root().join(format!("{name}.{stage}.expected"));
        fs::read_to_string(&path).unwrap_or_else(|err| {
            panic!("failed to read phase contract expected output {path:?}: {err}");
        })
    }

    /// Lowers a phase-contract fixture into deterministic CoreIR contract text.
    ///
    /// Inputs:
    /// - `fixture`: phase-contract fixture descriptor with module name and
    ///   source path relative to the phase-contract fixture root.
    ///
    /// Output:
    /// - Deterministic `CoreModule::contract_text()` for the parsed, resolved,
    ///   and CoreIR-lowered fixture.
    ///
    /// Transformation:
    /// - Reads the fixture source, parses it into syntax output, resolves it
    ///   with local interfaces, lowers the resolved typed module into CoreIR,
    ///   and returns the CoreIR contract snapshot used by formal proof gates.
    fn phase_contract_core_contract_text(fixture: &PhaseContractFixture) -> String {
        let root = phase_contract_fixture_root();
        let source_path = root.join(fixture.source_path);
        let source = fs::read_to_string(&source_path)
            .unwrap_or_else(|err| panic!("failed to read phase fixture {source_path:?}: {err}"));
        let syntax_output =
            formal_pipeline::parse_source_as_syntax_output(&source_path.to_string_lossy(), &source)
                .unwrap_or_else(|err| {
                    panic!(
                        "failed to parse syntax output fixture {}: {err:?}",
                        fixture.source_path
                    )
                });
        let interfaces =
            formal_pipeline::load_external_interfaces(&source_path.to_string_lossy(), None);
        let resolved =
            resolve_syntax_module_output_with_interfaces(&syntax_output, &interfaces).module;
        terlan_typeck::lower_syntax_module_output_to_core(&syntax_output, &resolved).contract_text()
    }

    /// Runs `check --emit-phase-manifest` for a phase-contract fixture.
    ///
    /// Inputs:
    /// - `fixture`: phase-contract fixture descriptor with module name and
    ///   source path relative to the phase-contract fixture root.
    ///
    /// Output:
    /// - Parsed JSON phase manifest emitted by the CLI check command.
    ///
    /// Transformation:
    /// - Executes the same command-level check path used by external tooling,
    ///   writes the manifest to a temporary path, reads it back, and parses it
    ///   into JSON so tests can assert command-artifact proof coverage.
    fn phase_contract_check_manifest_json(fixture: &PhaseContractFixture) -> serde_json::Value {
        let root = phase_contract_fixture_root();
        let source_path = root.join(fixture.source_path);
        let dir = make_temp_dir(&format!("{}_phase_manifest", fixture.module_name));
        let manifest = dir.join(format!("{}.phase-manifest.json", fixture.module_name));
        let cache = dir.join("cache");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source_path.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        serde_json::from_str(&manifest_text).expect("parse phase manifest")
    }

    fn normalize_expected_text(text: &str) -> String {
        text.lines()
            .map(|line| line.trim_end())
            .filter(|line| !line.is_empty())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n"
    }

    fn syntax_public_function_surface_snapshot(module: &SyntaxModuleOutput) -> Vec<String> {
        let mut entries = module
            .declarations
            .iter()
            .filter_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Function {
                    name,
                    params,
                    is_public,
                    ..
                } if *is_public => Some(format!("{}/{}", name, params.len())),
                _ => None,
            })
            .collect::<Vec<_>>();
        entries.sort();
        entries
    }

    /// Builds the expected exported Erlang function surface for one syntax
    /// fixture.
    ///
    /// Inputs:
    /// - `module`: syntax-output module fixture.
    ///
    /// Output:
    /// - Sorted Erlang export names including public source functions with
    ///   hidden trait-evidence arguments and constructor helper exports.
    ///
    /// Transformation:
    /// - Derives public function arity from source parameters plus runtime
    ///   trait-evidence parameters, then appends deterministic constructor
    ///   helper names for public constructors.
    fn syntax_public_erlang_surface_snapshot(module: &SyntaxModuleOutput) -> Vec<String> {
        let mut entries = module
            .declarations
            .iter()
            .filter_map(|decl| match &decl.payload {
                SyntaxDeclarationPayload::Function {
                    name,
                    params,
                    generic_bounds,
                    is_public,
                    ..
                } if *is_public => {
                    Some(format!("{}/{}", name, params.len() + generic_bounds.len()))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        for decl in &module.declarations {
            match &decl.payload {
                SyntaxDeclarationPayload::Constructor {
                    name,
                    is_public,
                    clauses,
                    ..
                } if *is_public => {
                    for clause in clauses {
                        let fixed_arity = clause
                            .params
                            .iter()
                            .filter(|param| !param.is_varargs)
                            .count();
                        let varargs = clause.params.iter().any(|param| param.is_varargs);
                        let emitted_arity = if varargs {
                            fixed_arity + 1
                        } else {
                            fixed_arity
                        };
                        entries.push(format!(
                            "{}/{}",
                            phase_contract_constructor_function_name(name, fixed_arity, varargs),
                            emitted_arity
                        ));
                    }
                }
                _ => {}
            }
        }
        entries.sort();
        entries
    }

    /// Maps a public constructor declaration to the emitted helper name used by
    /// phase-contract backend surface checks.
    ///
    /// Inputs:
    /// - `name`: source constructor name.
    /// - `fixed_arity`: number of non-vararg constructor parameters.
    /// - `varargs`: whether the constructor accepts a vararg parameter.
    ///
    /// Output:
    /// - Erlang/JavaScript helper function name expected in backend exports.
    ///
    /// Transformation:
    /// - Mirrors the backend's deterministic constructor helper naming scheme
    ///   for phase-contract tests without depending on backend-private helpers.
    fn phase_contract_constructor_function_name(
        name: &str,
        fixed_arity: usize,
        varargs: bool,
    ) -> String {
        if varargs {
            format!(
                "typer_ctor_{}_varargs_{}",
                phase_contract_erlang_type_name(name),
                fixed_arity
            )
        } else {
            format!(
                "typer_ctor_{}_{}",
                phase_contract_erlang_type_name(name),
                fixed_arity
            )
        }
    }

    /// Converts a source constructor name into the backend helper stem used by
    /// phase-contract tests.
    ///
    /// Inputs:
    /// - `name`: source constructor name.
    ///
    /// Output:
    /// - Lowercase snake-style backend type-name stem.
    ///
    /// Transformation:
    /// - Inserts underscores before non-leading uppercase ASCII letters and
    ///   lowercases uppercase ASCII letters, matching backend helper naming.
    fn phase_contract_erlang_type_name(name: &str) -> String {
        let mut out = String::new();
        for (idx, ch) in name.chars().enumerate() {
            if ch.is_ascii_uppercase() {
                if idx > 0 {
                    out.push('_');
                }
                out.push(ch.to_ascii_lowercase());
            } else {
                out.push(ch);
            }
        }
        out
    }

    fn resolve_stage_snapshot(resolved: &terlan_hir::ResolvedModule) -> String {
        let mut out = Vec::new();
        out.push(format!("module={}", resolved.name));
        out.push(format!("diagnostics={}", resolved.diagnostics.len()));
        let mut function_keys = resolved
            .function_symbols
            .iter()
            .map(|(key, symbol)| {
                (
                    key.0.clone(),
                    key.1,
                    symbol.public,
                    symbol.exported,
                    symbol.return_type.clone(),
                    symbol
                        .params
                        .iter()
                        .map(|param| format!("{}:{}", param.name, param.annotation))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        function_keys.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        out.push(format!("function_symbols={}", function_keys.len()));
        for (name, arity, public, exported, return_type, params) in function_keys {
            out.push(format!(
                "fn={}/{} public={} exported={} return={}",
                name, arity, public, exported, return_type
            ));
            for param in params {
                out.push(format!("  param={}", param));
            }
        }

        let mut local_types = resolved
            .local_type_names
            .iter()
            .map(|(name, vis)| format!("{name}:{vis:?}"))
            .collect::<Vec<_>>();
        local_types.sort();
        out.push(format!("local_types={}", local_types.join(",")));

        let mut imported_types = resolved
            .imported_types
            .iter()
            .map(|(name, imported)| {
                format!(
                    "{}:{}:{}",
                    name, imported.source_module, imported.visibility as i32
                )
            })
            .collect::<Vec<_>>();
        imported_types.sort();
        out.push(format!("imported_types={}", imported_types.join(",")));

        let mut imported_traits = resolved
            .imported_traits
            .iter()
            .map(|(name, imported)| {
                format!(
                    "{}:{}:{}",
                    name, imported.source_module, imported.visibility as i32
                )
            })
            .collect::<Vec<_>>();
        imported_traits.sort();
        out.push(format!("imported_traits={}", imported_traits.join(",")));

        let mut interface_map = resolved.interface_map.keys().cloned().collect::<Vec<_>>();
        interface_map.sort();
        out.push(format!("interface_map={}", interface_map.join(",")));
        out.push(format!(
            "interface_functions={}",
            resolved.interface.functions.len()
        ));
        normalize_expected_text(&out.join("\n"))
    }

    fn typed_stage_snapshot(diagnostics: &[terlan_typeck::Diagnostic]) -> String {
        if diagnostics.is_empty() {
            return "diagnostics=ok\n".to_string();
        }
        let mut entries = diagnostics
            .iter()
            .map(|diagnostic| {
                let severity = match diagnostic.severity {
                    terlan_typeck::DiagSeverity::Error => "error",
                    terlan_typeck::DiagSeverity::Warning => "warning",
                };
                format!(
                    "{}:{}-{}:{}",
                    severity, diagnostic.span.start, diagnostic.span.end, diagnostic.message
                )
            })
            .collect::<Vec<_>>();
        entries.sort();
        normalize_expected_text(&entries.join("\n"))
    }

    fn core_stage_snapshot(core: &terlan_typeck::CoreModule) -> String {
        normalize_expected_text(&core.contract_text())
    }

    fn emit_stage_snapshot(path: &Path) -> String {
        let source = fs::read_to_string(path).unwrap_or_else(|err| {
            panic!("failed to read emitted file {path:?}: {err}");
        });
        let mut out = Vec::new();
        for line in source.lines() {
            let trimmed = line.trim_end();
            if trimmed.starts_with("-module(")
                || trimmed.starts_with("-export(")
                || (trimmed.ends_with(" ->") && !trimmed.starts_with(" "))
            {
                out.push(trimmed.to_string());
            }
        }
        if out.is_empty() {
            panic!("no emit snapshot lines found in {path:?}");
        }
        normalize_expected_text(&out.join("\n"))
    }

    fn parse_erlang_exported_function_surface(path: &Path) -> Vec<String> {
        let source = fs::read_to_string(path).unwrap_or_else(|err| {
            panic!("failed to read emitted erlang file {path:?}: {err}");
        });
        let mut exports = Vec::new();
        for line in source.lines() {
            let trimmed = line.trim();
            let Some(body) = trimmed.strip_prefix("-export([") else {
                continue;
            };
            let Some(body) = body.strip_suffix("]).") else {
                continue;
            };
            if body.trim().is_empty() {
                continue;
            }
            for entry in body.split(',') {
                let entry = entry.trim();
                if entry.is_empty() {
                    continue;
                }
                if let Some((name, arity)) = entry.rsplit_once('/') {
                    if !name.is_empty() && !arity.is_empty() {
                        exports.push(entry.to_string());
                    }
                }
            }
        }
        exports.sort();
        exports
    }

    fn parse_js_exported_function_surface(path: &Path) -> Vec<String> {
        let source = fs::read_to_string(path).unwrap_or_else(|err| {
            panic!("failed to read emitted js file {path:?}: {err}");
        });
        let mut exports = Vec::new();
        for line in source.lines() {
            let trimmed = line.trim();
            let Some(rest) = trimmed.strip_prefix("export function ") else {
                continue;
            };
            let Some(paren_start) = rest.find('(') else {
                continue;
            };
            let function_name = rest[..paren_start].trim();
            if function_name.is_empty() {
                continue;
            }
            let rest = &rest[paren_start + 1..];
            let Some(paren_end) = rest.find(')') else {
                continue;
            };
            let params = rest[..paren_end].trim();
            let arity = if params.is_empty() {
                0
            } else {
                params.split(',').count()
            };
            exports.push(format!("{function_name}/{arity}"));
        }
        exports.sort();
        exports
    }

    /// Extracts public function names from backend surface entries.
    ///
    /// Inputs:
    /// - `surface`: sorted backend export entries formatted as `name/arity`.
    ///
    /// Output:
    /// - Sorted function names with backend arity removed.
    ///
    /// Transformation:
    /// - Splits each surface entry at the final `/`, keeps the function-name
    ///   prefix, sorts the names, and removes duplicates so cross-backend
    ///   checks compare source-visible names rather than backend ABI arity.
    fn public_function_names_from_surface(surface: &[String]) -> Vec<String> {
        let mut names = surface
            .iter()
            .filter_map(|entry| {
                entry
                    .rsplit_once('/')
                    .map(|(name, _arity)| name.to_string())
            })
            .collect::<Vec<_>>();
        names.sort();
        names.dedup();
        names
    }

    fn assert_phase_contract_expected(fixture: PhaseContractFixture) {
        let root = phase_contract_fixture_root();
        let update_expected = std::env::var_os("TERLAN_UPDATE_PHASE_EXPECTED").is_some();
        let source_path = root.join(fixture.source_path);
        let source = fs::read_to_string(&source_path).unwrap_or_else(|err| {
            panic!("failed to read phase fixture source {source_path:?}: {err}");
        });
        let syntax_output =
            formal_pipeline::parse_source_as_syntax_output(&source_path.to_string_lossy(), &source)
                .unwrap_or_else(|err| {
                    panic!(
                        "failed to parse syntax output fixture {}: {err:?}",
                        fixture.source_path
                    )
                });

        let interfaces =
            formal_pipeline::load_external_interfaces(&source_path.to_string_lossy(), None);
        let resolved =
            resolve_syntax_module_output_with_interfaces(&syntax_output, &interfaces).module;
        let resolved_snapshot = resolve_stage_snapshot(&resolved);
        let expected_resolve = read_phase_contract_expected(fixture.module_name, "resolve");
        if update_expected {
            let expected_path = root.join(format!("{}.resolve.expected", fixture.module_name));
            fs::write(&expected_path, &resolved_snapshot).expect("write resolve phase expected");
        } else {
            assert_eq!(
                resolved_snapshot,
                normalize_expected_text(&expected_resolve)
            );
        }

        let diagnostics =
            type_check_syntax_module_output_with_templates(&syntax_output, &resolved, &source_path);
        let typed_snapshot = typed_stage_snapshot(&diagnostics);
        let expected_typed = read_phase_contract_expected(fixture.module_name, "typed");
        if update_expected {
            let expected_path = root.join(format!("{}.typed.expected", fixture.module_name));
            fs::write(&expected_path, &typed_snapshot).expect("write typed phase expected");
        } else {
            assert_eq!(typed_snapshot, normalize_expected_text(&expected_typed));
        }

        let core = terlan_typeck::lower_syntax_module_output_to_core(&syntax_output, &resolved);
        let core_snapshot = core_stage_snapshot(&core);
        let expected_core = read_phase_contract_expected(fixture.module_name, "core");
        if update_expected {
            let expected_path = root.join(format!("{}.core.expected", fixture.module_name));
            fs::write(&expected_path, &core_snapshot).expect("write core phase expected");
        } else {
            assert_eq!(core_snapshot, normalize_expected_text(&expected_core));
        }

        let out_dir = make_temp_dir("phase_contract_emit");
        let exit = commands::emit::run(
            CliCommand {
                verb: Some("emit".into()),
                args: vec![source_path.to_string_lossy().to_string()],
            },
            CliState {
                out_dir: out_dir.clone(),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);
        let emitted_path = out_dir.join(format!(
            "{}.erl",
            support::erlang_output_stem(&syntax_output.module_name)
        ));
        let emit_snapshot = emit_stage_snapshot(&emitted_path);
        let expected_emit = read_phase_contract_expected(fixture.module_name, "emit");
        if update_expected {
            let expected_path = root.join(format!("{}.emit.expected", fixture.module_name));
            fs::write(&expected_path, &emit_snapshot).expect("write emit phase expected");
        } else {
            assert_eq!(emit_snapshot, normalize_expected_text(&expected_emit));
        }
    }

    #[test]
    fn run_phase_contract_fixtures_match_expected_outputs() {
        for fixture in phase_contract_fixtures() {
            assert_phase_contract_expected(fixture);
        }
    }

    /// Verifies LP8 CoreIR-to-Lean conformance baselines stay Lean-covered.
    ///
    /// Inputs:
    /// - `phase_core_lean`: simple function fixture that exercises direct
    ///   Lean-covered variable CoreIR.
    /// - `phase_core_lambda`: anonymous-function fixture that exercises
    ///   runtime-binding freshness evidence for lambda lowering.
    /// - `phase_constructor_resolution`: resolved constructor-call fixture
    ///   that exercises Lean-covered constructor values.
    /// - `phase_constructor_pattern_resolution`: resolved constructor-pattern
    ///   fixture that exercises case-pattern runtime-binding freshness.
    ///
    /// Output:
    /// - Test assertion only; no source or expected-output files are modified.
    ///
    /// Transformation:
    /// - Lowers each fixture through the formal parse/resolve/typecheck/CoreIR
    ///   path and checks the resulting CoreIR contract text for the proof
    ///   readiness and freshness snippets required by the Lean handoff.
    #[test]
    fn run_phase_contract_lean_conformance_baselines_are_lean_covered() {
        for baseline in validation::proof_baseline::contract_baselines() {
            let fixture = phase_contract_fixtures()
                .into_iter()
                .find(|fixture| fixture.module_name == baseline.module_name)
                .unwrap_or_else(|| {
                    panic!("missing Lean conformance fixture {}", baseline.module_name)
                });
            let core_contract = phase_contract_core_contract_text(&fixture);

            validation::proof_baseline::validate_contract_baseline(baseline, &core_contract)
                .unwrap_or_else(|err| panic!("{err}:\n{core_contract}"));
        }
    }

    /// Verifies the next LP8 Lean-model candidate has stable typed CoreIR.
    ///
    /// Inputs:
    /// - `phase_basic`: arithmetic fixture that currently lowers to typed
    ///   `BinaryOp` CoreIR with Lean-covered variable children.
    ///
    /// Output:
    /// - Test assertion only; no source or expected-output files are modified.
    ///
    /// Transformation:
    /// - Lowers each candidate fixture through the formal
    ///   parse/resolve/typecheck/CoreIR path and checks that the resulting
    ///   contract remains typed, preservation-backed, and
    ///   `proof-model-required` until Lean models that CoreIR form.
    #[test]
    fn run_phase_contract_next_lean_model_candidates_are_pinned() {
        for baseline in validation::proof_baseline::next_lean_model_candidate_baselines() {
            let fixture = phase_contract_fixtures()
                .into_iter()
                .find(|fixture| fixture.module_name == baseline.module_name)
                .unwrap_or_else(|| panic!("missing Lean model candidate {}", baseline.module_name));
            let core_contract = phase_contract_core_contract_text(&fixture);

            validation::proof_baseline::validate_contract_baseline(baseline, &core_contract)
                .unwrap_or_else(|err| panic!("{err}:\n{core_contract}"));
        }
    }

    /// Verifies LP8 Lean conformance baselines are visible in phase manifests.
    ///
    /// Inputs:
    /// - `phase_core_lean`: simple function fixture that should emit one
    ///   Lean-covered expression and one Lean-covered pattern.
    /// - `phase_core_lambda`: anonymous-function fixture that should emit two
    ///   Lean-covered expressions with one runtime-binding freshness
    ///   obligation.
    /// - `phase_constructor_resolution`: resolved constructor-call fixture
    ///   that should emit one resolved constructor-call identity.
    /// - `phase_constructor_pattern_resolution`: resolved constructor-pattern
    ///   fixture that should emit one resolved constructor-pattern identity
    ///   and case runtime-binding freshness evidence.
    ///
    /// Output:
    /// - Test assertion only; no source or expected-output files are modified.
    ///
    /// Transformation:
    /// - Runs each fixture through command-level `check --emit-phase-manifest`
    ///   and verifies the manifest `core_proof_coverage` counters match the
    ///   CoreIR Lean-conformance baseline expected by external proof tooling.
    #[test]
    fn run_check_phase_contract_lean_conformance_baselines_emit_manifest_evidence() {
        for baseline in validation::proof_baseline::manifest_baselines() {
            let fixture = phase_contract_fixtures()
                .into_iter()
                .find(|fixture| fixture.module_name == baseline.module_name)
                .unwrap_or_else(|| {
                    panic!("missing Lean conformance fixture {}", baseline.module_name)
                });
            let manifest_json = phase_contract_check_manifest_json(&fixture);

            validation::proof_baseline::validate_manifest_baseline_artifact(
                baseline,
                manifest_json["core_ir_hash"].as_u64(),
                manifest_json["core_proof_coverage"]["readiness"].as_str(),
                |field| manifest_json["core_proof_coverage"][field].as_u64(),
            )
            .unwrap_or_else(|err| panic!("{err}"));
        }
    }

    /// Verifies next LP8 Lean-model candidates are visible in phase manifests.
    ///
    /// Inputs:
    /// - `phase_trait`: trait fixture that should emit one
    ///   proof-model-required remote/scoped-call expression and Lean-covered
    ///   variable argument children.
    ///
    /// Output:
    /// - Test assertion only; no source or expected-output files are modified.
    ///
    /// Transformation:
    /// - Runs each candidate fixture through command-level
    ///   `check --emit-phase-manifest` and verifies the manifest
    ///   `core_proof_coverage` counters match the candidate baseline while the
    ///   readiness remains `proof-model-required`.
    #[test]
    fn run_check_phase_contract_next_lean_model_candidates_emit_manifest_evidence() {
        for baseline in validation::proof_baseline::next_lean_model_candidate_manifest_baselines() {
            let fixture = phase_contract_fixtures()
                .into_iter()
                .find(|fixture| fixture.module_name == baseline.module_name)
                .unwrap_or_else(|| panic!("missing Lean model candidate {}", baseline.module_name));
            let manifest_json = phase_contract_check_manifest_json(&fixture);

            validation::proof_baseline::validate_manifest_baseline_artifact_with_readiness(
                baseline,
                "proof-model-required",
                manifest_json["core_ir_hash"].as_u64(),
                manifest_json["core_proof_coverage"]["readiness"].as_str(),
                |field| manifest_json["core_proof_coverage"][field].as_u64(),
            )
            .unwrap_or_else(|err| panic!("{err}"));
        }
    }

    #[test]
    fn run_phase_contract_fixtures_backend_parity() {
        for fixture in phase_contract_fixtures() {
            let root = phase_contract_fixture_root();
            let source_path = root.join(fixture.source_path);
            let source = fs::read_to_string(&source_path).unwrap_or_else(|err| {
                panic!("failed to read phase fixture {source_path:?}: {err}")
            });
            let syntax_output = formal_pipeline::parse_source_as_syntax_output(
                &source_path.to_string_lossy(),
                &source,
            )
            .unwrap_or_else(|err| {
                panic!(
                    "failed to parse syntax output fixture {}: {err:?}",
                    fixture.source_path
                )
            });
            let expected_js_surface = syntax_public_function_surface_snapshot(&syntax_output);
            let expected_erlang_surface = syntax_public_erlang_surface_snapshot(&syntax_output);
            let interfaces =
                formal_pipeline::load_external_interfaces(&source_path.to_string_lossy(), None);
            let resolved =
                resolve_syntax_module_output_with_interfaces(&syntax_output, &interfaces).module;
            let core = terlan_typeck::lower_syntax_module_output_to_core(&syntax_output, &resolved);
            let erlang_interfaces = interfaces.into_iter().collect::<BTreeMap<_, _>>();
            let direct_erlang =
                terlan_erlang::try_emit_syntax_module_output_to_erlang_with_interfaces_file_imports_templates_and_markdown(
                    &syntax_output,
                    &erlang_interfaces,
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                )
                .unwrap_or_else(|err| {
                    panic!("failed direct Erlang lowering for {source_path:?}: {err}")
                });
            let core_gated_erlang =
                terlan_erlang::try_emit_core_module_to_erlang_with_syntax_bridge(
                    &core,
                    &syntax_output,
                    &erlang_interfaces,
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                    &BTreeMap::new(),
                )
                .unwrap_or_else(|err| {
                    panic!("failed CoreIR-gated Erlang lowering for {source_path:?}: {err}")
                });
            assert_eq!(
                core_gated_erlang, direct_erlang,
                "CoreIR-gated Erlang output drift for {:?}",
                source_path
            );

            let erlang_dir = make_temp_dir("backend_parity_erlang");
            assert_eq!(
                commands::emit::run(
                    CliCommand {
                        verb: Some("emit".into()),
                        args: vec![source_path.to_string_lossy().to_string()],
                    },
                    CliState {
                        out_dir: erlang_dir.clone(),
                        ..Default::default()
                    },
                ),
                ExitCode::SUCCESS
            );
            let erlang_path = erlang_dir.join(format!(
                "{}.erl",
                support::erlang_output_stem(&syntax_output.module_name)
            ));
            let erlang_surface = parse_erlang_exported_function_surface(&erlang_path);
            assert_eq!(
                erlang_surface, expected_erlang_surface,
                "erlang surface mismatch for {:?}",
                source_path
            );

            let js_dir = make_temp_dir("backend_parity_js");
            assert_eq!(
                commands::emit_js::run(
                    &[
                        source_path.to_string_lossy().to_string(),
                        "--declarations".into(),
                    ],
                    &CliState {
                        out_dir: js_dir.clone(),
                        ..Default::default()
                    },
                ),
                ExitCode::SUCCESS
            );
            let js_path = js_dir.join(format!("{}.js", syntax_output.module_name));
            let js_source = fs::read_to_string(&js_path)
                .unwrap_or_else(|err| panic!("failed to read emitted js file {js_path:?}: {err}"));
            commands::emit_js::assert_oxc_accepts_js_artifact(&js_path, &js_source);
            let js_surface = parse_js_exported_function_surface(&js_path);
            assert_eq!(
                js_surface, expected_js_surface,
                "js surface mismatch for {:?}",
                source_path
            );
            let erlang_public_names = public_function_names_from_surface(&erlang_surface);
            for public_function in public_function_names_from_surface(&js_surface) {
                assert!(
                    erlang_public_names.contains(&public_function),
                    "Erlang surface missing public JS function name {public_function} for {:?}",
                    source_path
                );
            }

            let declarations_path = js_dir.join(format!("{}.d.ts", syntax_output.module_name));
            let declarations = fs::read_to_string(&declarations_path).unwrap_or_else(|err| {
                panic!("failed to read ts declarations {declarations_path:?}: {err}")
            });
            let expected_declarations_empty =
                core.types.iter().all(|type_decl| {
                    !matches!(type_decl.visibility, terlan_typeck::CoreVisibility::Public)
                }) && core.functions.iter().all(|function| !function.public);
            if expected_declarations_empty {
                assert!(
                    declarations.is_empty(),
                    "expected empty declarations for fixture with no public CoreIR declaration surface {:?}",
                    source_path
                );
            } else {
                assert!(
                    !declarations.is_empty(),
                    "expected declarations for fixture with public CoreIR declaration surface {:?}",
                    source_path
                );
            }
        }
    }

    /// Guards the standard emit command against direct syntax-output Erlang lowering.
    ///
    /// Inputs:
    /// - The local `commands/emit/mod.rs` source file.
    ///
    /// Output:
    /// - Test success when the command uses the CoreIR-gated backend entry point
    ///   and does not import/call the direct syntax-output Erlang emitter.
    ///
    /// Transformation:
    /// - Reads the command source as text and checks the transition invariant
    ///   required while direct syntax-output emitters still exist for parity
    ///   and compatibility paths.
    #[test]
    fn emit_command_uses_core_ir_gated_erlang_lowering() {
        let source = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands/emit/mod.rs"),
        )
        .expect("read emit command source");

        assert!(
            source.contains("try_emit_core_module_to_erlang_with_syntax_bridge"),
            "emit command must use the CoreIR-gated Erlang backend"
        );
        assert!(
            !source.contains(
                "try_emit_syntax_module_output_to_erlang_with_interfaces_file_imports_templates_and_markdown"
            ),
            "emit command must not call direct syntax-output Erlang lowering"
        );
    }

    /// Guards REPL expression execution against direct syntax-output Erlang lowering.
    ///
    /// Inputs:
    /// - The local `commands/repl/mod.rs` source file.
    ///
    /// Output:
    /// - Test success when REPL expression execution uses the CoreIR-gated
    ///   backend entry point and does not call the direct syntax-output Erlang
    ///   emitter.
    ///
    /// Transformation:
    /// - Reads the REPL command source as text and checks the CoreIR transition
    ///   invariant for the remaining interactive formal execution path.
    #[test]
    fn repl_expression_execution_uses_core_ir_gated_erlang_lowering() {
        let source = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands/repl/mod.rs"),
        )
        .expect("read repl command source");

        assert!(
            source.contains("try_emit_core_module_to_erlang_with_syntax_bridge"),
            "REPL expression execution must use the CoreIR-gated Erlang backend"
        );
        assert!(
            !source.contains(
                "try_emit_syntax_module_output_to_erlang_with_interfaces_file_imports_templates_and_markdown"
            ),
            "REPL expression execution must not call direct syntax-output Erlang lowering"
        );
    }

    /// Guards doctest validation against direct syntax-output Erlang lowering.
    ///
    /// Inputs:
    /// - The local `commands/doc/validation.rs` source file.
    ///
    /// Output:
    /// - Test success when doctest validation uses the CoreIR-gated backend entry
    ///   point and does not call the direct syntax-output Erlang emitter.
    ///
    /// Transformation:
    /// - Reads the doc validation source as text and checks the CoreIR
    ///   transition invariant for doctest compiler execution.
    #[test]
    fn doctest_validation_uses_core_ir_gated_erlang_lowering() {
        let source = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands/doc/validation.rs"),
        )
        .expect("read doc validation source");

        assert!(
            source.contains("try_emit_core_module_to_erlang_with_syntax_bridge"),
            "doctest validation must use the CoreIR-gated Erlang backend"
        );
        assert!(
            !source.contains(
                "try_emit_syntax_module_output_to_erlang_with_interfaces_file_imports_templates_and_markdown"
            ),
            "doctest validation must not call direct syntax-output Erlang lowering"
        );
    }

    #[test]
    fn formal_doc_markdown_generates_from_syntax_output() {
        let dir = make_temp_dir("formal_doc_markdown");
        let path = fixture(
            &dir,
            "//! Formal docs.\nmodule formal_docs.\n\n/// Adds one.\npub add(X: Int): Int ->\n    X + 1.\n",
        );
        let out_dir = dir.join("docs");

        let exit = commands::doc::run(
            CliCommand {
                verb: Some("doc".into()),
                args: vec![path],
            },
            CliState {
                out_dir: out_dir.clone(),
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
        let markdown = fs::read_to_string(out_dir.join("formal_docs.md")).expect("read docs");
        assert!(markdown.contains("# `formal_docs`"));
        assert!(markdown.contains("Formal docs."));
        assert!(markdown.contains("### `add/1`"));
        assert!(markdown.contains("pub add(X: Int): Int."));
    }

    #[test]
    fn formal_static_syntax_output_discovers_entrypoints_and_routes() {
        let module = parse_module_as_syntax_output(
            "\
module site.\n\
\n\
pub index(): Html[Never] ->\n\
    html { <main></main> }.\n\
\n\
static route \"/\" ->\n\
    home().\n\
\n\
home(): Html[Never] ->\n\
    html { <main><h1>Home</h1></main> }.\n\
",
        )
        .expect("parse syntax-output static module");

        assert_eq!(
            discover_syntax_static_entrypoints(&module),
            vec!["index".to_string()]
        );
        let routes = discover_syntax_static_routes(&module).expect("discover syntax routes");
        assert_eq!(
            routes,
            vec![StaticRoute {
                path: "/".to_string(),
                handler: "home".to_string(),
            }]
        );
        validate_syntax_static_route_handlers(&module, &routes)
            .expect("syntax route handlers should be valid");
    }

    #[test]
    fn formal_static_emit_renders_html_blocks_from_syntax_output() {
        let dir = make_temp_dir("formal_static_emit");
        let path = fixture(
            &dir,
            "module site.\n\npub page(): Html[Never] ->\n    html { <main class=\"home\"><h1>Hello</h1></main> }.\n",
        );
        let out_dir = dir.join("public");

        let exit = run_emit_static(
            CliCommand {
                verb: Some("emit-static".into()),
                args: vec![path],
            },
            CliState {
                out_dir: out_dir.clone(),
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
        assert_eq!(
            fs::read_to_string(out_dir.join("page.html")).expect("read static html"),
            "<main class=\"home\"><h1>Hello</h1></main>"
        );
    }

    #[test]
    fn formal_static_emit_renders_markdown_html_from_syntax_output() {
        let dir = make_temp_dir("formal_static_markdown");
        fs::create_dir_all(dir.join("posts")).expect("create posts");
        fs::write(
            dir.join("posts/welcome.md"),
            "# Welcome\n\nThis page came from **Markdown**.\n",
        )
        .expect("write markdown");
        let path = fixture(
            &dir,
            "module site.\n\nimport markdown \"./posts/welcome.md\" as WelcomePost.\n\npub post(): Html[Never] ->\n    WelcomePost.html.\n",
        );
        let out_dir = dir.join("public");

        let exit = run_emit_static(
            CliCommand {
                verb: Some("emit-static".into()),
                args: vec![path],
            },
            CliState {
                out_dir: out_dir.clone(),
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
        let html = fs::read_to_string(out_dir.join("post.html")).expect("read markdown html");
        assert!(html.contains("<h1>Welcome</h1>"));
        assert!(html.contains("<strong>Markdown</strong>"));
    }

    #[test]
    fn formal_static_emit_renders_external_template_from_syntax_output() {
        let dir = make_temp_dir("formal_static_template");
        fs::create_dir_all(dir.join("templates")).expect("create templates");
        fs::write(
            dir.join("templates/card.tl.html"),
            "<article data-id=\"{user.id}\"><h1>{title}</h1><p>{user.name}</p></article>",
        )
        .expect("write template");
        let path = fixture(
            &dir,
            "module site.\n\npub struct User {\n    id: Int,\n    name: Text\n}.\n\ntemplate Card from \"./templates/card.tl.html\" {\n    title: Text,\n    user: User\n}.\n\npub home(): Html[Never] ->\n    Card{ title = <<\"Hi & Bye\">>, user = #User{id = 7, name = <<\"Ada <A>\">>} }.\n",
        );
        let out_dir = dir.join("public");

        let exit = run_emit_static(
            CliCommand {
                verb: Some("emit-static".into()),
                args: vec![path],
            },
            CliState {
                out_dir: out_dir.clone(),
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
        assert_eq!(
            fs::read_to_string(out_dir.join("home.html")).expect("read template html"),
            "<article data-id=\"7\"><h1>Hi &amp; Bye</h1><p>Ada &lt;A&gt;</p></article>"
        );
    }

    #[test]
    fn formal_static_emit_renders_external_template_components_from_syntax_output() {
        let dir = make_temp_dir("formal_static_template_component");
        fs::create_dir_all(dir.join("templates")).expect("create templates");
        fs::write(
            dir.join("templates/page_shell.tl.html"),
            "<main class=\"{shell_class}\">{children}</main>",
        )
        .expect("write shell template");
        fs::write(
            dir.join("templates/page.tl.html"),
            "<page-shell shell_class=\"shell\"><h1>{title}</h1><p>Wrapped</p></page-shell>",
        )
        .expect("write page template");
        let source = "module site.\n\ntemplate PageShell from \"./templates/page_shell.tl.html\" {\n    shell_class: Text\n}.\n\ntemplate Page from \"./templates/page.tl.html\" {\n    title: Text\n}.\n\npub home(): Html[Never] ->\n    Page{ title = \"Home\" }.\n";
        let path = fixture(&dir, source);
        let module = parse_module_as_syntax_output(source).expect("parse syntax-output module");
        let templates =
            commands::artifacts::collect_syntax_template_inputs(&module, Path::new(&path))
                .expect("collect templates");

        let html = commands::static_site::render_syntax_static_entrypoint(
            &module,
            &templates,
            &BTreeMap::new(),
            "home",
        )
        .expect("render syntax static template component");

        assert_eq!(
            html,
            "<main class=\"shell\"><h1>Home</h1><p>Wrapped</p></main>"
        );
    }

    #[test]
    fn formal_static_emit_renders_inline_template_components_from_syntax_output() {
        let dir = make_temp_dir("formal_static_inline_template_component");
        fs::create_dir_all(dir.join("templates")).expect("create templates");
        fs::write(
            dir.join("templates/page_shell.tl.html"),
            "<main class=\"{shell_class}\">{view1}<span>and</span>{view2}{children}</main>",
        )
        .expect("write shell template");
        fs::write(
            dir.join("templates/welcome_content.tl.html"),
            "<p>Welcome</p>",
        )
        .expect("write welcome template");
        let source = "module site.\n\ntemplate PageShell from \"./templates/page_shell.tl.html\" {\n    shell_class: Text,\n    view1: Html[Never],\n    view2: Html[Never]\n}.\n\ntemplate WelcomeContent from \"./templates/welcome_content.tl.html\" {}.\n\npub home(): Html[Never] ->\n    html {\n        <page-shell shell_class=\"shell\">\n            @view1 {\n                <welcome-content></welcome-content>\n            }\n            @view2 {\n                <p>Second</p>\n            }\n            <p>After</p>\n        </page-shell>\n    }.\n";
        let path = fixture(&dir, source);
        let module = parse_module_as_syntax_output(source).expect("parse syntax-output module");
        let templates =
            commands::artifacts::collect_syntax_template_inputs(&module, Path::new(&path))
                .expect("collect templates");

        let html = commands::static_site::render_syntax_static_entrypoint(
            &module,
            &templates,
            &BTreeMap::new(),
            "home",
        )
        .expect("render syntax inline static template component");

        assert_eq!(
            html,
            "<main class=\"shell\"><p>Welcome</p><span>and</span><p>Second</p><p>After</p></main>"
        );
    }

    #[test]
    fn run_emit_js_writes_js_and_declarations() {
        let dir = make_temp_dir("emit_js_success");
        let path = fixture(
            &dir,
            "module js_demo.\n\npub type Option[T] =\n      none\n    | {some, T}.\n\npub type Result[T, E] =\n      {ok, T}\n    | {error, E}.\n\ntype PrivateAlias = Int.\n\npub validate_age(Age: Int): Result[Int, invalid_age] ->\n    case Age >= 0 {\n        true -> {ok, Age};\n        false -> {error, invalid_age}\n    }.\n\nprivate_flag(Name: Text): Bool ->\n    Name >= <<\"a\">>.\n",
        );
        let out_dir = dir.join("js");
        let parsed =
            commands::emit_js::parse_emit_js_args(&[path.clone(), "--declarations".into()])
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
        let declarations =
            fs::read_to_string(out_dir.join("js_demo.d.ts")).expect("read declarations");
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

    #[test]
    fn run_emit_js_reports_errors() {
        let missing_arg = commands::emit_js::run(&[], &CliState::default());
        assert_eq!(missing_arg, ExitCode::from(2));

        assert!(commands::emit_js::parse_emit_js_args(&["m.tl".into(), "--bad".into()]).is_err());

        let read_error = commands::emit_js::run(
            &["/tmp/terlan_missing_emit_js.tl".into()],
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
        fs::create_dir_all(write_dts_dir.join("js_error.d.ts"))
            .expect("create conflicting dts dir");
        let write_dts_error = commands::emit_js::run(
            &[source_path, "--declarations".into()],
            &CliState {
                out_dir: write_dts_dir,
                ..Default::default()
            },
        );
        assert_eq!(write_dts_error, ExitCode::from(1));
    }

    #[test]
    fn run_interface_success_and_error_paths() {
        let dir = make_temp_dir("interface_paths");
        let success_dir = dir.join("success");
        fs::create_dir_all(&success_dir).expect("create success dir");
        let path = fixture(
            &success_dir,
            "//! Cache contract interface.\nmodule cache_contract.\n\n/// User ID alias.\npub type UserId = Int.\n\n/// User ID box alias.\npub type UserBox[T] = {box, T}.\n\n/// Cache handle.\npub opaque type Cache.\n\n/// Reads a value from the cache.\npub get(Cache: Cache, Key: Binary): Result[Binary, not_found].\n\n/// Trait for logging values.\npub trait Logger[A] {\n    log(V: A): Dynamic.\n}.\n",
        );
        let out_dir = dir.join("out");
        let exit = commands::interface::run(
            &[path.clone()],
            &CliState {
                out_dir: out_dir.clone(),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);
        let emitted = fs::read_to_string(out_dir.join("cache_contract.typi")).expect("read typi");
        assert!(emitted.contains("//! Cache contract interface."));
        assert!(emitted.contains("/// User ID alias."));
        assert!(emitted.contains("pub type UserId =\n    Int."));
        assert!(emitted.contains("/// User ID box alias."));
        assert!(emitted.contains("pub type UserBox[T] =\n    {box, T}."));
        assert!(emitted.contains("/// Cache handle."));
        assert!(emitted.contains("/// Reads a value from the cache."));
        assert!(emitted.contains("pub opaque type Cache."));
        assert!(emitted.contains("pub get(Cache: Cache, Key: Binary): Result[Binary, not_found]."));
        assert!(emitted.contains("/// Trait for logging values."));
        assert!(emitted.contains("pub trait Logger[A]"));
        assert!(emitted.contains("log(V: A): Dynamic."));

        let exit = commands::interface::run(&[], &CliState::default());
        assert_eq!(exit, ExitCode::from(2));

        let bad_dir = dir.join("bad_parse");
        fs::create_dir_all(&bad_dir).expect("create bad dir");
        let bad_parse = fixture(&bad_dir, "module broken\n");
        let exit = commands::interface::run(&[bad_parse], &CliState::default());
        assert_eq!(exit, ExitCode::from(1));

        let blocked_dir = dir.join("blocked_interface_out");
        fs::write(&blocked_dir, "not-a-dir").expect("write blocked out");
        let exit = commands::interface::run(
            &[path.clone()],
            &CliState {
                out_dir: blocked_dir,
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let out_dir = dir.join("write_fail");
        fs::create_dir_all(&out_dir).expect("create out");
        fs::create_dir_all(out_dir.join("cache_contract.typi")).expect("create conflicting target");
        let exit = commands::interface::run(
            &[path],
            &CliState {
                out_dir,
                incremental: true,
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));
    }

    #[test]
    fn run_check_dir_incremental_dependency_closure() {
        let dir = make_temp_dir("check_dir_incremental_dependency_closure");
        let cache = dir.join("cache");
        fs::write(
            dir.join("incr_lib.tl"),
            "module incr_lib.\n\npub add(X: Int): Int ->\n    X + 1.\n",
        )
        .expect("write lib");
        fs::write(
            dir.join("incr_user.tl"),
            "module incr_user.\n\nimport incr_lib.{add}.\n\npub compute(X: Int): Int ->\n    add(X).\n",
        )
        .expect("write user");
        fs::write(
            dir.join("incr_other.tl"),
            "module incr_other.\n\npub value(): Int ->\n    1.\n",
        )
        .expect("write unrelated");

        let state = CliState {
            incremental: true,
            cache_dir: Some(cache.clone()),
            ..Default::default()
        };
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![dir.to_string_lossy().into()],
            },
            state.clone(),
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let lib_manifest = cache.join("incr_lib.typi.deps");
        let user_manifest = cache.join("incr_user.typi.deps");
        let other_manifest = cache.join("incr_other.typi.deps");

        assert!(lib_manifest.exists());
        assert!(user_manifest.exists());
        assert!(other_manifest.exists());

        let baseline_user_manifest =
            fs::read_to_string(&user_manifest).expect("read baseline user manifest");
        let baseline_other_manifest =
            fs::read_to_string(&other_manifest).expect("read baseline other manifest");
        let baseline_lib_manifest =
            fs::read_to_string(&lib_manifest).expect("read baseline lib manifest");

        fs::write(
            dir.join("incr_lib.tl"),
            "module incr_lib.\n\npub add(X: Int): Int ->\n    X + 2.\n",
        )
        .expect("edit private-irrelevant lib body");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![dir.to_string_lossy().into()],
            },
            state.clone(),
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        assert_eq!(
            fs::read_to_string(&user_manifest).expect("read user manifest"),
            baseline_user_manifest,
            "user should not be rechecked when dependency interface is unchanged"
        );
        assert_eq!(
            fs::read_to_string(&other_manifest).expect("read other manifest"),
            baseline_other_manifest,
            "unrelated module should not be rechecked"
        );
        assert_ne!(
            fs::read_to_string(&lib_manifest).expect("read lib manifest"),
            baseline_lib_manifest,
            "changed dependency source should refresh its own manifest"
        );

        fs::write(
            dir.join("incr_lib.tl"),
            "module incr_lib.\n\npub add(X: Int): Int ->\n    X + 2.\n\npub neg(X: Int): Int ->\n    0 - X.\n",
        )
        .expect("edit public interface");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![dir.to_string_lossy().into()],
            },
            state,
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        assert_ne!(
            fs::read_to_string(&user_manifest).expect("read user manifest after interface change"),
            baseline_user_manifest,
            "user should be rechecked when dependency interface changes"
        );
        assert_ne!(
            fs::read_to_string(&lib_manifest).expect("read lib manifest after interface change"),
            baseline_lib_manifest,
            "changed dependency interface should refresh its own manifest"
        );
        assert_eq!(
            fs::read_to_string(&other_manifest)
                .expect("read other manifest after interface change"),
            baseline_other_manifest,
            "unrelated module should stay out of dependency closure"
        );
    }

    #[test]
    fn run_check_dir_incremental_with_trait_interfaces() {
        let dir = make_temp_dir("check_dir_incremental_trait_interfaces");
        fs::write(
            dir.join("trait_cache_lib.tl"),
            "module trait_cache_lib.\n\npub trait Label[A] {\n    show(value: A): Text.\n}.\n\nhelper(value: Int): Int ->\n    value + 1.\n\npub debug(value: Int): Int ->\n    helper(value).\n",
        )
        .expect("write trait_cache_lib");
        fs::write(
            dir.join("trait_cache_client.tl"),
            "module trait_cache_client.\n\nimport trait_cache_lib.{Label}.\n\npub render(value: Int): Int ->\n    value.\n",
        )
        .expect("write trait_cache_client");

        let cache = dir.join("cache");
        let state = CliState {
            incremental: true,
            trace_invalidation: true,
            cache_dir: Some(cache.clone()),
            ..Default::default()
        };
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![dir.to_string_lossy().into()],
            },
            state.clone(),
        );
        assert_eq!(exit, ExitCode::SUCCESS);
        assert!(cache.join("trait_cache_lib.typi").exists());
        assert!(cache.join("trait_cache_lib.typi.deps").exists());
        assert!(cache.join("trait_cache_client.typi").exists());

        fs::write(
            dir.join("trait_cache_lib.tl"),
            "module trait_cache_lib.\n\npub trait Label[A] {\n    show(value: A): Text.\n}.\n\nhelper(value: Int): Int ->\n    value + 2.\n\npub debug(value: Int): Int ->\n    helper(value).\n",
        )
        .expect("edit helper");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![dir.to_string_lossy().into()],
            },
            state.clone(),
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        fs::write(
            dir.join("trait_cache_lib.tl"),
            "module trait_cache_lib.\n\npub trait Label[A] {\n    show(value: A, flag: Int): Text.\n}.\n\nhelper(value: Int): Int ->\n    value + 2.\n\npub debug(value: Int): Int ->\n    helper(value).\n",
        )
        .expect("edit public trait and interface");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![dir.to_string_lossy().into()],
            },
            state,
        );
        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies directory module-layout mismatches stop before cache emission
    /// and are recorded in phase manifests.
    ///
    /// Inputs:
    /// - A directory-mode source file whose path implies `app.User` while its
    ///   declaration says `app.Profile`.
    ///
    /// Output:
    /// - Test passes when `terlc check <dir> --emit-phase-manifest <dir>` fails,
    ///   emits no interface cache artifacts, and records `module_layout_error`
    ///   in the resolve phase.
    ///
    /// Transformation:
    /// - Runs directory checking through the public CLI command, then inspects
    ///   cache and manifest artifacts to prove layout validation happens before
    ///   interface cache emission and before typecheck/CoreIR phases.
    #[test]
    fn run_check_dir_rejects_module_layout_mismatch() {
        let dir = make_temp_dir("check_dir_module_layout_mismatch");
        let app_dir = dir.join("app");
        fs::create_dir_all(&app_dir).expect("create app dir");
        fs::write(
            app_dir.join("User.tl"),
            "module app.Profile.\n\npub value(): Int ->\n    1.\n",
        )
        .expect("write mismatched module source");

        let cache = dir.join("cache");
        let manifest_dir = dir.join("manifests");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    dir.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest_dir.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache.clone()),
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::from(1));
        assert!(
            !cache.join("app.Profile.typi").exists(),
            "layout mismatch should stop before interface cache emission"
        );
        assert!(
            !cache.join("app.Profile.typi.deps").exists(),
            "layout mismatch should not emit interface dependency cache"
        );

        let manifest_text =
            fs::read_to_string(manifest_dir.join("app.Profile.phase-manifest.json"))
                .expect("read layout mismatch phase manifest");
        let manifest: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse layout mismatch phase manifest");
        assert_eq!(manifest["module"], "app.Profile");
        let phases = manifest["phases"].as_array().expect("phase entries");
        let resolve_phase = phases
            .iter()
            .find(|phase| phase["name"] == "resolve")
            .expect("resolve phase");
        assert_eq!(resolve_phase["status"], "error");
        assert_eq!(
            resolve_phase["diagnostics"][0]["code"],
            "module_layout_error"
        );
        assert!(resolve_phase["diagnostics"][0]["message"]
            .as_str()
            .expect("diagnostic message")
            .contains("does not match source path"));
    }

    #[test]
    fn run_check_dir_rejects_raw_macro_in_syntax_phase() {
        let dir = make_temp_dir("check_dir_raw_macro_rejected");
        fs::write(
            dir.join("macro_user.tl"),
            "module macro_user.\n\npub value(): Int ->\n    sql{select * from users}.\n",
        )
        .expect("write raw macro source");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![dir.to_string_lossy().into()],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));
    }

    #[test]
    fn run_check_dir_rejects_unsupported_raw_declaration_kind() {
        let dir = make_temp_dir("check_dir_unsupported_raw_declaration");
        fs::write(
            dir.join("unsupported_target.tl"),
            "module unsupported_target.\nprotocol removed_form { raw }.\n",
        )
        .expect("write unsupported raw declaration source");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![dir.to_string_lossy().into()],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));
    }

    #[test]
    fn run_check_single_file_reports_derive_expansion_phase_error() {
        let dir = make_temp_dir("check_single_file_unknown_derive");
        let source = dir.join("derive_fail.tl");
        fs::write(
            &source,
            "module derive_fail.\n\npub struct User derives MissingShow {\n    value: Int\n}.\n",
        )
        .expect("write unknown derive source");
        let manifest = dir.join("derive_fail.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"macro_expansion","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"derive_expansion","status":"error""#));
        assert!(manifest_text.contains(r#""name":"resolve","status":"skipped""#));
        assert!(manifest_text.contains(r#""name":"typecheck","status":"skipped""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
    }

    /// Verifies a successful single-file check emits a Core phase and debug trace.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers into Lean-covered
    ///   CoreIR.
    ///
    /// Output:
    /// - Test assertion only; no repository fixtures are modified.
    ///
    /// Transformation:
    /// - Runs `terlc check --emit-phase-manifest`, parses the manifest JSON,
    ///   and checks both CoreIR proof counters and source-to-CoreIR debug
    ///   identity.
    #[test]
    fn run_check_single_file_success_emits_core_phase_ok() {
        let dir = make_temp_dir("check_single_file_core_phase_ok");
        let source = dir.join("core_ok.tl");
        fs::write(&source, "module core_ok.\n\npub value(): Int ->\n    1.\n")
            .expect("write core ok source");
        let manifest = dir.join("core_ok.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"macro_expansion","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"derive_expansion","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        let source_path_text = source.to_string_lossy();
        assert_eq!(
            manifest_json["debug_trace"]["module"]
                .as_str()
                .expect("debug trace module"),
            "core_ok"
        );
        assert_eq!(
            manifest_json["debug_trace"]["source_path"]
                .as_str()
                .expect("debug trace source path"),
            source_path_text
        );
        assert_eq!(
            manifest_json["debug_trace"]["core_ir_available"]
                .as_bool()
                .expect("debug trace CoreIR availability"),
            true
        );
        assert_eq!(
            manifest_json["debug_trace"]["generated_artifact_kind"]
                .as_str()
                .expect("debug trace generated artifact kind"),
            "none"
        );
        assert!(
            manifest_json["debug_trace"]["generated_artifact_name"].is_null(),
            "check manifests should not claim a generated backend artifact"
        );
        assert_ne!(
            manifest_json["core_ir_hash"]
                .as_u64()
                .expect("core ir hash"),
            0
        );
        assert_eq!(
            manifest_json["debug_trace"]["core_ir_hash"]
                .as_u64()
                .expect("debug trace CoreIR hash"),
            manifest_json["core_ir_hash"]
                .as_u64()
                .expect("top-level CoreIR hash")
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["readiness"]
                .as_str()
                .expect("core proof readiness"),
            "lean-covered"
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["lean_covered"]
                .as_u64()
                .expect("lean-covered proof count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["proof_model_required"]
                .as_u64()
                .expect("proof-model-required proof count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["pattern_lean_covered"]
                .as_u64()
                .expect("lean-covered pattern proof count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["typed_core_expr"]
                .as_u64()
                .expect("typed CoreExpr count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["summary_only_expr"]
                .as_u64()
                .expect("summary-only expression count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["checked_preservation_expr"]
                .as_u64()
                .expect("checked-preservation expression count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["checked_preservation_expr_structural"]
                .as_u64()
                .expect("structural checked-preservation expression count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["checked_preservation_expr_no_runtime_bindings"]
                .as_u64()
                .expect("no-runtime-bindings expression count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]
                ["checked_preservation_expr_runtime_bindings_required"]
                .as_u64()
                .expect("runtime-bindings-required expression count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["typed_core_pattern"]
                .as_u64()
                .expect("typed CorePattern count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["summary_only_pattern"]
                .as_u64()
                .expect("summary-only pattern count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["checked_preservation_pattern"]
                .as_u64()
                .expect("checked-preservation pattern count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["checked_preservation_pattern_structural"]
                .as_u64()
                .expect("structural checked-preservation pattern count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]
                ["checked_preservation_pattern_no_runtime_bindings"]
                .as_u64()
                .expect("no-runtime-bindings pattern count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]
                ["checked_preservation_pattern_runtime_bindings_required"]
                .as_u64()
                .expect("runtime-bindings-required pattern count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
                .as_u64()
                .expect("resolved constructor-call identity count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_chain_identity"]
                .as_u64()
                .expect("resolved constructor-chain identity count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_pattern_identity"]
                .as_u64()
                .expect("resolved constructor-pattern identity count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_call_candidate"]
                .as_u64()
                .expect("unresolved constructor-call candidate count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_chain_candidate"]
                .as_u64()
                .expect("unresolved constructor-chain candidate count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_pattern_candidate"]
                .as_u64()
                .expect("unresolved constructor-pattern candidate count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["typed_core_type"]
                .as_u64()
                .expect("typed CoreType count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["summary_only_type"]
                .as_u64()
                .expect("summary-only type count"),
            0
        );
    }

    /// Verifies the `check` command accepts Lean-covered programs under the
    /// portable CoreIR v0 target profile.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a Lean-covered
    ///   arithmetic CoreIR expression.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts the accepted portable subset still exits successfully.
    #[test]
    fn run_check_single_file_accepts_subtraction_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_accepts_subtraction");
        let path = fixture(
            &dir,
            "\
module core_v0_accepts_subtraction.\n\npub value(left: Int, right: Int): Int ->\n    left - right.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts the frozen A0 fixture shape under
    /// the A0 Erlang target profile.
    ///
    /// Inputs:
    /// - Temporary source file matching the frozen A0 arithmetic fixture.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A0Erlang` and
    ///   asserts the documented A0 baseline exits successfully.
    #[test]
    fn run_check_single_file_accepts_mathx_for_a0_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_erlang_accepts_mathx");
        let path = fixture(
            &dir,
            "\
module a0_erlang_accepts_mathx.\n\npub add(x: Int): Int ->\n    x + 1.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A0Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects a source feature outside the frozen
    /// A0 artifact matrix.
    ///
    /// Inputs:
    /// - Temporary source file with a binary/string literal body.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A0Erlang` and
    ///   asserts excluded syntax fails before a successful result is returned.
    #[test]
    fn run_check_single_file_rejects_binary_for_a0_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_erlang_rejects_binary");
        let path = fixture(
            &dir,
            "\
module a0_erlang_rejects_binary.\n\npub value(): Binary ->\n    \"hello\".\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A0Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts the named A0.1 successor arithmetic
    /// and comparison subset.
    ///
    /// Inputs:
    /// - Temporary source file with `Int` parameters, arithmetic operators, and
    ///   a comparison return.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A01Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_arithmetic_for_a0_1_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_1_erlang_accepts_arithmetic");
        let path = fixture(
            &dir,
            "\
module a0_1_erlang_accepts_arithmetic.\n\npub bigger(x: Int, y: Int): Bool ->\n    x * 2 - 1 > y.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A01Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts the named A0.2 successor boolean
    /// expression subset.
    ///
    /// Inputs:
    /// - Temporary source file with `Bool` return annotation, boolean literal,
    ///   boolean operators, and comparison expressions.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A02Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_bool_ops_for_a0_2_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_2_erlang_accepts_bool_ops");
        let path = fixture(
            &dir,
            "\
module a0_2_erlang_accepts_bool_ops.\n\npub both(x: Int, y: Int): Bool ->\n    true and x > 0 or y > 0.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A02Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the A0.1 profile does not silently widen when A0.2 boolean
    /// expressions are introduced.
    ///
    /// Inputs:
    /// - Temporary source file using `and`, which belongs to the named A0.2
    ///   successor matrix rather than the A0.1 matrix.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A01Erlang`
    ///   and asserts the earlier successor profile still rejects the new
    ///   feature.
    #[test]
    fn run_check_single_file_keeps_bool_ops_out_of_a0_1_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_1_erlang_rejects_bool_ops");
        let path = fixture(
            &dir,
            "\
module a0_1_erlang_rejects_bool_ops.\n\npub both(x: Int, y: Int): Bool ->\n    x > 0 and y > 0.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A01Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts the named A0.3 successor
    /// conditional expression subset.
    ///
    /// Inputs:
    /// - Temporary source file with an `if` expression whose conditions and
    ///   branch bodies stay inside the A0.2 expression subset.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A03Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_if_expr_for_a0_3_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_3_erlang_accepts_if_expr");
        let path = fixture(
            &dir,
            "\
module a0_3_erlang_accepts_if_expr.\n\npub choose(x: Int): Int ->\n    if { x > 0 -> x; true -> 0 }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A03Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the A0.2 profile does not silently widen when A0.3 conditional
    /// expressions are introduced.
    ///
    /// Inputs:
    /// - Temporary source file using `if`, which belongs to the named A0.3
    ///   successor matrix rather than the A0.2 matrix.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A02Erlang`
    ///   and asserts the earlier successor profile still rejects the new
    ///   feature.
    #[test]
    fn run_check_single_file_keeps_if_expr_out_of_a0_2_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_2_erlang_rejects_if_expr");
        let path = fixture(
            &dir,
            "\
module a0_2_erlang_rejects_if_expr.\n\npub choose(x: Int): Int ->\n    if { x > 0 -> x; true -> 0 }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A02Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts the named A0.4 successor case
    /// expression subset.
    ///
    /// Inputs:
    /// - Temporary source file with a `case` expression whose scrutinee,
    ///   patterns, and branch bodies stay inside the A0.4 subset.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A04Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_case_expr_for_a0_4_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_4_erlang_accepts_case_expr");
        let path = fixture(
            &dir,
            "\
module a0_4_erlang_accepts_case_expr.\n\npub choose(x: Int): Int ->\n    case x { 0 -> 0; n -> n }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A04Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the A0.3 profile does not silently widen when A0.4 case
    /// expressions are introduced.
    ///
    /// Inputs:
    /// - Temporary source file using `case`, which belongs to the named A0.4
    ///   successor matrix rather than the A0.3 matrix.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A03Erlang`
    ///   and asserts the earlier successor profile still rejects the new
    ///   feature.
    #[test]
    fn run_check_single_file_keeps_case_expr_out_of_a0_3_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_3_erlang_rejects_case_expr");
        let path = fixture(
            &dir,
            "\
module a0_3_erlang_rejects_case_expr.\n\npub choose(x: Int): Int ->\n    case x { 0 -> 0; n -> n }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A03Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts the named A0.5 successor raw atom
    /// literal subset.
    ///
    /// Inputs:
    /// - Temporary source file with a raw atom expression body and raw atom
    ///   literal case pattern.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A05Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_raw_atoms_for_a0_5_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_5_erlang_accepts_raw_atoms");
        let path = fixture(
            &dir,
            "\
module a0_5_erlang_accepts_raw_atoms.\n\npub none(): Dynamic ->\n    :none.\n\npub is_none(x: Dynamic): Bool ->\n    case x { :none -> true; _ -> false }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A05Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the A0.4 profile does not silently widen when A0.5 raw atom
    /// literals are introduced.
    ///
    /// Inputs:
    /// - Temporary source file using raw atom literals, which belong to the
    ///   named A0.5 successor matrix rather than the A0.4 matrix.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A04Erlang`
    ///   and asserts the earlier successor profile still rejects the new
    ///   feature.
    #[test]
    fn run_check_single_file_keeps_raw_atoms_out_of_a0_4_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_4_erlang_rejects_raw_atoms");
        let path = fixture(
            &dir,
            "\
module a0_4_erlang_rejects_raw_atoms.\n\npub none(): Dynamic ->\n    :none.\n\npub is_none(x: Dynamic): Bool ->\n    case x { :none -> true; _ -> false }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A04Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts the named A0.6 successor tuple
    /// expression and pattern subset.
    ///
    /// Inputs:
    /// - Temporary source file with tuple construction and tuple case pattern
    ///   matching.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A06Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_tuples_for_a0_6_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_6_erlang_accepts_tuples");
        let path = fixture(
            &dir,
            "\
module a0_6_erlang_accepts_tuples.\n\npub pair(x: Int): Dynamic ->\n    {x, :none}.\n\npub first(value: Dynamic): Int ->\n    case value { {n, :none} -> n; _ -> 0 }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A06Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the A0.5 profile does not silently widen when A0.6 tuple forms
    /// are introduced.
    ///
    /// Inputs:
    /// - Temporary source file using tuple construction and tuple patterns,
    ///   which belong to the named A0.6 successor matrix rather than A0.5.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A05Erlang`
    ///   and asserts the earlier successor profile still rejects the new
    ///   feature.
    #[test]
    fn run_check_single_file_keeps_tuples_out_of_a0_5_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_5_erlang_rejects_tuples");
        let path = fixture(
            &dir,
            "\
module a0_5_erlang_rejects_tuples.\n\npub pair(x: Int): Dynamic ->\n    {x, :none}.\n\npub first(value: Dynamic): Int ->\n    case value { {n, :none} -> n; _ -> 0 }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A05Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts the named A0.7 successor list
    /// expression and fixed-list pattern subset.
    ///
    /// Inputs:
    /// - Temporary source file with list construction and fixed-list case
    ///   pattern matching.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A07Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_lists_for_a0_7_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_7_erlang_accepts_lists");
        let path = fixture(
            &dir,
            "\
module a0_7_erlang_accepts_lists.\n\npub values(): Dynamic ->\n    [1, 2, 3].\n\npub first(value: Dynamic): Int ->\n    case value { [n, _] -> n; _ -> 0 }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A07Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the A0.6 profile does not silently widen when A0.7 list forms
    /// are introduced.
    ///
    /// Inputs:
    /// - Temporary source file using list construction and fixed-list patterns,
    ///   which belong to the named A0.7 successor matrix rather than A0.6.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A06Erlang`
    ///   and asserts the earlier successor profile still rejects the new
    ///   feature.
    #[test]
    fn run_check_single_file_keeps_lists_out_of_a0_6_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_6_erlang_rejects_lists");
        let path = fixture(
            &dir,
            "\
module a0_6_erlang_rejects_lists.\n\npub values(): Dynamic ->\n    [1, 2, 3].\n\npub first(value: Dynamic): Int ->\n    case value { [n, _] -> n; _ -> 0 }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A06Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts binary/string literal expressions
    /// under the named A0.8 Erlang successor target profile.
    ///
    /// Inputs:
    /// - Temporary source file with a `Binary` return annotation and string
    ///   literal expression body.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A08Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_binary_for_a0_8_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_8_erlang_accepts_binary");
        let path = fixture(
            &dir,
            "\
module a0_8_erlang_accepts_binary.\n\npub value(): Binary ->\n    \"hello\".\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A08Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the A0.7 profile does not silently widen when A0.8 binary
    /// literal expressions are introduced.
    ///
    /// Inputs:
    /// - Temporary source file using a string literal expression, which belongs
    ///   to the named A0.8 successor matrix rather than A0.7.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A07Erlang`
    ///   and asserts the earlier successor profile still rejects the new
    ///   feature.
    #[test]
    fn run_check_single_file_keeps_binary_out_of_a0_7_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_7_erlang_rejects_binary");
        let path = fixture(
            &dir,
            "\
module a0_7_erlang_rejects_binary.\n\npub value(): Binary ->\n    \"hello\".\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A07Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts expression-side list cons under the
    /// named A0.9 Erlang successor target profile.
    ///
    /// Inputs:
    /// - Temporary source file with a list cons expression body.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A09Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_list_cons_for_a0_9_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_9_erlang_accepts_list_cons");
        let path = fixture(
            &dir,
            "\
module a0_9_erlang_accepts_list_cons.\n\npub prepend(head: Int, tail: List[Int]): List[Int] ->\n    [head | tail].\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A09Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the A0.8 profile does not silently widen when A0.9 list cons
    /// expressions are introduced.
    ///
    /// Inputs:
    /// - Temporary source file using expression-side list cons, which belongs to
    ///   the named A0.9 successor matrix rather than A0.8.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A08Erlang`
    ///   and asserts the earlier successor profile still rejects the new
    ///   feature.
    #[test]
    fn run_check_single_file_keeps_list_cons_out_of_a0_8_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_8_erlang_rejects_list_cons");
        let path = fixture(
            &dir,
            "\
module a0_8_erlang_rejects_list_cons.\n\npub prepend(head: Int, tail: List[Int]): List[Int] ->\n    [head | tail].\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A08Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts lowercase local named calls under
    /// the named A0.10 Erlang successor target profile.
    ///
    /// Inputs:
    /// - Temporary source file with a private local function and public caller.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A010Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_named_call_for_a0_10_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_10_erlang_accepts_named_call");
        let path = fixture(
            &dir,
            "\
module a0_10_erlang_accepts_named_call.\n\nidentity(x: Int): Int ->\n    x.\n\npub call_it(): Int ->\n    identity(1).\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A010Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the A0.9 profile does not silently widen when A0.10 local
    /// named-call expressions are introduced.
    ///
    /// Inputs:
    /// - Temporary source file using a lowercase local named call, which belongs
    ///   to the named A0.10 successor matrix rather than A0.9.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A09Erlang`
    ///   and asserts the earlier successor profile still rejects the new
    ///   feature.
    #[test]
    fn run_check_single_file_keeps_named_call_out_of_a0_9_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_9_erlang_rejects_named_call");
        let path = fixture(
            &dir,
            "\
module a0_9_erlang_rejects_named_call.\n\nidentity(x: Int): Int ->\n    x.\n\npub call_it(): Int ->\n    identity(1).\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A09Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts unary negation under the named
    /// A0.11 Erlang successor target profile.
    ///
    /// Inputs:
    /// - Temporary source file with a unary negation expression body.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A011Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_unary_neg_for_a0_11_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_11_erlang_accepts_unary_neg");
        let path = fixture(
            &dir,
            "\
module a0_11_erlang_accepts_unary_neg.\n\npub negate(value: Int): Int ->\n    -value.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A011Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the A0.10 profile does not silently widen when A0.11 unary
    /// negation expressions are introduced.
    ///
    /// Inputs:
    /// - Temporary source file using unary negation, which belongs to the named
    ///   A0.11 successor matrix rather than A0.10.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A010Erlang`
    ///   and asserts the earlier successor profile still rejects the new
    ///   feature.
    #[test]
    fn run_check_single_file_keeps_unary_neg_out_of_a0_10_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_10_erlang_rejects_unary_neg");
        let path = fixture(
            &dir,
            "\
module a0_10_erlang_rejects_unary_neg.\n\npub negate(value: Int): Int ->\n    -value.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A010Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts resolved constructor calls under
    /// the named A0.12 Erlang successor target profile.
    ///
    /// Inputs:
    /// - Temporary source file with an explicit constructor declaration and a
    ///   matching constructor call expression.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A012Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_constructor_call_for_a0_12_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_12_erlang_accepts_constructor_call");
        let path = fixture(
            &dir,
            "\
module a0_12_erlang_accepts_constructor_call.\n\npub constructor Ok {\n    (Value: Int): Dynamic ->\n        Value\n}.\n\npub make(): Dynamic ->\n    Ok(1).\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A012Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the A0.11 profile does not silently widen when A0.12
    /// constructor-call expressions are introduced.
    ///
    /// Inputs:
    /// - Temporary source file using a resolved constructor call, which belongs
    ///   to the named A0.12 successor matrix rather than A0.11.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A011Erlang`
    ///   and asserts the earlier successor profile still rejects the new
    ///   feature.
    #[test]
    fn run_check_single_file_keeps_constructor_call_out_of_a0_11_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_11_erlang_rejects_constructor_call");
        let path = fixture(
            &dir,
            "\
module a0_11_erlang_rejects_constructor_call.\n\npub constructor Ok {\n    (Value: Int): Dynamic ->\n        Value\n}.\n\npub make(): Dynamic ->\n    Ok(1).\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A011Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts resolved constructor patterns under
    /// the named A0.13 Erlang successor target profile.
    ///
    /// Inputs:
    /// - Temporary source file with an explicit constructor declaration and a
    ///   matching constructor pattern in a case expression.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A013Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_constructor_pattern_for_a0_13_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_13_erlang_accepts_constructor_pattern");
        let path = fixture(
            &dir,
            "\
module a0_13_erlang_accepts_constructor_pattern.\n\npub constructor Some {\n    (value: Dynamic): Dynamic ->\n        {:some, value}\n}.\n\npub unwrap(input: Dynamic): Dynamic ->\n    case input {\n        Some(value) -> value\n    }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A013Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the A0.12 profile does not silently widen when A0.13
    /// constructor-pattern forms are introduced.
    ///
    /// Inputs:
    /// - Temporary source file using a resolved constructor pattern, which
    ///   belongs to the named A0.13 successor matrix rather than A0.12.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A012Erlang`
    ///   and asserts the earlier successor profile still rejects the new
    ///   feature.
    #[test]
    fn run_check_single_file_keeps_constructor_pattern_out_of_a0_12_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_12_erlang_rejects_constructor_pattern");
        let path = fixture(
            &dir,
            "\
module a0_12_erlang_rejects_constructor_pattern.\n\npub constructor Some {\n    (value: Dynamic): Dynamic ->\n        {:some, value}\n}.\n\npub unwrap(input: Dynamic): Dynamic ->\n    case input {\n        Some(value) -> value\n    }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A012Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts anonymous function values under the
    /// named A0.14 Erlang successor target profile.
    ///
    /// Inputs:
    /// - Temporary source file with a `(x) -> x` expression body.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A014Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_lambda_for_a0_14_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_14_erlang_accepts_lambda");
        let path = fixture(
            &dir,
            "\
module a0_14_erlang_accepts_lambda.\n\npub id_fun(): Term ->\n    (x) -> x.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A014Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the A0.13 profile does not silently widen when A0.14 lambda
    /// expressions are introduced.
    ///
    /// Inputs:
    /// - Temporary source file using an anonymous function value.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A013Erlang`
    ///   and asserts the earlier successor profile still rejects the new
    ///   feature.
    #[test]
    fn run_check_single_file_keeps_lambda_out_of_a0_13_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_13_erlang_rejects_lambda");
        let path = fixture(
            &dir,
            "\
module a0_13_erlang_rejects_lambda.\n\npub id_fun(): Term ->\n    (x) -> x.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A013Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts constructor extension under the
    /// named A0.15 Erlang successor target profile.
    ///
    /// Inputs:
    /// - Temporary source file with `User(id, name) with Admin { ... }`.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A015Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_constructor_extension_for_a0_15_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_15_erlang_accepts_constructor_extension");
        let path = fixture(
            &dir,
            "\
module a0_15_erlang_accepts_constructor_extension.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub build(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id = id, name = name }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A015Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the A0.14 profile does not silently widen when A0.15
    /// constructor extension expressions are introduced.
    ///
    /// Inputs:
    /// - Temporary source file using `User(id, name) with Admin { ... }`.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A014Erlang`
    ///   and asserts the earlier successor profile still rejects the new
    ///   feature.
    #[test]
    fn run_check_single_file_keeps_constructor_extension_out_of_a0_14_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_14_erlang_rejects_constructor_extension");
        let path = fixture(
            &dir,
            "\
module a0_14_erlang_rejects_constructor_extension.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub build(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id = id, name = name }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A014Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts function-value invocation under the
    /// named A0.16 Erlang successor target profile.
    ///
    /// Inputs:
    /// - Temporary source file using dedicated `f.(value)` syntax.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A016Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_fun_call_for_a0_16_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_16_erlang_accepts_fun_call");
        let path = fixture(
            &dir,
            "\
module a0_16_erlang_accepts_fun_call.\n\npub apply(value: Int, f: (Int) -> Int): Int ->\n    f.(value).\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A016Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the A0.15 profile does not silently widen when A0.16
    /// function-value invocation syntax is introduced.
    ///
    /// Inputs:
    /// - Temporary source file using dedicated `f.(value)` syntax.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A015Erlang`
    ///   and asserts the earlier successor profile rejects the new expression
    ///   kind.
    #[test]
    fn run_check_single_file_keeps_fun_call_out_of_a0_15_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_15_erlang_rejects_fun_call");
        let path = fixture(
            &dir,
            "\
module a0_15_erlang_rejects_fun_call.\n\npub apply(value: Int, f: (Int) -> Int): Int ->\n    f.(value).\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A015Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts struct field access under the named
    /// A0.17 Erlang successor target profile.
    ///
    /// Inputs:
    /// - Temporary source file with a public struct and `point.x`.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A017Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_field_access_for_a0_17_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_17_erlang_accepts_field_access");
        let path = fixture(
            &dir,
            "\
module a0_17_erlang_accepts_field_access.\n\npub struct Point {\n    x: Int\n}.\n\npub read(point: Point): Int ->\n    point.x.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A017Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the A0.16 profile does not silently widen when A0.17 struct
    /// field access is introduced.
    ///
    /// Inputs:
    /// - Temporary source file with a public struct and `point.x`.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A016Erlang`
    ///   and asserts the earlier successor profile rejects the new expression
    ///   shape.
    #[test]
    fn run_check_single_file_keeps_field_access_out_of_a0_16_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_16_erlang_rejects_field_access");
        let path = fixture(
            &dir,
            "\
module a0_16_erlang_rejects_field_access.\n\npub struct Point {\n    x: Int\n}.\n\npub read(point: Point): Int ->\n    point.x.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A016Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts local let bindings under the named
    /// A0.18 Erlang successor target profile.
    ///
    /// Inputs:
    /// - Temporary source file with a `let y = ...; z = ...; body` expression.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A018Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_let_expr_for_a0_18_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_18_erlang_accepts_let_expr");
        let path = fixture(
            &dir,
            "\
module a0_18_erlang_accepts_let_expr.\n\npub calc(x: Int): Int ->\n    let y = x + 1; z = y * 2; z + y.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A018Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the A0.17 profile does not silently widen when A0.18 local let
    /// bindings are introduced.
    ///
    /// Inputs:
    /// - Temporary source file with a `let y = ...; z = ...; body` expression.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A017Erlang`
    ///   and asserts the earlier successor profile rejects the new expression
    ///   shape.
    #[test]
    fn run_check_single_file_keeps_let_expr_out_of_a0_17_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_17_erlang_rejects_let_expr");
        let path = fixture(
            &dir,
            "\
module a0_17_erlang_rejects_let_expr.\n\npub calc(x: Int): Int ->\n    let y = x + 1; z = y * 2; z + y.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A017Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts index access under the named A0.19
    /// Erlang successor target profile.
    ///
    /// Inputs:
    /// - Temporary source file with a `values[0]` expression.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A019Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_index_access_for_a0_19_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_19_erlang_accepts_index_access");
        let path = fixture(
            &dir,
            "\
module a0_19_erlang_accepts_index_access.\n\npub first(values: Dynamic): Dynamic ->\n    values[0].\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A019Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the A0.18 profile does not silently widen when A0.19 index
    /// access is introduced.
    ///
    /// Inputs:
    /// - Temporary source file with a `values[0]` expression.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A018Erlang`
    ///   and asserts the earlier successor profile rejects the new expression
    ///   shape.
    #[test]
    fn run_check_single_file_keeps_index_access_out_of_a0_18_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_18_erlang_rejects_index_access");
        let path = fixture(
            &dir,
            "\
module a0_18_erlang_rejects_index_access.\n\npub first(values: Dynamic): Dynamic ->\n    values[0].\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A018Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts qualified and scoped calls under
    /// the named A0.20 Erlang successor target profile.
    ///
    /// Inputs:
    /// - Temporary source file with lowercase module-path and uppercase
    ///   scoped-call expressions.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A020Erlang`
    ///   and asserts the documented successor matrix exits successfully.
    #[test]
    fn run_check_single_file_accepts_qualified_calls_for_a0_20_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_20_erlang_accepts_qualified_calls");
        let path = fixture(
            &dir,
            "\
module a0_20_erlang_accepts_qualified_calls.\n\npub qualified(): Dynamic ->\n    std.core.Math.add(1, 2).\n\npub scoped(): Dynamic ->\n    User.default().\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A020Erlang,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the A0.19 profile does not silently widen when A0.20
    /// qualified and scoped calls are introduced.
    ///
    /// Inputs:
    /// - Temporary source file with lowercase module-path and uppercase
    ///   scoped-call expressions.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A019Erlang`
    ///   and asserts the earlier successor profile rejects the new expression
    ///   shape.
    #[test]
    fn run_check_single_file_keeps_qualified_calls_out_of_a0_19_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_19_erlang_rejects_qualified_calls");
        let path = fixture(
            &dir,
            "\
module a0_19_erlang_rejects_qualified_calls.\n\npub qualified(): Dynamic ->\n    std.core.Math.add(1, 2).\n\npub scoped(): Dynamic ->\n    User.default().\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A019Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies receiver-method calls remain outside CoreIR v0 until method
    /// resolution is implemented.
    ///
    /// Inputs:
    /// - Temporary source file containing `receiver.method(args)` syntax in a
    ///   function body.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts method-call syntax is parsed but rejected before a successful
    ///   backend-ready result can be returned.
    #[test]
    fn run_check_single_file_rejects_method_call_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_method_call");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_method_call.\n\npub display(user: Dynamic): Dynamic ->\n    user.display_name(\"short\").\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects backend-specific remote function
    /// references under the named A0.21 Erlang diagnostic target profile.
    ///
    /// Inputs:
    /// - Temporary source file with backend-specific `fun module:function/arity`
    ///   expression.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A021Erlang`
    ///   and asserts backend-specific reference syntax is rejected by target
    ///   validation instead of being allowed into backend emission.
    #[test]
    fn run_check_single_file_rejects_remote_fun_ref_for_a0_21_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_21_erlang_rejects_remote_fun_ref");
        let path = fixture(
            &dir,
            "\
module a0_21_erlang_rejects_remote_fun_ref.\n\npub reference(): Dynamic ->\n    fun erlang:abs/1.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A021Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the frozen A0 profile does not silently widen when A0.1 is
    /// introduced.
    ///
    /// Inputs:
    /// - Temporary source file using subtraction, which belongs to the named
    ///   A0.1 successor matrix rather than the frozen A0 matrix.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::A0Erlang`
    ///   and asserts the frozen profile still rejects the successor feature.
    #[test]
    fn run_check_single_file_keeps_subtraction_out_of_a0_erlang_target_profile() {
        let dir = make_temp_dir("check_single_file_a0_erlang_rejects_subtraction");
        let path = fixture(
            &dir,
            "\
module a0_erlang_rejects_subtraction.\n\npub subtract(x: Int): Int ->\n    x - 1.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::A0Erlang,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command accepts resolved type-alias constructor
    /// calls under the portable CoreIR v0 target profile.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a Lean-covered
    ///   constructor call with identity from an eligible single-shape type
    ///   alias.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts resolved alias constructor calls remain inside the portable
    ///   CoreIR v0 subset.
    #[test]
    fn run_check_single_file_accepts_alias_constructor_call_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_accepts_alias_constructor_call");
        let path = fixture(
            &dir,
            "\
module core_v0_accepts_alias_constructor_call.\n\npub type Ok[T] = {:ok, value: T}.\n\npub value(): Dynamic ->\n    Ok(1).\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_eq!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command enforces the selected portable CoreIR v0
    /// target profile.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to map CoreIR.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts broad CoreIR is rejected before a successful result is
    ///   returned.
    #[test]
    fn run_check_single_file_rejects_map_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_map");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_map.\n\npub value(): Map ->\n    #{a := 1}.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects map patterns for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a typed case
    ///   expression containing a map pattern.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts proof-model-required map-pattern CoreIR is rejected before a
    ///   successful result is returned.
    #[test]
    fn run_check_single_file_rejects_map_pattern_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_map_pattern");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_map_pattern.\n\npub value(input: Dynamic): Dynamic ->\n    case input {\n        #{a = x} -> x;\n        _ -> input\n    }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects list-cons patterns for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a typed case
    ///   expression containing a list-cons pattern.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts proof-model-required list-cons-pattern CoreIR is rejected
    ///   before a successful result is returned.
    #[test]
    fn run_check_single_file_rejects_list_cons_pattern_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_list_cons_pattern");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_list_cons_pattern.\n\npub value(input: List[Int]): Dynamic ->\n    case input {\n        [head | tail] -> head;\n        _ -> input\n    }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects record patterns for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a typed case
    ///   expression containing a record pattern.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts proof-model-required record-pattern CoreIR is rejected before a
    ///   successful result is returned.
    #[test]
    fn run_check_single_file_rejects_record_pattern_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_record_pattern");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_record_pattern.\n\npub struct Point {\n    x: Int\n}.\n\npub value(input: Dynamic): Dynamic ->\n    case input {\n        #Point { x = x } -> x;\n        _ -> input\n    }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects float patterns for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a typed case
    ///   expression containing a float pattern.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts proof-model-required float-pattern CoreIR is rejected before a
    ///   successful result is returned.
    #[test]
    fn run_check_single_file_rejects_float_pattern_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_float_pattern");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_float_pattern.\n\npub value(input: Dynamic): Dynamic ->\n    case input {\n        1.0 -> :float;\n        _ -> :other\n    }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects floats for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a typed float
    ///   literal CoreIR expression.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts proof-model-required float CoreIR is rejected before a
    ///   successful result is returned.
    #[test]
    fn run_check_single_file_rejects_float_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_float");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_float.\n\npub value(): Dynamic ->\n    1.0.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects fixed arrays for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a typed
    ///   fixed-array CoreIR expression.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts proof-model-required fixed-array CoreIR is rejected before a
    ///   successful result is returned.
    #[test]
    fn run_check_single_file_rejects_fixed_array_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_fixed_array");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_fixed_array.\n\npub value(): Dynamic ->\n    #[1, 2, 3].\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects index access for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a typed index
    ///   CoreIR expression.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts proof-model-required index CoreIR is rejected before a
    ///   successful result is returned.
    #[test]
    fn run_check_single_file_rejects_index_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_index");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_index.\n\npub value(values: List[Int]): Dynamic ->\n    values[0].\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects list comprehensions for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a typed
    ///   list-comprehension CoreIR expression.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts proof-model-required list-comprehension CoreIR is rejected
    ///   before a successful result is returned.
    #[test]
    fn run_check_single_file_rejects_list_comprehension_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_list_comprehension");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_list_comprehension.\n\npub value(values: List[Int]): Dynamic ->\n    [value | value <- values].\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies multi-generator list comprehensions fail before semantic
    /// phases.
    ///
    /// Inputs:
    /// - A temporary single-file Terlan module containing a list comprehension
    ///   with two generators and a requested phase-manifest output path.
    ///
    /// Output:
    /// - Test assertion only; the command must fail and write a phase manifest
    ///   with a parse diagnostic and skipped resolve/typecheck/CoreIR phases.
    ///
    /// Transformation:
    /// - Runs the command-level check path and confirms the parser-level A0.24
    ///   collection contract is visible in phase output before unsupported
    ///   comprehension shape can reach semantic lowering.
    #[test]
    fn run_check_single_file_rejects_multi_generator_list_comprehension_before_phase_manifest() {
        let dir = make_temp_dir("check_single_file_multi_generator_list_comprehension_rejected");
        let source = dir.join("multi_generator_list_comprehension.tl");
        fs::write(
            &source,
            "module multi_generator_list_comprehension.\n\npub value(values: List[Int], others: List[Int]): Dynamic ->\n  [value | value <- values, other <- others].\n",
        )
        .expect("write multi-generator list comprehension source");
        let manifest = dir.join("multi_generator_list_comprehension.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );

        assert_ne!(exit, ExitCode::SUCCESS);
        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"parse","status":"error""#));
        assert!(
            manifest_text.contains("multiple list comprehension generators are not supported"),
            "{manifest_text}"
        );
        assert!(manifest_text.contains(r#""name":"resolve","status":"skipped""#));
        assert!(manifest_text.contains(r#""name":"typecheck","status":"skipped""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
    }

    /// Verifies binary segment lowering fails before backend emission.
    ///
    /// Inputs:
    /// - A temporary single-file Terlan module containing structured binary
    ///   segment syntax and a requested phase-manifest output path.
    ///
    /// Output:
    /// - Test assertion only; the command must fail and write a phase manifest
    ///   whose CoreIR phase records the target-profile diagnostic.
    ///
    /// Transformation:
    /// - Runs the command-level check path and confirms the parser preserves
    ///   binary segment text while target-profile validation rejects deferred
    ///   segment lowering before backend-ready success.
    #[test]
    fn run_check_single_file_rejects_binary_segment_lowering_in_phase_manifest() {
        let dir = make_temp_dir("check_single_file_binary_segment_lowering_rejected");
        let source = dir.join("binary_segment_lowering.tl");
        fs::write(
            &source,
            "module binary_segment_lowering.\n\npub byte(value: Int): Binary ->\n  <<value:8/integer-unsigned-big>>.\n",
        )
        .expect("write binary segment source");
        let manifest = dir.join("binary_segment_lowering.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );

        assert_ne!(exit, ExitCode::SUCCESS);
        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"error""#));
        assert!(manifest_text.contains("binary segment lowering"));
        assert!(manifest_text.contains("<<value:8/integer-unsigned-big>>"));
    }

    /// Verifies the `check` command rejects receive expressions for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a typed receive {
    ///   CoreIR expression with a timeout branch.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts proof-model-required receive CoreIR is rejected before a
    ///   successful result is returned.
    #[test]
    fn run_check_single_file_rejects_receive_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_receive");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_receive.\n\npub value(): Dynamic ->\n    receive {\n        value -> value;\n    after 0 -> :timeout\n    }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects try expressions for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a typed try CoreIR
    ///   expression with `of`, `catch`, and `after` branches.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts proof-model-required try CoreIR is rejected before a
    ///   successful result is returned.
    #[test]
    fn run_check_single_file_rejects_try_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_try");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_try.\n\npub value(): Dynamic ->\n    try 1 {\n        value -> value\n    catch\n        reason -> reason\n    after\n        0 -> :done\n    }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects quote expressions for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body parses as a `quote`
    ///   keyword expression and typechecks as an AST value.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts runtime-boundary quote syntax is rejected before a successful
    ///   backend-ready result is returned.
    #[test]
    fn run_check_single_file_rejects_quote_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_quote");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_quote.\n\npub value(x: Int): Ast[Int] ->\n    quote x.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects unquote expressions for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body parses as an `unquote`
    ///   keyword expression and typechecks to the inner expression type.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts runtime-boundary unquote syntax is rejected before a
    ///   successful backend-ready result is returned.
    #[test]
    fn run_check_single_file_rejects_unquote_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_unquote");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_unquote.\n\npub value(x: Int): Int ->\n    unquote(x).\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects guarded case clauses for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a case expression
    ///   with a clause guard.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts guarded branch semantics stay out of the Lean-covered CoreV0
    ///   subset until their proof model is explicit.
    #[test]
    fn run_check_single_file_rejects_guarded_case_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_guarded_case");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_guarded_case.\n\npub value(x: Int): Int ->\n    case x {\n        value when value > 0 -> value;\n        _ -> 0\n    }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects partial case branch bodies for
    /// CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose case expression is syntactically valid but
    ///   has quote expressions as branch bodies.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts summary-only branch bodies prevent the enclosing keyword
    ///   expression from being accepted as backend-ready CoreV0.
    #[test]
    fn run_check_single_file_rejects_partial_case_branch_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_partial_case_branch");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_partial_case_branch.\n\npub value(x: Int): Ast[Int] ->\n    case x {\n        0 -> quote x;\n        _ -> quote x\n    }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects constructor chains for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a constructor-chain
    ///   CoreIR expression with resolved base constructor identity.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts partial constructor-chain CoreIR is rejected before a
    ///   successful result is returned.
    #[test]
    fn run_check_single_file_rejects_constructor_chain_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_constructor_chain");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_constructor_chain.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub value(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id = id, name = name }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects type-alias constructor chains for
    /// CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a constructor-chain
    ///   CoreIR expression with resolved base identity from an eligible
    ///   single-shape type alias.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts alias identity evidence does not promote constructor-chain
    ///   semantics into the portable subset.
    #[test]
    fn run_check_single_file_rejects_alias_constructor_chain_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_alias_constructor_chain");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_alias_constructor_chain.\n\npub type User = {:user, id: Int, name: Binary}.\n\npub value(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id = id, name = name }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects remote calls for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a typed remote-call
    ///   CoreIR expression.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts proof-model-required remote-call CoreIR is rejected before a
    ///   successful result is returned.
    #[test]
    fn run_check_single_file_rejects_remote_call_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_remote_call");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_remote_call.\n\npub value(): Int ->\n    erlang.abs(1).\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects remote function references for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a typed remote
    ///   function reference CoreIR expression.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts proof-model-required remote function reference CoreIR is
    ///   rejected before a successful result is returned.
    #[test]
    fn run_check_single_file_rejects_remote_fun_ref_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_remote_fun_ref");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_remote_fun_ref.\n\npub value(): Dynamic ->\n    fun erlang:abs/1.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects record construction for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a typed record
    ///   construction CoreIR expression.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts proof-model-required record construction CoreIR is rejected
    ///   before a successful result is returned.
    #[test]
    fn run_check_single_file_rejects_record_construct_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_record_construct");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_record_construct.\n\npub value(): Dynamic ->\n    #Point { x = 1 }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects record access for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a typed record
    ///   access CoreIR expression.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts proof-model-required record access CoreIR is rejected before a
    ///   successful result is returned.
    #[test]
    fn run_check_single_file_rejects_record_access_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_record_access");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_record_access.\n\npub struct Point {\n    x: Int\n}.\n\npub value(point: Point): Dynamic ->\n    point#Point.x.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects record updates for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a typed record
    ///   update CoreIR expression.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts proof-model-required record update CoreIR is rejected before a
    ///   successful result is returned.
    #[test]
    fn run_check_single_file_rejects_record_update_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_record_update");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_record_update.\n\npub struct Point {\n    x: Int\n}.\n\npub value(point: Point): Dynamic ->\n    point#Point { x = 1 }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies the `check` command rejects template instantiation for CoreIR v0.
    ///
    /// Inputs:
    /// - Temporary source file whose function body lowers to a typed
    ///   template-instantiation CoreIR expression.
    ///
    /// Output:
    /// - Test assertion only; the temporary source file is deleted by the OS
    ///   temp directory lifecycle.
    ///
    /// Transformation:
    /// - Runs the command-level `check` path with `TargetProfile::CoreV0` and
    ///   asserts proof-model-required template-instantiation CoreIR is rejected
    ///   before a successful result is returned.
    #[test]
    fn run_check_single_file_rejects_template_instantiate_for_core_v0_target_profile() {
        let dir = make_temp_dir("check_single_file_core_v0_rejects_template_instantiate");
        let template_dir = dir.join("templates");
        fs::create_dir_all(&template_dir).expect("create template dir");
        fs::write(template_dir.join("user_card.tl.html"), "<p>{name}</p>")
            .expect("write template file");
        let path = fixture(
            &dir,
            "\
module core_v0_rejects_template_instantiate.\n\ntemplate UserCard from \"./templates/user_card.tl.html\" {\n    name: Text\n}.\n\npub value(): Dynamic ->\n    UserCard{ name = \"Ada\" }.\n",
        );

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![path],
            },
            CliState {
                target_profile: TargetProfile::CoreV0,
                ..Default::default()
            },
        );

        assert_ne!(exit, ExitCode::SUCCESS);
    }

    /// Verifies unsupported subject-bearing annotations stop in the syntax
    /// phase of the command-level `check` path.
    ///
    /// Inputs:
    /// - A temporary module containing an unambiguous annotation subject and a
    ///   requested phase-manifest output path.
    ///
    /// Output:
    /// - Test assertion only; the command must fail and write a phase manifest
    ///   with parse error plus skipped later phases.
    ///
    /// Transformation:
    /// - Runs `terlc check` through the normal command entrypoint and confirms
    ///   the A0.32 annotation-subject diagnostic is visible in phase output
    ///   and prevents resolution, typecheck, and CoreIR from running.
    #[test]
    fn run_check_single_file_rejects_annotation_subject_before_phase_manifest() {
        let dir = make_temp_dir("check_single_file_annotation_subject_rejected");
        let source = dir.join("annotation_subject.tl");
        fs::write(
            &source,
            "module annotation_subject.\n\n@doc \"User type\"\ntype User = Int.\n",
        )
        .expect("write annotation subject source");
        let manifest = dir.join("annotation_subject.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );

        assert_ne!(exit, ExitCode::SUCCESS);
        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"parse","status":"error""#));
        assert!(manifest_text.contains("annotation subjects are not supported in Terlan 0.0.1"));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
    }

    /// Verifies asset imports fail in the generic formal compile path before
    /// backend emission.
    ///
    /// Inputs:
    /// - A temporary Terlan module with a CSS asset import and a simple
    ///   backend-supported function.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` fails in the
    ///   CoreIR target-profile phase and records the unsupported asset-import
    ///   decision in the manifest.
    ///
    /// Transformation:
    /// - Runs the command-level check path and confirms parse/resolve/typecheck
    ///   can accept the syntax while CoreIR target-profile validation rejects
    ///   unresolved asset import resolution.
    #[test]
    fn run_check_single_file_rejects_asset_import_resolution_in_phase_manifest() {
        let dir = make_temp_dir("check_single_file_asset_import_rejected");
        let source = dir.join("asset_import.tl");
        fs::write(
            &source,
            "module asset_import.\n\nimport css \"./style.css\" as PageCss.\n\npub main(): Int ->\n    1.\n",
        )
        .expect("write asset import source");
        let manifest = dir.join("asset_import.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );

        assert_ne!(exit, ExitCode::SUCCESS);
        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"error""#));
        assert!(manifest_text.contains("asset import resolution Css import `PageCss<-./style.css`"));
    }

    /// Verifies constructor declaration edge cases fail before backend phases.
    ///
    /// Inputs:
    /// - Temporary Terlan modules containing unsupported constructor
    ///   default/vararg/arity shapes plus requested phase-manifest paths.
    ///
    /// Output:
    /// - Test assertions only; each command run must fail and write a phase
    ///   manifest with the parse diagnostic and skipped CoreIR phase.
    ///
    /// Transformation:
    /// - Runs each constructor edge-case source through command-level
    ///   `terlc check --emit-phase-manifest` and confirms A0.32 constructor
    ///   diagnostics are visible before resolution, typecheck, or backend
    ///   emission can run.
    #[test]
    fn run_check_single_file_rejects_constructor_edge_cases_before_phase_manifest() {
        let cases = [
            (
                "constructor_varargs_not_last",
                "constructor varargs parameter must be last",
                "module bad.constructor_varargs_not_last.\n\nconstructor Bad {\n  (...items: Int, last: Int): Bad -> 1\n}.\n",
            ),
            (
                "constructor_default_not_trailing",
                "constructor default parameters must be trailing",
                "module bad.constructor_default_not_trailing.\n\nconstructor Bad {\n  (first: Int = 1, second: Int): Bad -> 1\n}.\n",
            ),
            (
                "constructor_varargs_with_default",
                "constructor varargs parameters cannot have defaults",
                "module bad.constructor_varargs_with_default.\n\nconstructor Bad {\n  (...items: Int = []): Bad -> 1\n}.\n",
            ),
            (
                "constructor_duplicate_varargs_clauses",
                "constructor has ambiguous varargs clauses",
                "module bad.constructor_duplicate_varargs_clauses.\n\nconstructor Bad {\n  (...items: Int): Bad -> 1;\n  (first: Int, ...rest: Int): Bad -> 1\n}.\n",
            ),
            (
                "constructor_overlapping_default_arity",
                "constructor has ambiguous arity clauses",
                "module bad.constructor_overlapping_default_arity.\n\nconstructor Bad {\n  (first: Int): Bad -> 1;\n  (first: Int, second: Int = 1): Bad -> 1\n}.\n",
            ),
        ];

        for (fixture_name, expected_message, source_text) in cases {
            let dir = make_temp_dir(&format!("check_single_file_{fixture_name}_rejected"));
            let source = dir.join(format!("{fixture_name}.tl"));
            fs::write(&source, source_text).expect("write constructor edge-case source");
            let manifest = dir.join(format!("{fixture_name}.phase-manifest.json"));

            let exit = commands::check::run(
                CliCommand {
                    verb: Some("check".into()),
                    args: vec![
                        source.to_string_lossy().into(),
                        "--emit-phase-manifest".into(),
                        manifest.to_string_lossy().into(),
                    ],
                },
                CliState::default(),
            );

            assert_ne!(exit, ExitCode::SUCCESS, "{fixture_name}");
            let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
            assert!(
                manifest_text.contains(r#""name":"parse","status":"error""#),
                "{fixture_name}: {manifest_text}"
            );
            assert!(
                manifest_text.contains(expected_message),
                "{fixture_name}: {manifest_text}"
            );
            assert!(
                manifest_text.contains(r#""name":"resolve","status":"skipped""#),
                "{fixture_name}: {manifest_text}"
            );
            assert!(
                manifest_text.contains(r#""name":"core","status":"skipped""#),
                "{fixture_name}: {manifest_text}"
            );
        }
    }

    /// Verifies unsupported function declaration and clause shapes fail early.
    ///
    /// Inputs:
    /// - Temporary Terlan modules containing function defaults, function
    ///   varargs, mismatched secondary clause names, and mismatched secondary
    ///   clause arities.
    ///
    /// Output:
    /// - Test assertions only; each command run must fail and write a phase
    ///   manifest with the parse diagnostic and skipped CoreIR phase.
    ///
    /// Transformation:
    /// - Runs each unsupported function source through command-level
    ///   `terlc check --emit-phase-manifest` and confirms the A0.32
    ///   function/clauses decision is visible before semantic lowering or
    ///   backend emission.
    #[test]
    fn run_check_single_file_rejects_function_clause_edge_cases_before_phase_manifest() {
        let cases = [
            (
                "function_default_param",
                "function default parameters are not supported in Terlan 0.0.1",
                "module bad.function_default_param.\n\npub add(x: Int = 1): Int ->\n  x.\n",
            ),
            (
                "function_varargs_param",
                "function varargs parameters are not supported in Terlan 0.0.1",
                "module bad.function_varargs_param.\n\npub sum(...items: Int): Int ->\n  0.\n",
            ),
            (
                "function_clause_name_mismatch",
                "expected Dot",
                "module bad.function_clause_name_mismatch.\n\nvalue(0) -> 0;\nother(1) -> 1.\n",
            ),
            (
                "function_clause_arity_mismatch",
                "clause for value has arity 2, expected 1",
                "module bad.function_clause_arity_mismatch.\n\nvalue(0) -> 0;\nvalue(1, 2) -> 1.\n",
            ),
        ];

        for (fixture_name, expected_message, source_text) in cases {
            let dir = make_temp_dir(&format!("check_single_file_{fixture_name}_rejected"));
            let source = dir.join(format!("{fixture_name}.tl"));
            fs::write(&source, source_text).expect("write function edge-case source");
            let manifest = dir.join(format!("{fixture_name}.phase-manifest.json"));

            let exit = commands::check::run(
                CliCommand {
                    verb: Some("check".into()),
                    args: vec![
                        source.to_string_lossy().into(),
                        "--emit-phase-manifest".into(),
                        manifest.to_string_lossy().into(),
                    ],
                },
                CliState::default(),
            );

            assert_ne!(exit, ExitCode::SUCCESS, "{fixture_name}");
            let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
            assert!(
                manifest_text.contains(r#""name":"parse","status":"error""#),
                "{fixture_name}: {manifest_text}"
            );
            assert!(
                manifest_text.contains(expected_message),
                "{fixture_name}: {manifest_text}"
            );
            assert!(
                manifest_text.contains(r#""name":"resolve","status":"skipped""#),
                "{fixture_name}: {manifest_text}"
            );
            assert!(
                manifest_text.contains(r#""name":"core","status":"skipped""#),
                "{fixture_name}: {manifest_text}"
            );
        }
    }

    /// Verifies generic `check` rejects unresolved external template bodies.
    ///
    /// Inputs:
    /// - A temporary Terlan module that declares and instantiates an external
    ///   template whose source file is absent.
    ///
    /// Output:
    /// - Test assertions only; the command must fail and write a phase
    ///   manifest with a typecheck diagnostic and skipped CoreIR phase.
    ///
    /// Transformation:
    /// - Runs `terlc check --emit-phase-manifest` through the formal pipeline
    ///   and confirms template body resolution is validated before CoreIR or
    ///   backend emission unless a command owns template loading/rendering.
    #[test]
    fn run_check_single_file_rejects_unresolved_template_body_before_core_phase() {
        let dir = make_temp_dir("check_single_file_template_body_rejected");
        let source = dir.join("template_body.tl");
        fs::write(
            &source,
            "module template_body.\n\ntemplate Page from \"./templates/missing.tl.html\" {\n  title: Text\n}.\n\npub home(): Html[Never] ->\n  Page{ title = \"Home\" }.\n",
        )
        .expect("write unresolved template source");
        let manifest = dir.join("template_body.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );

        assert_ne!(exit, ExitCode::SUCCESS);
        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"resolve","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains("failed to read template"));
        assert!(manifest_text.contains("missing.tl.html"));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
    }

    /// Verifies config metadata entries are visible but non-semantic in 0.0.1.
    ///
    /// Inputs:
    /// - A temporary single-file Terlan module containing a `target` config
    ///   declaration with structured metadata entries and one simple function.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds, records a
    ///   warning in the typecheck phase, and still lowers the function to CoreIR.
    ///
    /// Transformation:
    /// - Runs the generic formal compiler path and confirms config entries are
    ///   preserved as source metadata instead of being silently treated as backend
    ///   semantics.
    #[test]
    fn run_check_single_file_warns_for_unconsumed_config_entries_in_phase_manifest() {
        let dir = make_temp_dir("check_single_file_config_entries_warn");
        let source = dir.join("config_entries.tl");
        fs::write(
            &source,
            "module config_entries.\n\ntarget erlang {\n  otp_application: true;\n  features: [sockets]\n}.\n\npub value(): Int ->\n  1.\n",
        )
        .expect("write config entry source");
        let manifest = dir.join("config_entries.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );

        assert_eq!(exit, ExitCode::SUCCESS);
        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""code":"type_warning""#));
        assert!(manifest_text.contains("config metadata entries for `target erlang`"));
        assert!(manifest_text.contains("preserved but not semantically consumed"));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
    }

    /// Verifies raw macro primary expressions fail before semantic phases.
    ///
    /// Inputs:
    /// - A temporary single-file Terlan module containing a raw macro expression
    ///   as a function body and a requested phase-manifest output path.
    ///
    /// Output:
    /// - Test assertion only; the command must fail and write a phase manifest
    ///   with a macro-expansion diagnostic and skipped resolve/typecheck/CoreIR
    ///   phases.
    ///
    /// Transformation:
    /// - Runs the command-level check path and confirms raw macro syntax is
    ///   preserved by parsing but cannot leak into backend lowering without an
    ///   explicit macro-resolution implementation.
    #[test]
    fn run_check_single_file_rejects_raw_macro_primary_before_phase_manifest() {
        let dir = make_temp_dir("check_single_file_raw_macro_primary_rejected");
        let source = dir.join("raw_macro_primary.tl");
        fs::write(
            &source,
            "module raw_macro_primary.\n\npub query(): Dynamic ->\n  sql{select * from users}.\n",
        )
        .expect("write raw macro primary source");
        let manifest = dir.join("raw_macro_primary.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );

        assert_ne!(exit, ExitCode::SUCCESS);
        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"parse","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"macro_expansion","status":"error""#));
        assert!(
            manifest_text.contains("raw macro expression `sql` requires macro resolution"),
            "{manifest_text}"
        );
        assert!(manifest_text.contains(r#""name":"resolve","status":"skipped""#));
        assert!(manifest_text.contains(r#""name":"typecheck","status":"skipped""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
    }

    /// Verifies a declaration-only check emits a no-expressions Core manifest.
    ///
    /// Inputs:
    /// - A temporary single-file Terlan module containing only a public type
    ///   alias.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   manifest reports CoreIR readiness as `no-expressions` with one typed
    ///   CoreType payload and no expression or pattern payloads.
    ///
    /// Transformation:
    /// - Runs the CLI check command through the formal pipeline and validates
    ///   the emitted phase-manifest JSON.
    #[test]
    fn run_check_single_file_type_only_emits_no_expressions_manifest() {
        let dir = make_temp_dir("check_single_file_no_expressions_manifest");
        let source = dir.join("type_only.tl");
        fs::write(&source, "module type_only.\n\npub type UserId = Int.\n")
            .expect("write type-only source");
        let manifest = dir.join("type_only.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_ne!(
            manifest_json["core_ir_hash"]
                .as_u64()
                .expect("core ir hash"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["readiness"]
                .as_str()
                .expect("core proof readiness"),
            "no-expressions"
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["typed_core_type"]
                .as_u64()
                .expect("typed CoreType count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["summary_only_type"]
                .as_u64()
                .expect("summary-only CoreType count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["typed_core_expr"]
                .as_u64()
                .expect("typed CoreExpr count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["typed_core_pattern"]
                .as_u64()
                .expect("typed CorePattern count"),
            0
        );
    }

    /// Verifies declaration-only summary type debt reaches phase manifests.
    ///
    /// Inputs:
    /// - A temporary single-file Terlan module containing only a public struct
    ///   declaration whose body is not yet modeled as typed CoreType.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   manifest reports CoreIR readiness as `proof-model-required` with one
    ///   summary-only CoreType payload.
    ///
    /// Transformation:
    /// - Runs the CLI check command through the formal pipeline and validates
    ///   that type-model debt prevents declaration-only CoreIR from reporting
    ///   `no-expressions`.
    #[test]
    fn run_check_single_file_struct_only_emits_typed_struct_body_manifest() {
        let dir = make_temp_dir("check_single_file_summary_type_debt_manifest");
        let source = dir.join("struct_only.tl");
        fs::write(
            &source,
            "module struct_only.\n\npub struct Point {\n    x: Int,\n    y: Int\n}.\n",
        )
        .expect("write struct-only source");
        let manifest = dir.join("struct_only.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_ne!(
            manifest_json["core_ir_hash"]
                .as_u64()
                .expect("core ir hash"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["readiness"]
                .as_str()
                .expect("core proof readiness"),
            "no-expressions"
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["typed_core_type"]
                .as_u64()
                .expect("typed CoreType count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["summary_only_type"]
                .as_u64()
                .expect("summary-only CoreType count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["typed_core_expr"]
                .as_u64()
                .expect("typed CoreExpr count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["typed_core_pattern"]
                .as_u64()
                .expect("typed CorePattern count"),
            0
        );
    }

    /// Verifies lambda freshness obligations reach phase manifests.
    ///
    /// Inputs:
    /// - A temporary single-file Terlan module whose public function returns
    ///   an anonymous function expression with one runtime parameter binding.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   manifest partitions expression preservation evidence into one
    ///   no-runtime-binding child and one runtime-binding lambda root.
    ///
    /// Transformation:
    /// - Runs the CLI check command through the formal pipeline and validates
    ///   the freshness buckets future Lean proof export will need for lambda
    ///   substitution evidence.
    #[test]
    fn run_check_single_file_lambda_emits_runtime_binding_freshness_manifest() {
        let dir = make_temp_dir("check_single_file_lambda_freshness_manifest");
        let source = dir.join("lambda_freshness.tl");
        fs::write(
            &source,
            "module lambda_freshness.\n\npub id_fun(): Term ->\n    (x) -> x.\n",
        )
        .expect("write lambda freshness source");
        let manifest = dir.join("lambda_freshness.phase-manifest.json");

        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState::default(),
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["readiness"]
                .as_str()
                .expect("core proof readiness"),
            "lean-covered"
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["typed_core_expr"]
                .as_u64()
                .expect("typed CoreExpr count"),
            2
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["checked_preservation_expr"]
                .as_u64()
                .expect("checked-preservation expression count"),
            2
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["checked_preservation_expr_no_runtime_bindings"]
                .as_u64()
                .expect("no-runtime-bindings expression count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]
                ["checked_preservation_expr_runtime_bindings_required"]
                .as_u64()
                .expect("runtime-bindings-required expression count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["checked_preservation_pattern"]
                .as_u64()
                .expect("checked-preservation pattern count"),
            0
        );
    }

    #[test]
    fn run_check_single_file_rejects_unknown_constructor_before_core_phase() {
        let dir = make_temp_dir("check_single_file_unresolved_constructor_candidate");
        let source = dir.join("constructor_candidate.tl");
        fs::write(
            &source,
            "module constructor_candidate.\n\npub value(): Dynamic ->\n    Ok(1).\n",
        )
        .expect("write unresolved constructor candidate source");
        let manifest = dir.join("constructor_candidate.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
        assert!(manifest_text.contains(r#""code":"type_error""#));
        assert!(manifest_text.contains("unknown constructor Ok / 1"));
    }

    /// Verifies imported public struct type identity does not permit raw
    /// construction outside the defining module.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports `Point`.
    /// - A temporary consumer `.tl` module that imports `Point` as a type and
    ///   attempts `#Point { ... }` raw construction.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` fails in the
    ///   typecheck phase, skips CoreIR, and reports the raw-construction
    ///   visibility error.
    ///
    /// Transformation:
    /// - Runs command-level check through sibling-interface loading, name
    ///   resolution, typechecking, and phase-manifest emission to pin the
    ///   constructor-boundary rule on the formal compiler path.
    #[test]
    fn run_check_single_file_rejects_imported_raw_struct_construction_before_core_phase() {
        let dir = make_temp_dir("check_single_file_imported_raw_struct_construction");
        let provider = dir.join("provider.tli");
        fs::write(
            &provider,
            "module provider.\n\npub struct Point {\n    x: Int\n}.\n",
        )
        .expect("write provider struct interface");

        let source = dir.join("imported_raw_struct_construction.tl");
        fs::write(
            &source,
            "module imported_raw_struct_construction.\n\nimport type provider.Point.\n\npub value(): Dynamic ->\n    #Point { x = 1 }.\n",
        )
        .expect("write imported raw struct construction source");
        let manifest = dir.join("imported_raw_struct_construction.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
        assert!(manifest_text.contains(r#""code":"type_error""#));
        assert!(manifest_text.contains("cannot raw-construct imported struct provider.Point"));
    }

    /// Verifies public constructors cannot expose private return types.
    ///
    /// Inputs:
    /// - A temporary single-file Terlan module with a private `Secret` struct
    ///   and public constructor returning `Secret`.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` fails in the
    ///   typecheck phase, skips CoreIR, and reports the constructor visibility
    ///   diagnostic.
    ///
    /// Transformation:
    /// - Runs command-level check through parsing, resolution, typechecking,
    ///   and phase-manifest emission to prove constructor API visibility is
    ///   enforced before CoreIR/backend emission.
    #[test]
    fn run_check_single_file_rejects_public_constructor_private_return_before_core_phase() {
        let dir = make_temp_dir("check_single_file_public_constructor_private_return");
        let source = dir.join("public_constructor_private_return.tl");
        fs::write(
            &source,
            "module public_constructor_private_return.\n\nstruct Secret {\n    value: Int\n}.\n\npub constructor Secret {\n    (value: Int): Secret -> value\n}.\n",
        )
        .expect("write public constructor private return source");
        let manifest = dir.join("public_constructor_private_return.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
        assert!(manifest_text.contains(r#""code":"type_error""#));
        assert!(
            manifest_text.contains("public constructor Secret exposes private return type Secret")
        );
    }

    /// Verifies eligible type-alias constructor calls with wrong arity fail
    /// before CoreIR.
    ///
    /// Inputs:
    /// - A temporary single-file Terlan module that declares `Ok[T] =
    ///   {:ok, value: T}` and calls `Ok()` with no payload.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` fails in the
    ///   typecheck phase, skips CoreIR, and reports the constructor arity
    ///   mismatch.
    ///
    /// Transformation:
    /// - Runs command-level check through typechecking and confirms eligible
    ///   type-alias constructors remain semantically resolved enough to report
    ///   arity errors rather than unresolved constructor metadata.
    #[test]
    fn run_check_single_file_rejects_alias_constructor_wrong_arity_before_core_phase() {
        let dir = make_temp_dir("check_single_file_alias_constructor_wrong_arity");
        let source = dir.join("alias_constructor_wrong_arity.tl");
        fs::write(
            &source,
            "module alias_constructor_wrong_arity.\n\npub type Ok[T] = {:ok, value: T}.\n\npub value(): Dynamic ->\n    Ok().\n",
        )
        .expect("write alias constructor wrong-arity source");
        let manifest = dir.join("alias_constructor_wrong_arity.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
        assert!(manifest_text.contains(r#""code":"type_error""#));
        assert!(manifest_text
            .contains("constructor Ok has arity mismatch: expected 1..1 args, found 0"));
    }

    /// Verifies imported eligible type-alias constructor calls with wrong arity
    /// fail before CoreIR.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports `Ok[T] =
    ///   {:ok, value: T}`.
    /// - A temporary consumer `.tl` module that imports `Ok` and calls `Ok()`.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` fails in the
    ///   typecheck phase, skips CoreIR, and reports the constructor arity
    ///   mismatch.
    ///
    /// Transformation:
    /// - Runs command-level check through sibling-interface loading and
    ///   typechecking, confirming imported eligible aliases report arity
    ///   failures rather than unresolved constructor metadata.
    #[test]
    fn run_check_single_file_rejects_imported_alias_constructor_wrong_arity_before_core_phase() {
        let dir = make_temp_dir("check_single_file_imported_alias_constructor_wrong_arity");
        let provider = dir.join("result.tli");
        fs::write(
            &provider,
            "module result.\n\npub type Ok[T] = {:ok, value: T}.\n",
        )
        .expect("write provider alias constructor interface");

        let source = dir.join("imported_alias_constructor_wrong_arity.tl");
        fs::write(
            &source,
            "module imported_alias_constructor_wrong_arity.\n\nimport result.{Ok}.\n\npub value(): Dynamic ->\n    Ok().\n",
        )
        .expect("write imported alias constructor wrong-arity source");
        let manifest = dir.join("imported_alias_constructor_wrong_arity.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
        assert!(manifest_text.contains(r#""code":"type_error""#));
        assert!(manifest_text
            .contains("constructor Ok has arity mismatch: expected 1..1 args, found 0"));
    }

    /// Verifies aliased imported eligible type-alias constructor calls with
    /// wrong arity fail before CoreIR and report the source alias name.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports `Ok[T] =
    ///   {:ok, value: T}`.
    /// - A temporary consumer `.tl` module that imports `Ok as Success` and
    ///   calls `Success()`.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` fails in the
    ///   typecheck phase, skips CoreIR, and reports the constructor arity
    ///   mismatch for `Success`.
    ///
    /// Transformation:
    /// - Runs command-level check through sibling-interface loading,
    ///   alias-aware import resolution, and typechecking, confirming aliased
    ///   eligible alias calls report arity failures rather than unresolved
    ///   constructor metadata.
    #[test]
    fn run_check_single_file_rejects_aliased_imported_alias_constructor_wrong_arity_before_core_phase(
    ) {
        let dir = make_temp_dir("check_single_file_aliased_imported_alias_constructor_wrong_arity");
        let provider = dir.join("result.tli");
        fs::write(
            &provider,
            "module result.\n\npub type Ok[T] = {:ok, value: T}.\n",
        )
        .expect("write provider alias constructor interface");

        let source = dir.join("aliased_imported_alias_constructor_wrong_arity.tl");
        fs::write(
            &source,
            "module aliased_imported_alias_constructor_wrong_arity.\n\nimport result.{Ok as Success}.\n\npub value(): Dynamic ->\n    Success().\n",
        )
        .expect("write aliased imported alias constructor wrong-arity source");
        let manifest =
            dir.join("aliased_imported_alias_constructor_wrong_arity.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
        assert!(manifest_text.contains(r#""code":"type_error""#));
        assert!(manifest_text
            .contains("constructor Success has arity mismatch: expected 1..1 args, found 0"));
    }

    /// Verifies imported eligible type-alias constructor patterns with wrong
    /// arity fail before CoreIR.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports `Ok[T] =
    ///   {:ok, value: T}`.
    /// - A temporary consumer `.tl` module that imports `Ok` and matches
    ///   `Ok(value, extra)`.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` fails in the
    ///   typecheck phase, skips CoreIR, and reports the constructor-pattern
    ///   arity mismatch.
    ///
    /// Transformation:
    /// - Runs command-level check through sibling-interface loading and
    ///   typechecking, confirming imported eligible alias patterns report arity
    ///   failures rather than unresolved constructor-pattern metadata.
    #[test]
    fn run_check_single_file_rejects_imported_alias_constructor_pattern_wrong_arity_before_core_phase(
    ) {
        let dir = make_temp_dir("check_single_file_imported_alias_constructor_pattern_wrong_arity");
        let provider = dir.join("result.tli");
        fs::write(
            &provider,
            "module result.\n\npub type Ok[T] = {:ok, value: T}.\n",
        )
        .expect("write provider alias constructor interface");

        let source = dir.join("imported_alias_constructor_pattern_wrong_arity.tl");
        fs::write(
            &source,
            "module imported_alias_constructor_pattern_wrong_arity.\n\nimport result.{Ok}.\n\npub unwrap(input: Ok[Int]): Int ->\n    case input {\n        Ok(value, extra) -> value\n    }.\n",
        )
        .expect("write imported alias constructor pattern wrong-arity source");
        let manifest =
            dir.join("imported_alias_constructor_pattern_wrong_arity.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
        assert!(manifest_text.contains(r#""code":"type_error""#));
        assert!(manifest_text
            .contains("constructor Ok has arity mismatch: expected 1..1 args, found 2"));
    }

    /// Verifies aliased imported eligible type-alias constructor patterns with
    /// wrong arity fail before CoreIR and report the source alias name.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports `Ok[T] =
    ///   {:ok, value: T}`.
    /// - A temporary consumer `.tl` module that imports `Ok as Success` and
    ///   matches `Success(value, extra)`.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` fails in the
    ///   typecheck phase, skips CoreIR, and reports the constructor-pattern
    ///   arity mismatch for `Success`.
    ///
    /// Transformation:
    /// - Runs command-level check through sibling-interface loading,
    ///   alias-aware import resolution, and typechecking, confirming aliased
    ///   eligible alias patterns report arity failures rather than unresolved
    ///   constructor-pattern metadata.
    #[test]
    fn run_check_single_file_rejects_aliased_imported_alias_constructor_pattern_wrong_arity_before_core_phase(
    ) {
        let dir = make_temp_dir(
            "check_single_file_aliased_imported_alias_constructor_pattern_wrong_arity",
        );
        let provider = dir.join("result.tli");
        fs::write(
            &provider,
            "module result.\n\npub type Ok[T] = {:ok, value: T}.\n",
        )
        .expect("write provider alias constructor interface");

        let source = dir.join("aliased_imported_alias_constructor_pattern_wrong_arity.tl");
        fs::write(
            &source,
            "module aliased_imported_alias_constructor_pattern_wrong_arity.\n\nimport result.{Ok as Success}.\n\npub unwrap(input: Success[Int]): Int ->\n    case input {\n        Success(value, extra) -> value\n    }.\n",
        )
        .expect("write aliased imported alias constructor pattern wrong-arity source");
        let manifest =
            dir.join("aliased_imported_alias_constructor_pattern_wrong_arity.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
        assert!(manifest_text.contains(r#""code":"type_error""#));
        assert!(manifest_text
            .contains("constructor Success has arity mismatch: expected 1..1 args, found 2"));
    }

    /// Verifies eligible type-alias constructor patterns with wrong arity fail
    /// before CoreIR.
    ///
    /// Inputs:
    /// - A temporary single-file Terlan module that declares `Ok[T] =
    ///   {:ok, value: T}` and matches `Ok(value, extra)`.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` fails in the
    ///   typecheck phase, skips CoreIR, and reports the constructor-pattern
    ///   arity mismatch.
    ///
    /// Transformation:
    /// - Runs command-level check through typechecking and confirms eligible
    ///   type-alias constructor patterns report arity errors rather than
    ///   unresolved pattern metadata.
    #[test]
    fn run_check_single_file_rejects_alias_constructor_pattern_wrong_arity_before_core_phase() {
        let dir = make_temp_dir("check_single_file_alias_constructor_pattern_wrong_arity");
        let source = dir.join("alias_constructor_pattern_wrong_arity.tl");
        fs::write(
            &source,
            "module alias_constructor_pattern_wrong_arity.\n\npub type Ok[T] = {:ok, value: T}.\n\npub unwrap(input: Ok[Int]): Int ->\n    case input {\n        Ok(value, extra) -> value\n    }.\n",
        )
        .expect("write alias constructor pattern wrong-arity source");
        let manifest = dir.join("alias_constructor_pattern_wrong_arity.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
        assert!(manifest_text.contains(r#""code":"type_error""#));
        assert!(manifest_text
            .contains("constructor Ok has arity mismatch: expected 1..1 args, found 2"));
    }

    /// Verifies eligible type-alias constructor-chain bases with wrong arity
    /// fail before CoreIR.
    ///
    /// Inputs:
    /// - A temporary single-file Terlan module that declares `User =
    ///   {:user, id: Int, name: Binary}` and uses `User(id)` as a
    ///   constructor-chain base.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` fails in the
    ///   typecheck phase, skips CoreIR, and reports the constructor arity
    ///   mismatch.
    ///
    /// Transformation:
    /// - Runs command-level check through typechecking and confirms chain-base
    ///   arity failures remain typecheck diagnostics rather than unresolved
    ///   constructor-chain metadata.
    #[test]
    fn run_check_single_file_rejects_alias_constructor_chain_wrong_arity_before_core_phase() {
        let dir = make_temp_dir("check_single_file_alias_constructor_chain_wrong_arity");
        let source = dir.join("alias_constructor_chain_wrong_arity.tl");
        fs::write(
            &source,
            "module alias_constructor_chain_wrong_arity.\n\npub type User = {:user, id: Int, name: Binary}.\n\npub value(id: Int): Dynamic ->\n    User(id) with Wrapped { id = id }.\n",
        )
        .expect("write alias constructor chain wrong-arity source");
        let manifest = dir.join("alias_constructor_chain_wrong_arity.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
        assert!(manifest_text.contains(r#""code":"type_error""#));
        assert!(manifest_text
            .contains("constructor User has arity mismatch: expected 2..2 args, found 1"));
    }

    /// Verifies directly imported eligible type-alias constructor-chain bases
    /// with wrong arity fail before CoreIR.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports `User =
    ///   {:user, id: Int, name: Binary}`.
    /// - A temporary consumer `.tl` module that imports `User` and uses
    ///   `User(id)` as a constructor-chain base.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` fails in the
    ///   typecheck phase, skips CoreIR, and reports the imported constructor
    ///   arity mismatch.
    ///
    /// Transformation:
    /// - Runs command-level check through sibling-interface loading and
    ///   typechecking, confirming imported single-shape alias chain-base arity
    ///   failures remain typecheck diagnostics rather than unresolved
    ///   constructor-chain metadata.
    #[test]
    fn run_check_single_file_rejects_imported_alias_constructor_chain_wrong_arity_before_core_phase(
    ) {
        let dir = make_temp_dir("check_single_file_imported_alias_constructor_chain_wrong_arity");
        let provider = dir.join("result.tli");
        fs::write(
            &provider,
            "module result.\n\npub type User = {:user, id: Int, name: Binary}.\n",
        )
        .expect("write provider alias constructor-chain interface");

        let source = dir.join("imported_alias_constructor_chain_wrong_arity.tl");
        fs::write(
            &source,
            "module imported_alias_constructor_chain_wrong_arity.\n\nimport result.{User}.\n\npub value(id: Int): Dynamic ->\n    User(id) with Wrapped { id = id }.\n",
        )
        .expect("write imported alias constructor chain wrong-arity source");
        let manifest = dir.join("imported_alias_constructor_chain_wrong_arity.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
        assert!(manifest_text.contains(r#""code":"type_error""#));
        assert!(manifest_text
            .contains("constructor User has arity mismatch: expected 2..2 args, found 1"));
    }

    /// Verifies aliased imported eligible type-alias constructor-chain bases
    /// with wrong arity fail before CoreIR and report the source alias name.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports `User =
    ///   {:user, id: Int, name: Binary}`.
    /// - A temporary consumer `.tl` module that imports `User as Member` and
    ///   uses `Member(id)` as a constructor-chain base.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` fails in the
    ///   typecheck phase, skips CoreIR, and reports the constructor arity
    ///   mismatch for `Member`.
    ///
    /// Transformation:
    /// - Runs command-level check through sibling-interface loading,
    ///   alias-aware import resolution, and typechecking, confirming aliased
    ///   eligible alias chain-base arity failures remain typecheck diagnostics
    ///   rather than unresolved constructor-chain metadata.
    #[test]
    fn run_check_single_file_rejects_aliased_imported_alias_constructor_chain_wrong_arity_before_core_phase(
    ) {
        let dir =
            make_temp_dir("check_single_file_aliased_imported_alias_constructor_chain_wrong_arity");
        let provider = dir.join("result.tli");
        fs::write(
            &provider,
            "module result.\n\npub type User = {:user, id: Int, name: Binary}.\n",
        )
        .expect("write provider alias constructor-chain interface");

        let source = dir.join("aliased_imported_alias_constructor_chain_wrong_arity.tl");
        fs::write(
            &source,
            "module aliased_imported_alias_constructor_chain_wrong_arity.\n\nimport result.{User as Member}.\n\npub value(id: Int): Dynamic ->\n    Member(id) with Wrapped { id = id }.\n",
        )
        .expect("write aliased imported alias constructor chain wrong-arity source");
        let manifest =
            dir.join("aliased_imported_alias_constructor_chain_wrong_arity.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
        assert!(manifest_text.contains(r#""code":"type_error""#));
        assert!(manifest_text
            .contains("constructor Member has arity mismatch: expected 2..2 args, found 1"));
    }

    /// Verifies imported list aliases cannot become constructor-chain bases.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports public list alias
    ///   `Items`.
    /// - A temporary consumer `.tl` module that imports `Items` and attempts to
    ///   use `Items(values)` as a constructor-chain base.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` fails in the
    ///   typecheck phase, skips CoreIR, and reports `unknown constructor
    ///   Items / 1`.
    ///
    /// Transformation:
    /// - Runs command-level check through sibling-interface loading and
    ///   typechecking, proving non-eligible imported aliases are rejected
    ///   before CoreIR identity annotation can run.
    #[test]
    fn run_check_single_file_rejects_imported_list_alias_constructor_chain_before_core_phase() {
        let dir = make_temp_dir("check_single_file_imported_list_alias_constructor_chain");
        let provider = dir.join("items.tli");
        fs::write(&provider, "module items.\n\npub type Items[T] = List[T].\n")
            .expect("write provider list alias interface");

        let source = dir.join("imported_list_alias_constructor_chain.tl");
        fs::write(
            &source,
            "module imported_list_alias_constructor_chain.\n\nimport items.{Items}.\n\npub value(values: List[Int]): Dynamic ->\n    Items(values) with Wrapped { values = values }.\n",
        )
        .expect("write imported list alias constructor-chain source");
        let manifest = dir.join("imported_list_alias_constructor_chain.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
        assert!(manifest_text.contains(r#""code":"type_error""#));
        assert!(manifest_text.contains("unknown constructor Items / 1"));
    }

    /// Verifies imported list aliases cannot become constructor calls.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports public list alias
    ///   `Items`.
    /// - A temporary consumer `.tl` module that imports `Items` and attempts
    ///   to call `Items(values)`.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` fails in the
    ///   typecheck phase, skips CoreIR, and reports `unknown constructor
    ///   Items / 1`.
    ///
    /// Transformation:
    /// - Runs command-level check through sibling-interface loading and
    ///   typechecking, proving non-eligible imported aliases are rejected
    ///   before CoreIR constructor-call identity annotation can run.
    #[test]
    fn run_check_single_file_rejects_imported_list_alias_constructor_call_before_core_phase() {
        let dir = make_temp_dir("check_single_file_imported_list_alias_constructor_call");
        let provider = dir.join("items.tli");
        fs::write(&provider, "module items.\n\npub type Items[T] = List[T].\n")
            .expect("write provider list alias interface");

        let source = dir.join("imported_list_alias_constructor_call.tl");
        fs::write(
            &source,
            "module imported_list_alias_constructor_call.\n\nimport items.{Items}.\n\npub value(values: List[Int]): Items[Int] ->\n    Items(values).\n",
        )
        .expect("write imported list alias constructor-call source");
        let manifest = dir.join("imported_list_alias_constructor_call.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
        assert!(manifest_text.contains(r#""code":"type_error""#));
        assert!(manifest_text.contains("unknown constructor Items / 1"));
    }

    /// Verifies aliased imported list aliases cannot become
    /// constructor-chain bases.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports public list alias
    ///   `Items`.
    /// - A temporary consumer `.tl` module that imports `Items as Bag` and
    ///   attempts to use `Bag(values)` as a constructor-chain base.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` fails in the
    ///   typecheck phase, skips CoreIR, and reports `unknown constructor
    ///   Bag / 1`.
    ///
    /// Transformation:
    /// - Runs command-level check through sibling-interface loading,
    ///   alias-aware import resolution, and typechecking, proving non-eligible
    ///   imported aliases are rejected before CoreIR identity annotation can
    ///   run under aliased names.
    #[test]
    fn run_check_single_file_rejects_aliased_imported_list_alias_constructor_chain_before_core_phase(
    ) {
        let dir = make_temp_dir("check_single_file_aliased_imported_list_alias_constructor_chain");
        let provider = dir.join("items.tli");
        fs::write(&provider, "module items.\n\npub type Items[T] = List[T].\n")
            .expect("write provider list alias interface");

        let source = dir.join("aliased_imported_list_alias_constructor_chain.tl");
        fs::write(
            &source,
            "module aliased_imported_list_alias_constructor_chain.\n\nimport items.{Items as Bag}.\n\npub value(values: List[Int]): Dynamic ->\n    Bag(values) with Wrapped { values = values }.\n",
        )
        .expect("write aliased imported list alias constructor-chain source");
        let manifest =
            dir.join("aliased_imported_list_alias_constructor_chain.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
        assert!(manifest_text.contains(r#""code":"type_error""#));
        assert!(manifest_text.contains("unknown constructor Bag / 1"));
    }

    /// Verifies aliased imported list aliases cannot become constructor calls.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports public list alias
    ///   `Items`.
    /// - A temporary consumer `.tl` module that imports `Items as Bag` and
    ///   attempts to call `Bag(values)`.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` fails in the
    ///   typecheck phase, skips CoreIR, and reports `unknown constructor
    ///   Bag / 1`.
    ///
    /// Transformation:
    /// - Runs command-level check through sibling-interface loading,
    ///   alias-aware import resolution, and typechecking, proving non-eligible
    ///   imported aliases are rejected before CoreIR constructor-call identity
    ///   annotation can run under aliased names.
    #[test]
    fn run_check_single_file_rejects_aliased_imported_list_alias_constructor_call_before_core_phase(
    ) {
        let dir = make_temp_dir("check_single_file_aliased_imported_list_alias_constructor_call");
        let provider = dir.join("items.tli");
        fs::write(&provider, "module items.\n\npub type Items[T] = List[T].\n")
            .expect("write provider list alias interface");

        let source = dir.join("aliased_imported_list_alias_constructor_call.tl");
        fs::write(
            &source,
            "module aliased_imported_list_alias_constructor_call.\n\nimport items.{Items as Bag}.\n\npub value(values: List[Int]): Bag[Int] ->\n    Bag(values).\n",
        )
        .expect("write aliased imported list alias constructor-call source");
        let manifest = dir.join("aliased_imported_list_alias_constructor_call.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
        assert!(manifest_text.contains(r#""code":"type_error""#));
        assert!(manifest_text.contains("unknown constructor Bag / 1"));
    }

    /// Verifies imported list aliases cannot become constructor patterns.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports public list alias
    ///   `Items`.
    /// - A temporary consumer `.tl` module that imports `Items` and attempts
    ///   to match `Items(values)`.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` fails in the
    ///   typecheck phase, skips CoreIR, and reports `unknown constructor
    ///   pattern Items`.
    ///
    /// Transformation:
    /// - Runs command-level check through sibling-interface loading and
    ///   typechecking, proving non-eligible imported aliases are rejected
    ///   before CoreIR constructor-pattern identity annotation can run.
    #[test]
    fn run_check_single_file_rejects_imported_list_alias_constructor_pattern_before_core_phase() {
        let dir = make_temp_dir("check_single_file_imported_list_alias_constructor_pattern");
        let provider = dir.join("items.tli");
        fs::write(&provider, "module items.\n\npub type Items[T] = List[T].\n")
            .expect("write provider list alias interface");

        let source = dir.join("imported_list_alias_constructor_pattern.tl");
        fs::write(
            &source,
            "module imported_list_alias_constructor_pattern.\n\nimport items.{Items}.\n\npub unwrap(input: Items[Int]): List[Int] ->\n    case input {\n        Items(values) -> values\n    }.\n",
        )
        .expect("write imported list alias constructor-pattern source");
        let manifest = dir.join("imported_list_alias_constructor_pattern.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
        assert!(manifest_text.contains(r#""code":"type_error""#));
        assert!(manifest_text.contains("unknown constructor pattern Items"));
    }

    /// Verifies aliased imported list aliases cannot become constructor
    /// patterns.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports public list alias
    ///   `Items`.
    /// - A temporary consumer `.tl` module that imports `Items as Bag` and
    ///   attempts to match `Bag(values)`.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` fails in the
    ///   typecheck phase, skips CoreIR, and reports `unknown constructor
    ///   pattern Bag`.
    ///
    /// Transformation:
    /// - Runs command-level check through sibling-interface loading,
    ///   alias-aware import resolution, and typechecking, proving non-eligible
    ///   imported aliases are rejected before CoreIR constructor-pattern
    ///   identity annotation can run under aliased names.
    #[test]
    fn run_check_single_file_rejects_aliased_imported_list_alias_constructor_pattern_before_core_phase(
    ) {
        let dir =
            make_temp_dir("check_single_file_aliased_imported_list_alias_constructor_pattern");
        let provider = dir.join("items.tli");
        fs::write(&provider, "module items.\n\npub type Items[T] = List[T].\n")
            .expect("write provider list alias interface");

        let source = dir.join("aliased_imported_list_alias_constructor_pattern.tl");
        fs::write(
            &source,
            "module aliased_imported_list_alias_constructor_pattern.\n\nimport items.{Items as Bag}.\n\npub unwrap(input: Bag[Int]): List[Int] ->\n    case input {\n        Bag(values) -> values\n    }.\n",
        )
        .expect("write aliased imported list alias constructor-pattern source");
        let manifest =
            dir.join("aliased_imported_list_alias_constructor_pattern.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
        assert!(manifest_text.contains(r#""code":"type_error""#));
        assert!(manifest_text.contains("unknown constructor pattern Bag"));
    }

    #[test]
    fn run_check_single_file_accepts_declared_constructor_call_in_core_phase() {
        let dir = make_temp_dir("check_single_file_declared_constructor_call");
        let source = dir.join("constructor_call.tl");
        fs::write(
            &source,
            "module constructor_call.\n\npub constructor Ok {\n    (value: Int): Dynamic -> value\n}.\n\npub value(): Dynamic ->\n    Ok(1).\n",
        )
        .expect("write declared constructor call source");
        let manifest = dir.join("constructor_call.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
                .as_u64()
                .expect("resolved constructor-call identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_call_candidate"]
                .as_u64()
                .expect("unresolved constructor-call candidate count"),
            0
        );
    }

    /// Verifies local type-alias constructor-call manifests carry resolved
    /// identity.
    ///
    /// Inputs:
    /// - A temporary `.tl` source file declaring a single-shape `Ok[T]` type
    ///   alias and calling `Ok(1)`.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   CoreIR proof coverage reports one resolved constructor-call identity
    ///   with no unresolved constructor-call candidates.
    ///
    /// Transformation:
    /// - Runs the command-level check path through typechecking, CoreIR
    ///   type-alias constructor identity annotation, and phase-manifest
    ///   emission.
    #[test]
    fn run_check_single_file_accepts_alias_constructor_call_in_core_phase() {
        let dir = make_temp_dir("check_single_file_alias_constructor_call");
        let source = dir.join("alias_constructor_call.tl");
        fs::write(
            &source,
            "module alias_constructor_call.\n\npub type Ok[T] = {:ok, value: T}.\n\npub value(): Dynamic ->\n    Ok(1).\n",
        )
        .expect("write alias constructor call source");
        let manifest = dir.join("alias_constructor_call.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
                .as_u64()
                .expect("resolved constructor-call identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_call_candidate"]
                .as_u64()
                .expect("unresolved constructor-call candidate count"),
            0
        );
    }

    /// Verifies imported constructor-call manifests carry provider-qualified identity.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports public constructor
    ///   `Ok`.
    /// - A temporary consumer `.tl` module that imports `Ok` and calls it.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   CoreIR proof coverage reports one resolved constructor-call identity
    ///   with no unresolved constructor-call candidates.
    ///
    /// Transformation:
    /// - Runs the command-level check path through sibling-interface loading,
    ///   typechecking, CoreIR lowering, and phase-manifest emission so imported
    ///   constructor identity resolution is pinned outside unit-only lowering.
    #[test]
    fn run_check_single_file_accepts_imported_constructor_call_in_core_phase() {
        let dir = make_temp_dir("check_single_file_imported_constructor_call");
        let provider = dir.join("provider.tli");
        fs::write(
            &provider,
            "module provider.\n\npub constructor Ok {\n    (value: Int): Dynamic -> value\n}.\n",
        )
        .expect("write provider constructor interface");

        let source = dir.join("imported_constructor_call.tl");
        fs::write(
            &source,
            "module imported_constructor_call.\n\nimport provider.{Ok}.\n\npub value(): Dynamic ->\n    Ok(1).\n",
        )
        .expect("write imported constructor call source");
        let manifest = dir.join("imported_constructor_call.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
                .as_u64()
                .expect("resolved constructor-call identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_call_candidate"]
                .as_u64()
                .expect("unresolved constructor-call candidate count"),
            0
        );
    }

    /// Verifies aliased imported constructor-call manifests carry resolved identity.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports public constructor
    ///   `Ok`.
    /// - A temporary consumer `.tl` module that imports `Ok as Success` and
    ///   calls `Success`.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   CoreIR proof coverage reports one resolved constructor-call identity
    ///   with no unresolved constructor-call candidates.
    ///
    /// Transformation:
    /// - Runs the command-level check path through sibling-interface loading,
    ///   alias-aware typechecking, CoreIR lowering, and phase-manifest emission
    ///   so aliased imported constructor identity resolution is pinned outside
    ///   unit-only lowering.
    #[test]
    fn run_check_single_file_accepts_aliased_imported_constructor_call_in_core_phase() {
        let dir = make_temp_dir("check_single_file_aliased_imported_constructor_call");
        let provider = dir.join("provider.tli");
        fs::write(
            &provider,
            "module provider.\n\npub constructor Ok {\n    (value: Int): Dynamic -> value\n}.\n",
        )
        .expect("write provider constructor interface");

        let source = dir.join("aliased_imported_constructor_call.tl");
        fs::write(
            &source,
            "module aliased_imported_constructor_call.\n\nimport provider.{Ok as Success}.\n\npub value(): Dynamic ->\n    Success(1).\n",
        )
        .expect("write aliased imported constructor call source");
        let manifest = dir.join("aliased_imported_constructor_call.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
                .as_u64()
                .expect("resolved constructor-call identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_call_candidate"]
                .as_u64()
                .expect("unresolved constructor-call candidate count"),
            0
        );
    }

    /// Verifies directly imported type-alias constructor-call manifests carry
    /// provider-qualified identity.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports public single-shape
    ///   type alias `Ok`.
    /// - A temporary consumer `.tl` module that imports `Ok` directly and calls
    ///   `Ok`.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   CoreIR proof coverage reports one resolved constructor-call identity
    ///   with no unresolved constructor-call candidates.
    ///
    /// Transformation:
    /// - Runs command-level check through sibling-interface loading,
    ///   direct-import typechecking, CoreIR type-alias constructor identity
    ///   annotation, and phase-manifest emission.
    #[test]
    fn run_check_single_file_accepts_direct_imported_alias_constructor_call_in_core_phase() {
        let dir = make_temp_dir("check_single_file_direct_imported_alias_constructor_call");
        let provider = dir.join("provider.tli");
        fs::write(
            &provider,
            "module provider.\n\npub type Ok[T] = {:ok, value: T}.\n",
        )
        .expect("write provider alias constructor interface");

        let source = dir.join("direct_imported_alias_constructor_call.tl");
        fs::write(
            &source,
            "module direct_imported_alias_constructor_call.\n\nimport provider.{Ok}.\n\npub value(): Dynamic ->\n    Ok(1).\n",
        )
        .expect("write direct imported alias constructor call source");
        let manifest = dir.join("direct_imported_alias_constructor_call.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
                .as_u64()
                .expect("resolved constructor-call identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_call_candidate"]
                .as_u64()
                .expect("unresolved constructor-call candidate count"),
            0
        );
    }

    /// Verifies imported type-alias constructor-call manifests carry
    /// provider-qualified identity.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports public single-shape
    ///   type alias `Ok`.
    /// - A temporary consumer `.tl` module that imports `Ok as Success` and
    ///   calls `Success`.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   CoreIR proof coverage reports one resolved constructor-call identity
    ///   with no unresolved constructor-call candidates.
    ///
    /// Transformation:
    /// - Runs the command-level check path through sibling-interface loading,
    ///   alias-aware typechecking, CoreIR type-alias constructor identity
    ///   annotation, and phase-manifest emission.
    #[test]
    fn run_check_single_file_accepts_aliased_imported_alias_constructor_call_in_core_phase() {
        let dir = make_temp_dir("check_single_file_aliased_imported_alias_constructor_call");
        let provider = dir.join("provider.tli");
        fs::write(
            &provider,
            "module provider.\n\npub type Ok[T] = {:ok, value: T}.\n",
        )
        .expect("write provider alias constructor interface");

        let source = dir.join("aliased_imported_alias_constructor_call.tl");
        fs::write(
            &source,
            "module aliased_imported_alias_constructor_call.\n\nimport provider.{Ok as Success}.\n\npub value(): Dynamic ->\n    Success(1).\n",
        )
        .expect("write aliased imported alias constructor call source");
        let manifest = dir.join("aliased_imported_alias_constructor_call.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
                .as_u64()
                .expect("resolved constructor-call identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_call_candidate"]
                .as_u64()
                .expect("unresolved constructor-call candidate count"),
            0
        );
    }

    /// Verifies imported constructor-pattern manifests carry provider-qualified identity.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports public constructor
    ///   `Some`.
    /// - A temporary consumer `.tl` module that imports `Some` and matches it
    ///   in a `case` expression.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   CoreIR proof coverage reports one resolved constructor-pattern
    ///   identity with no unresolved constructor-pattern candidates.
    ///
    /// Transformation:
    /// - Runs the command-level check path through sibling-interface loading,
    ///   typechecking, CoreIR pattern lowering, and phase-manifest emission so
    ///   imported pattern identity resolution is pinned outside unit-only
    ///   lowering.
    #[test]
    fn run_check_single_file_accepts_imported_constructor_pattern_in_core_phase() {
        let dir = make_temp_dir("check_single_file_imported_constructor_pattern");
        let provider = dir.join("provider.tli");
        fs::write(
            &provider,
            "module provider.\n\npub constructor Some {\n    (value: Dynamic): Dynamic -> {:some, value}\n}.\n",
        )
        .expect("write provider constructor interface");

        let source = dir.join("imported_constructor_pattern.tl");
        fs::write(
            &source,
            "module imported_constructor_pattern.\n\nimport provider.{Some}.\n\npub unwrap(input: Dynamic): Dynamic ->\n    case input {\n        Some(value) -> value\n    }.\n",
        )
        .expect("write imported constructor pattern source");
        let manifest = dir.join("imported_constructor_pattern.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_pattern_identity"]
                .as_u64()
                .expect("resolved constructor-pattern identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_pattern_candidate"]
                .as_u64()
                .expect("unresolved constructor-pattern candidate count"),
            0
        );
    }

    /// Verifies aliased imported constructor-pattern manifests carry resolved identity.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports public constructor
    ///   `Some`.
    /// - A temporary consumer `.tl` module that imports `Some as Maybe` and
    ///   matches `Maybe(value)` in a `case` expression.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   CoreIR proof coverage reports one resolved constructor-pattern
    ///   identity with no unresolved constructor-pattern candidates.
    ///
    /// Transformation:
    /// - Runs the command-level check path through sibling-interface loading,
    ///   alias-aware typechecking, CoreIR pattern lowering, and
    ///   phase-manifest emission so aliased imported constructor-pattern
    ///   identity resolution is pinned outside unit-only lowering.
    #[test]
    fn run_check_single_file_accepts_aliased_imported_constructor_pattern_in_core_phase() {
        let dir = make_temp_dir("check_single_file_aliased_imported_constructor_pattern");
        let provider = dir.join("provider.tli");
        fs::write(
            &provider,
            "module provider.\n\npub constructor Some {\n    (value: Dynamic): Dynamic -> {:some, value}\n}.\n",
        )
        .expect("write provider constructor interface");

        let source = dir.join("aliased_imported_constructor_pattern.tl");
        fs::write(
            &source,
            "module aliased_imported_constructor_pattern.\n\nimport provider.{Some as Maybe}.\n\npub unwrap(input: Dynamic): Dynamic ->\n    case input {\n        Maybe(value) -> value\n    }.\n",
        )
        .expect("write aliased imported constructor pattern source");
        let manifest = dir.join("aliased_imported_constructor_pattern.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_pattern_identity"]
                .as_u64()
                .expect("resolved constructor-pattern identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_pattern_candidate"]
                .as_u64()
                .expect("unresolved constructor-pattern candidate count"),
            0
        );
    }

    /// Verifies directly imported type-alias constructor-pattern manifests
    /// carry provider-qualified identity.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports public single-shape
    ///   type alias `Ok`.
    /// - A temporary consumer `.tl` module that imports `Ok` directly and
    ///   matches `Ok(value)`.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   CoreIR proof coverage reports one resolved constructor-pattern
    ///   identity with no unresolved constructor-pattern candidates.
    ///
    /// Transformation:
    /// - Runs command-level check through sibling-interface loading,
    ///   direct-import typechecking, CoreIR type-alias constructor-pattern
    ///   identity annotation, and phase-manifest emission.
    #[test]
    fn run_check_single_file_accepts_direct_imported_alias_constructor_pattern_in_core_phase() {
        let dir = make_temp_dir("check_single_file_direct_imported_alias_constructor_pattern");
        let provider = dir.join("provider.tli");
        fs::write(
            &provider,
            "module provider.\n\npub type Ok[T] = {:ok, value: T}.\n",
        )
        .expect("write provider alias constructor interface");

        let source = dir.join("direct_imported_alias_constructor_pattern.tl");
        fs::write(
            &source,
            "module direct_imported_alias_constructor_pattern.\n\nimport provider.{Ok}.\n\npub unwrap(input: Ok[Int]): Int ->\n    case input {\n        Ok(value) -> value\n    }.\n",
        )
        .expect("write direct imported alias constructor pattern source");
        let manifest = dir.join("direct_imported_alias_constructor_pattern.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_pattern_identity"]
                .as_u64()
                .expect("resolved constructor-pattern identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_pattern_candidate"]
                .as_u64()
                .expect("unresolved constructor-pattern candidate count"),
            0
        );
    }

    /// Verifies imported type-alias constructor-pattern manifests carry
    /// provider-qualified identity.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports public single-shape
    ///   type alias `Ok`.
    /// - A temporary consumer `.tl` module that imports `Ok as Success` and
    ///   matches `Success(value)`.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   CoreIR proof coverage reports one resolved constructor-pattern
    ///   identity with no unresolved constructor-pattern candidates.
    ///
    /// Transformation:
    /// - Runs command-level check through sibling-interface loading,
    ///   alias-aware typechecking, CoreIR type-alias constructor-pattern
    ///   identity annotation, and phase-manifest emission.
    #[test]
    fn run_check_single_file_accepts_aliased_imported_alias_constructor_pattern_in_core_phase() {
        let dir = make_temp_dir("check_single_file_aliased_imported_alias_constructor_pattern");
        let provider = dir.join("provider.tli");
        fs::write(
            &provider,
            "module provider.\n\npub type Ok[T] = {:ok, value: T}.\n",
        )
        .expect("write provider alias constructor interface");

        let source = dir.join("aliased_imported_alias_constructor_pattern.tl");
        fs::write(
            &source,
            "module aliased_imported_alias_constructor_pattern.\n\nimport provider.{Ok as Success}.\n\npub unwrap(input: Success[Int]): Int ->\n    case input {\n        Success(value) -> value\n    }.\n",
        )
        .expect("write aliased imported alias constructor pattern source");
        let manifest = dir.join("aliased_imported_alias_constructor_pattern.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_pattern_identity"]
                .as_u64()
                .expect("resolved constructor-pattern identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_pattern_candidate"]
                .as_u64()
                .expect("unresolved constructor-pattern candidate count"),
            0
        );
    }

    /// Verifies constructor-pattern manifests carry identity and freshness debt.
    ///
    /// Inputs:
    /// - A temporary single-file Terlan module with a declared constructor and
    ///   a `case` expression that binds through that constructor pattern.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds, records
    ///   one resolved constructor-pattern identity, and exposes runtime-binding
    ///   freshness obligations for both the selected case body and pattern.
    ///
    /// Transformation:
    /// - Runs the CLI check command through the formal pipeline and validates
    ///   the manifest fields that future Lean proof export will consume.
    #[test]
    fn run_check_single_file_accepts_declared_constructor_pattern_in_core_phase() {
        let dir = make_temp_dir("check_single_file_declared_constructor_pattern");
        let source = dir.join("constructor_pattern.tl");
        fs::write(
            &source,
            "module constructor_pattern.\n\npub constructor Some {\n    (value: Dynamic): Dynamic -> {:some, value}\n}.\n\npub unwrap(input: Dynamic): Dynamic ->\n    case input {\n        Some(value) -> value\n    }.\n",
        )
        .expect("write declared constructor pattern source");
        let manifest = dir.join("constructor_pattern.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_pattern_identity"]
                .as_u64()
                .expect("resolved constructor-pattern identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_pattern_candidate"]
                .as_u64()
                .expect("unresolved constructor-pattern candidate count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["checked_preservation_expr_no_runtime_bindings"]
                .as_u64()
                .expect("no-runtime-bindings expression count"),
            2
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]
                ["checked_preservation_expr_runtime_bindings_required"]
                .as_u64()
                .expect("runtime-bindings-required expression count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]
                ["checked_preservation_pattern_no_runtime_bindings"]
                .as_u64()
                .expect("no-runtime-bindings pattern count"),
            0
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]
                ["checked_preservation_pattern_runtime_bindings_required"]
                .as_u64()
                .expect("runtime-bindings-required pattern count"),
            1
        );
    }

    /// Verifies local type-alias constructor-pattern manifests carry resolved
    /// identity.
    ///
    /// Inputs:
    /// - A temporary `.tl` source file declaring a single-shape `Ok[T]` type
    ///   alias and matching `Ok(value)` in a case expression.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   CoreIR proof coverage reports one resolved constructor-pattern
    ///   identity with no unresolved constructor-pattern candidates.
    ///
    /// Transformation:
    /// - Runs command-level check through typechecking, CoreIR type-alias
    ///   constructor-pattern identity annotation, and phase-manifest emission.
    #[test]
    fn run_check_single_file_accepts_alias_constructor_pattern_in_core_phase() {
        let dir = make_temp_dir("check_single_file_alias_constructor_pattern");
        let source = dir.join("alias_constructor_pattern.tl");
        fs::write(
            &source,
            "module alias_constructor_pattern.\n\npub type Ok[T] = {:ok, value: T}.\n\npub unwrap(input: Ok[Int]): Int ->\n    case input {\n        Ok(value) -> value\n    }.\n",
        )
        .expect("write alias constructor pattern source");
        let manifest = dir.join("alias_constructor_pattern.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_pattern_identity"]
                .as_u64()
                .expect("resolved constructor-pattern identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_pattern_candidate"]
                .as_u64()
                .expect("unresolved constructor-pattern candidate count"),
            0
        );
    }

    /// Verifies unresolved local constructor patterns fail before CoreIR.
    ///
    /// Inputs:
    /// - A temporary single-file Terlan module that matches `Missing(value)`
    ///   without declaring or importing `Missing`.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` fails in the
    ///   typecheck phase, skips CoreIR, and records the unknown
    ///   constructor-pattern diagnostic in the emitted phase manifest.
    ///
    /// Transformation:
    /// - Runs command-level check through syntax-output parsing, HIR
    ///   resolution, and typechecking so the formal phase manifest proves
    ///   unresolved constructor-pattern sugar cannot reach CoreIR lowering.
    #[test]
    fn run_check_single_file_rejects_local_unknown_constructor_pattern_before_core_phase() {
        let dir = make_temp_dir("check_single_file_local_unknown_constructor_pattern");
        let source = dir.join("local_unknown_constructor_pattern.tl");
        fs::write(
            &source,
            "module local_unknown_constructor_pattern.\n\npub unwrap(input: Dynamic): Dynamic ->\n    case input {\n        Missing(value) -> value\n    }.\n",
        )
        .expect("write local unknown constructor-pattern source");
        let manifest = dir.join("local_unknown_constructor_pattern.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::from(1));

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"error""#));
        assert!(manifest_text.contains(r#""name":"core","status":"skipped""#));
        assert!(manifest_text.contains(r#""code":"type_error""#));
        assert!(manifest_text.contains("unknown constructor pattern Missing"));
    }

    /// Verifies local constructor-chain manifests carry resolved base identity.
    ///
    /// Inputs:
    /// - A temporary single-file Terlan module with a declared constructor
    ///   `User` and a constructor-chain expression that extends its result.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   CoreIR proof coverage reports one resolved constructor-call identity,
    ///   one resolved constructor-chain identity, and no unresolved chain
    ///   candidates.
    ///
    /// Transformation:
    /// - Runs the CLI check command through local constructor typechecking,
    ///   CoreIR constructor-chain lowering, and manifest emission.
    #[test]
    fn run_check_single_file_accepts_declared_constructor_chain_in_core_phase() {
        let dir = make_temp_dir("check_single_file_declared_constructor_chain");
        let source = dir.join("constructor_chain.tl");
        fs::write(
            &source,
            "module constructor_chain.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n\npub value(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id = id, name = name }.\n",
        )
        .expect("write declared constructor chain source");
        let manifest = dir.join("constructor_chain.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
                .as_u64()
                .expect("resolved constructor-call identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_chain_identity"]
                .as_u64()
                .expect("resolved constructor-chain identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_chain_candidate"]
                .as_u64()
                .expect("unresolved constructor-chain candidate count"),
            0
        );
    }

    /// Verifies local type-alias constructor-chain manifests carry resolved
    /// base identity.
    ///
    /// Inputs:
    /// - A temporary single-file Terlan module with a single-shape `User`
    ///   type alias and a constructor-chain expression that extends it.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   CoreIR proof coverage reports one resolved constructor-call identity,
    ///   one resolved constructor-chain identity, and no unresolved chain
    ///   candidates.
    ///
    /// Transformation:
    /// - Runs the CLI check command through type-alias constructor
    ///   typechecking, CoreIR constructor-chain identity annotation, and
    ///   manifest emission.
    #[test]
    fn run_check_single_file_accepts_alias_constructor_chain_in_core_phase() {
        let dir = make_temp_dir("check_single_file_alias_constructor_chain");
        let source = dir.join("alias_constructor_chain.tl");
        fs::write(
            &source,
            "module alias_constructor_chain.\n\npub type User = {:user, id: Int, name: Binary}.\n\npub value(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id = id, name = name }.\n",
        )
        .expect("write alias constructor chain source");
        let manifest = dir.join("alias_constructor_chain.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
                .as_u64()
                .expect("resolved constructor-call identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_chain_identity"]
                .as_u64()
                .expect("resolved constructor-chain identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_chain_candidate"]
                .as_u64()
                .expect("unresolved constructor-chain candidate count"),
            0
        );
    }

    /// Verifies imported constructor-chain manifests carry provider-qualified identity.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports public constructor
    ///   `User`.
    /// - A temporary consumer `.tl` module that imports `User` and uses it as
    ///   the base call in a constructor-chain expression.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   CoreIR proof coverage reports one resolved constructor-call identity,
    ///   one resolved constructor-chain identity, and no unresolved chain
    ///   candidates.
    ///
    /// Transformation:
    /// - Runs the command-level check path through sibling-interface loading,
    ///   imported constructor typechecking, CoreIR constructor-chain lowering,
    ///   and phase-manifest emission.
    #[test]
    fn run_check_single_file_accepts_imported_constructor_chain_in_core_phase() {
        let dir = make_temp_dir("check_single_file_imported_constructor_chain");
        let provider = dir.join("provider.tli");
        fs::write(
            &provider,
            "module provider.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n",
        )
        .expect("write provider constructor interface");

        let source = dir.join("imported_constructor_chain.tl");
        fs::write(
            &source,
            "module imported_constructor_chain.\n\nimport provider.{User}.\n\npub value(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id = id, name = name }.\n",
        )
        .expect("write imported constructor chain source");
        let manifest = dir.join("imported_constructor_chain.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
                .as_u64()
                .expect("resolved constructor-call identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_chain_identity"]
                .as_u64()
                .expect("resolved constructor-chain identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_chain_candidate"]
                .as_u64()
                .expect("unresolved constructor-chain candidate count"),
            0
        );
    }

    /// Verifies aliased imported constructor-chain manifests carry resolved identity.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports public constructor
    ///   `User`.
    /// - A temporary consumer `.tl` module that imports `User as Member` and
    ///   uses `Member` as the base call in a constructor-chain expression.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   CoreIR proof coverage reports one resolved constructor-call identity,
    ///   one resolved constructor-chain identity, and no unresolved chain
    ///   candidates.
    ///
    /// Transformation:
    /// - Runs the command-level check path through sibling-interface loading,
    ///   alias-aware constructor typechecking, CoreIR constructor-chain
    ///   lowering, and phase-manifest emission so aliased imported
    ///   constructor-chain identity resolution is pinned outside unit-only
    ///   lowering.
    #[test]
    fn run_check_single_file_accepts_aliased_imported_constructor_chain_in_core_phase() {
        let dir = make_temp_dir("check_single_file_aliased_imported_constructor_chain");
        let provider = dir.join("provider.tli");
        fs::write(
            &provider,
            "module provider.\n\npub constructor User {\n    (id: Int, name: Binary): Dynamic -> id\n}.\n",
        )
        .expect("write provider constructor interface");

        let source = dir.join("aliased_imported_constructor_chain.tl");
        fs::write(
            &source,
            "module aliased_imported_constructor_chain.\n\nimport provider.{User as Member}.\n\npub value(id: Int, name: Binary): Dynamic ->\n    Member(id, name) with Admin { id = id, name = name }.\n",
        )
        .expect("write aliased imported constructor chain source");
        let manifest = dir.join("aliased_imported_constructor_chain.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
                .as_u64()
                .expect("resolved constructor-call identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_chain_identity"]
                .as_u64()
                .expect("resolved constructor-chain identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_chain_candidate"]
                .as_u64()
                .expect("unresolved constructor-chain candidate count"),
            0
        );
    }

    /// Verifies directly imported type-alias constructor-chain manifests carry
    /// provider-qualified identity.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports public single-shape
    ///   type alias `User`.
    /// - A temporary consumer `.tl` module that imports `User` directly and
    ///   uses `User` as the base call in a constructor-chain expression.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   CoreIR proof coverage reports one resolved constructor-call identity,
    ///   one resolved constructor-chain identity, and no unresolved chain
    ///   candidates.
    ///
    /// Transformation:
    /// - Runs command-level check through sibling-interface loading,
    ///   direct-import typechecking, CoreIR type-alias constructor-chain
    ///   identity annotation, and phase-manifest emission.
    #[test]
    fn run_check_single_file_accepts_direct_imported_alias_constructor_chain_in_core_phase() {
        let dir = make_temp_dir("check_single_file_direct_imported_alias_constructor_chain");
        let provider = dir.join("provider.tli");
        fs::write(
            &provider,
            "module provider.\n\npub type User = {:user, id: Int, name: Binary}.\n",
        )
        .expect("write provider alias constructor-chain interface");

        let source = dir.join("direct_imported_alias_constructor_chain.tl");
        fs::write(
            &source,
            "module direct_imported_alias_constructor_chain.\n\nimport provider.{User}.\n\npub value(id: Int, name: Binary): Dynamic ->\n    User(id, name) with Admin { id = id, name = name }.\n",
        )
        .expect("write direct imported alias constructor chain source");
        let manifest = dir.join("direct_imported_alias_constructor_chain.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
                .as_u64()
                .expect("resolved constructor-call identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_chain_identity"]
                .as_u64()
                .expect("resolved constructor-chain identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_chain_candidate"]
                .as_u64()
                .expect("unresolved constructor-chain candidate count"),
            0
        );
    }

    /// Verifies imported type-alias constructor-chain manifests carry
    /// provider-qualified identity.
    ///
    /// Inputs:
    /// - A temporary provider `.tli` interface that exports public single-shape
    ///   type alias `User`.
    /// - A temporary consumer `.tl` module that imports `User as Member` and
    ///   uses `Member` as the base call in a constructor-chain expression.
    ///
    /// Output:
    /// - Test passes when `terlc check --emit-phase-manifest` succeeds and the
    ///   CoreIR proof coverage reports one resolved constructor-call identity,
    ///   one resolved constructor-chain identity, and no unresolved chain
    ///   candidates.
    ///
    /// Transformation:
    /// - Runs command-level check through sibling-interface loading,
    ///   alias-aware typechecking, CoreIR type-alias constructor-chain identity
    ///   annotation, and phase-manifest emission.
    #[test]
    fn run_check_single_file_accepts_aliased_imported_alias_constructor_chain_in_core_phase() {
        let dir = make_temp_dir("check_single_file_aliased_imported_alias_constructor_chain");
        let provider = dir.join("provider.tli");
        fs::write(
            &provider,
            "module provider.\n\npub type User = {:user, id: Int, name: Binary}.\n",
        )
        .expect("write provider alias constructor-chain interface");

        let source = dir.join("aliased_imported_alias_constructor_chain.tl");
        fs::write(
            &source,
            "module aliased_imported_alias_constructor_chain.\n\nimport provider.{User as Member}.\n\npub value(id: Int, name: Binary): Dynamic ->\n    Member(id, name) with Admin { id = id, name = name }.\n",
        )
        .expect("write aliased imported alias constructor chain source");
        let manifest = dir.join("aliased_imported_alias_constructor_chain.phase-manifest.json");

        let cache = dir.join("cache");
        let exit = commands::check::run(
            CliCommand {
                verb: Some("check".into()),
                args: vec![
                    source.to_string_lossy().into(),
                    "--emit-phase-manifest".into(),
                    manifest.to_string_lossy().into(),
                ],
            },
            CliState {
                cache_dir: Some(cache),
                ..Default::default()
            },
        );
        assert_eq!(exit, ExitCode::SUCCESS);

        let manifest_text = fs::read_to_string(&manifest).expect("read phase manifest");
        assert!(manifest_text.contains(r#""name":"typecheck","status":"ok""#));
        assert!(manifest_text.contains(r#""name":"core","status":"ok""#));
        let manifest_json: serde_json::Value =
            serde_json::from_str(&manifest_text).expect("parse phase manifest");
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_call_identity"]
                .as_u64()
                .expect("resolved constructor-call identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["resolved_constructor_chain_identity"]
                .as_u64()
                .expect("resolved constructor-chain identity count"),
            1
        );
        assert_eq!(
            manifest_json["core_proof_coverage"]["unresolved_constructor_chain_candidate"]
                .as_u64()
                .expect("unresolved constructor-chain candidate count"),
            0
        );
    }
}

#[cfg(test)]
mod doctest_compile_tests {
    use super::*;
    use terlan_syntax::parse_module_as_syntax_output;

    #[test]
    fn formal_doctest_compiles_terlan_blocks_from_syntax_output() {
        let source = "module docs.\n\n/// Module example.\n///\n/// ```terlan\n/// module docs_example.\n///\n/// pub value(): Int ->\n///     1 + 0.\n/// ```\npub add(X: Int): Int ->\n    X + 1.\n";
        let syntax_output =
            parse_module_as_syntax_output(source).expect("syntax-output module should parse");

        commands::doc::compile_syntax_terlan_doctests(&syntax_output, source, "docs.tl")
            .expect("syntax-output doctest should compile");
    }
}
