use super::*;

#[test]
fn parse_args_defaults_output_directory_to_build() {
    let (state, cmd) = parse_args(vec!["build".into()]);

    assert_eq!(state.out_dir, PathBuf::from("_build"));
    assert!(!state.experimental);
    assert_eq!(cmd.verb.as_deref(), Some("build"));
    assert!(cmd.args.is_empty());
}

#[test]
fn parse_args_accepts_hidden_experimental_flag() {
    let (state, cmd) = parse_args(vec![
        "--experimental".into(),
        "deploy".into(),
        "plan".into(),
        "app".into(),
    ]);

    assert!(state.experimental);
    assert_eq!(cmd.verb.as_deref(), Some("deploy"));
    assert_eq!(cmd.args, vec!["plan".to_string(), "app".to_string()]);
}

#[test]
fn run_cli_rejects_deploy_without_hidden_experimental_flag() {
    assert_eq!(
        run_cli(vec!["deploy".to_string(), "plan".to_string()]),
        ExitCode::from(2)
    );
}

/// Verifies CLI argument parsing defaults documentation to HTML.
///
/// Inputs:
/// - Synthetic CLI arguments containing a `doc` command without an
///   explicit `--format`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the documentation format is the
///   user-facing static HTML reference site by default.
#[test]
fn parse_args_defaults_doc_format_to_html() {
    let (state, cmd) = parse_args(vec!["doc".into(), "std".into()]);

    assert_eq!(state.doc_format, DocFormat::Html);
    assert_eq!(cmd.verb.as_deref(), Some("doc"));
    assert_eq!(cmd.args, vec!["std".to_string()]);
}

/// Verifies CLI argument parsing keeps Markdown as an explicit docs mode.
///
/// Inputs:
/// - Synthetic CLI arguments containing `--format markdown`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Parses the argument vector and asserts the explicit format override
///   selects Markdown while preserving command-local arguments.
#[test]
fn parse_args_accepts_explicit_markdown_doc_format() {
    let (state, cmd) = parse_args(vec![
        "doc".into(),
        "std".into(),
        "--format".into(),
        "markdown".into(),
    ]);

    assert_eq!(state.doc_format, DocFormat::Markdown);
    assert_eq!(cmd.verb.as_deref(), Some("doc"));
    assert_eq!(cmd.args, vec!["std".to_string()]);
}

/// Verifies top-level long help exits successfully.
///
/// Inputs:
/// - Synthetic CLI arguments containing only `--help`.
///
/// Output:
/// - Successful exit code.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms the top-level help path is
///   treated as a successful user request instead of an unknown command.
#[test]
fn run_cli_accepts_top_level_long_help() {
    assert_eq!(run_cli(vec!["--help".into()]), ExitCode::SUCCESS);
}

/// Verifies top-level help command exits successfully.
///
/// Inputs:
/// - Synthetic CLI arguments containing only `help`.
///
/// Output:
/// - Successful exit code.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms the command-style help
///   spelling prints usage successfully.
#[test]
fn run_cli_accepts_top_level_help_command() {
    assert_eq!(run_cli(vec!["help".into()]), ExitCode::SUCCESS);
}

/// Verifies bare top-level usage is release-facing only.
///
/// Inputs:
/// - The static usage lines printed by bare `terlc` and `terlc help`.
///
/// Output:
/// - Test assertions only; no process output is captured.
///
/// Transformation:
/// - Joins the top-level usage allowlist and checks that stable release
///   commands are present while scratch, maintainer, backend-probe, and
///   validation commands are absent from the default user surface.
#[test]
fn top_level_usage_hides_internal_scratch_commands() {
    let usage = public_usage_lines().join("\n");

    for public_command in [
        "terlc help [command]",
        "terlc init [project-name] [--profile default|web|static]",
        "terlc check <file.terl|file.terli|dir>",
        "terlc build [file.terl|dir] [--target erlang|js] [--out-dir <dir>]",
        "terlc run [project-dir] [--target erlang]",
        "terlc test [file.terl|dir] [--target erlang|js] [--name <test_function>]",
        "terlc static <emit|serve|check> <file.terl>",
        "terlc doc <file.terl|dir|std>",
        "terlc db <init|new|validate|status|migrate|rebuild|reset>",
        "terlc repl [--help]",
        "terlc fmt <file.terl>",
        "terlc version | terlc --version | terlc -V",
        "Global options: --diagnostic-format text|json --color auto|always|never --target-profile erlang|js.shared|js.browser|js.worker",
    ] {
        assert!(
            usage.contains(public_command),
            "top-level usage should include `{public_command}`:\n{usage}"
        );
    }

    for internal_command in [
        "bind rust",
        "--experimental",
        "deploy",
        "emit <file.terl>",
        "emit-static",
        "serve-static",
        "emit-js",
        "interface <file.terli>",
        "doctest",
        "emit-native-metadata",
        "hover",
        "lsp",
        "syntax-contract",
        "native-policy",
        "a0-erlang",
        "core-v0",
    ] {
        assert!(
            !usage.contains(internal_command),
            "top-level usage leaked internal command `{internal_command}`:\n{usage}"
        );
    }
}

/// Verifies help-command long help exits successfully.
///
/// Inputs:
/// - Synthetic CLI arguments containing `help --help`.
///
/// Output:
/// - Successful exit code.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms help for the help command
///   is treated as top-level help rather than an unknown command named
///   `--help`.
#[test]
fn run_cli_accepts_help_command_long_help() {
    assert_eq!(
        run_cli(vec!["help".to_string(), "--help".to_string()]),
        ExitCode::SUCCESS
    );
}

/// Verifies help-command short help exits successfully.
///
/// Inputs:
/// - Synthetic CLI arguments containing `help -h`.
///
/// Output:
/// - Successful exit code.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms the short help alias for
///   the help command follows top-level help semantics.
#[test]
fn run_cli_accepts_help_command_short_help() {
    assert_eq!(
        run_cli(vec!["help".to_string(), "-h".to_string()]),
        ExitCode::SUCCESS
    );
}

/// Verifies command-specific help exits successfully for known commands.
///
/// Inputs:
/// - Synthetic CLI arguments in the `help <command>` shape for release
///   commands and REPL.
///
/// Output:
/// - Successful exit code for each known command.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms command-specific help is
///   a successful user request instead of an unknown `help` command.
#[test]
fn run_cli_accepts_help_command_for_known_commands() {
    for command in [
        "help", "init", "bind", "build", "run", "static", "test", "doc", "db", "repl",
    ] {
        assert_eq!(
            run_cli(vec!["help".to_string(), command.to_string()]),
            ExitCode::SUCCESS,
            "help {command} should succeed"
        );
    }
}

/// Verifies command help still works after global options.
///
/// Inputs:
/// - Synthetic CLI arguments with `--color never` before `help build`.
///
/// Output:
/// - Successful exit code.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms global-option parsing does
///   not prevent `help <command>` from using command-specific help.
#[test]
fn run_cli_accepts_help_command_after_global_options() {
    assert_eq!(
        run_cli(vec![
            "--color".to_string(),
            "never".to_string(),
            "help".to_string(),
            "build".to_string(),
        ]),
        ExitCode::SUCCESS
    );
}

/// Verifies release command-local help still works after global options.
///
/// Inputs:
/// - Synthetic CLI arguments with `--color never` before `build --help`.
///
/// Output:
/// - Successful exit code.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms parsed command help
///   routing catches release-command help after global options are stripped.
#[test]
fn run_cli_accepts_release_command_help_after_global_options() {
    assert_eq!(
        run_cli(vec![
            "--color".to_string(),
            "never".to_string(),
            "build".to_string(),
            "--help".to_string(),
        ]),
        ExitCode::SUCCESS
    );
}

/// Verifies top-level help still works after global options.
///
/// Inputs:
/// - Synthetic CLI arguments with `--color never` before `--help` and
///   `-h`.
///
/// Output:
/// - Successful exit code for each top-level help spelling.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms parsed global options do
///   not turn top-level help flags into unknown command names.
#[test]
fn run_cli_accepts_top_level_help_after_global_options() {
    for flag in ["--help", "-h"] {
        assert_eq!(
            run_cli(vec![
                "--color".to_string(),
                "never".to_string(),
                flag.to_string(),
            ]),
            ExitCode::SUCCESS,
            "--color never {flag} should succeed"
        );
    }
}

/// Verifies top-level version still works after global options.
///
/// Inputs:
/// - Synthetic CLI arguments with `--color never` before `--version` and
///   `-V`.
///
/// Output:
/// - Successful exit code for each top-level version spelling.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms parsed global options do
///   not turn top-level version flags into unknown command names.
#[test]
fn run_cli_accepts_top_level_version_after_global_options() {
    for flag in ["--version", "-V"] {
        assert_eq!(
            run_cli(vec![
                "--color".to_string(),
                "never".to_string(),
                flag.to_string(),
            ]),
            ExitCode::SUCCESS,
            "--color never {flag} should succeed"
        );
    }
}

/// Verifies command-local help exits successfully for all known commands.
///
/// Inputs:
/// - Synthetic CLI arguments for each known command followed by `--help`.
///
/// Output:
/// - Successful exit code for each command help request.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms command-local help is
///   consistently handled before command parsers treat `--help` as an
///   operand.
#[test]
fn run_cli_accepts_command_local_help_for_known_commands() {
    for command in [
        "help",
        "init",
        "bind",
        "check",
        "build",
        "run",
        "static",
        "emit",
        "emit-js",
        "test",
        "interface",
        "doc",
        "db",
        "doctest",
        "emit-native-metadata",
        "repl",
        "fmt",
        "hover",
        "lsp",
        "version",
        "syntax-contract",
    ] {
        assert_eq!(
            run_cli(vec![command.to_string(), "--help".to_string()]),
            ExitCode::SUCCESS,
            "{command} --help should succeed"
        );
    }
}

/// Verifies the reserved Rust binding command reaches the generator.
///
/// Inputs:
/// - Synthetic `terlc bind rust --crate polars --out ...` arguments.
///
/// Output:
/// - Exit code assertion only.
///
/// Transformation:
/// - Runs the public dispatcher and confirms the P0.3 binding command
///   shape is recognized and routed to the deterministic generator probe.
#[test]
fn run_cli_reserves_bind_rust_generator_surface() {
    let out_dir = make_temp_dir("bind_rust_generator_surface").join("polars");
    assert_eq!(
        run_cli(vec![
            "bind".to_string(),
            "rust".to_string(),
            "--crate".to_string(),
            "polars".to_string(),
            "--out".to_string(),
            out_dir.to_string_lossy().to_string(),
        ]),
        ExitCode::SUCCESS
    );
    assert!(out_dir.join("terlan.toml").exists());
}

/// Verifies generic command-local help still works after global options.
///
/// Inputs:
/// - Synthetic CLI arguments with `--color never` before `check --help`.
///
/// Output:
/// - Successful exit code.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms parsed command help
///   routing covers non-release commands after global options are stripped.
#[test]
fn run_cli_accepts_generic_command_local_help_after_global_options() {
    assert_eq!(
        run_cli(vec![
            "--color".to_string(),
            "never".to_string(),
            "check".to_string(),
            "--help".to_string(),
        ]),
        ExitCode::SUCCESS
    );
}

/// Verifies command-specific help rejects unknown commands.
///
/// Inputs:
/// - Synthetic CLI arguments in the `help <command>` shape for an unknown
///   command.
///
/// Output:
/// - Usage-style failure exit code.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms unknown command help uses
///   the same invalid-usage exit code as other unknown commands.
#[test]
fn run_cli_rejects_help_command_for_unknown_command() {
    assert_eq!(
        run_cli(vec!["help".to_string(), "unknown".to_string()]),
        ExitCode::from(2)
    );
}

#[test]
fn run_cli_keeps_experimental_deploy_hidden_from_command_help() {
    assert_eq!(
        run_cli(vec!["help".to_string(), "deploy".to_string()]),
        ExitCode::from(2)
    );
}

/// Verifies help command rejects extra command operands.
///
/// Inputs:
/// - Synthetic CLI arguments in the malformed `help build test` shape.
///
/// Output:
/// - Usage-style failure exit code.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms parsed help routing keeps
///   `terlc help` bounded to zero or one command operand.
#[test]
fn run_cli_rejects_help_command_extra_operands() {
    assert_eq!(
        run_cli(vec![
            "help".to_string(),
            "build".to_string(),
            "test".to_string(),
        ]),
        ExitCode::from(2)
    );
}

/// Verifies top-level long version exits successfully.
///
/// Inputs:
/// - Synthetic CLI arguments containing only `--version`.
///
/// Output:
/// - Successful exit code.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms the long version alias is
///   treated as a successful user request instead of an unknown command.
#[test]
fn run_cli_accepts_top_level_long_version() {
    assert_eq!(run_cli(vec!["--version".into()]), ExitCode::SUCCESS);
}

/// Verifies top-level short version exits successfully.
///
/// Inputs:
/// - Synthetic CLI arguments containing only `-V`.
///
/// Output:
/// - Successful exit code.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms the short version alias is
///   treated as a successful user request instead of an unknown command.
#[test]
fn run_cli_accepts_top_level_short_version() {
    assert_eq!(run_cli(vec!["-V".into()]), ExitCode::SUCCESS);
}

/// Verifies version command help exits successfully.
///
/// Inputs:
/// - Synthetic CLI arguments for `version --help` and `version -h`.
///
/// Output:
/// - Successful exit code for each help spelling.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms version command-local
///   help prints usage instead of being treated as version output.
#[test]
fn run_cli_accepts_version_command_help() {
    for flag in ["--help", "-h"] {
        assert_eq!(
            run_cli(vec!["version".to_string(), flag.to_string()]),
            ExitCode::SUCCESS,
            "version {flag} should succeed"
        );
    }
}

/// Verifies version command rejects unexpected arguments.
///
/// Inputs:
/// - Synthetic CLI arguments for `version extra`.
///
/// Output:
/// - Usage-style failure exit code.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms the version command does
///   not silently ignore malformed arguments.
#[test]
fn run_cli_rejects_version_command_extra_arguments() {
    assert_eq!(
        run_cli(vec!["version".to_string(), "extra".to_string()]),
        ExitCode::from(2)
    );
}

/// Verifies release commands accept command-local long help.
///
/// Inputs:
/// - Synthetic CLI arguments for each release command followed by
///   `--help`.
///
/// Output:
/// - Successful exit code for each command help request.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms release-command help is a
///   successful user request, not an invalid command option.
#[test]
fn run_cli_accepts_release_command_long_help() {
    for command in ["init", "build", "run", "test", "doc"] {
        assert_eq!(
            run_cli(vec![command.to_string(), "--help".to_string()]),
            ExitCode::SUCCESS,
            "{command} --help should succeed"
        );
    }
}

/// Verifies release commands accept command-local short help.
///
/// Inputs:
/// - Synthetic CLI arguments for each release command followed by `-h`.
///
/// Output:
/// - Successful exit code for each command help request.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms short release-command help
///   aliases follow the same successful routing as `--help`.
#[test]
fn run_cli_accepts_release_command_short_help() {
    for command in ["init", "build", "run", "test", "doc"] {
        assert_eq!(
            run_cli(vec![command.to_string(), "-h".to_string()]),
            ExitCode::SUCCESS,
            "{command} -h should succeed"
        );
    }
}

/// Verifies command-local help is not consumed by the top-level parser.
///
/// Inputs:
/// - Raw CLI arguments for `repl --help`.
///
/// Output:
/// - Test assertions only; no files are read or written.
///
/// Transformation:
/// - Checks the top-level help detector directly so command implementations
///   retain ownership of their own help flags.
#[test]
fn top_level_help_does_not_consume_command_local_help() {
    let args = vec!["repl".to_string(), "--help".to_string()];

    assert!(!is_help_request(&args));
}

/// Verifies command-local help recognizes REPL help.
///
/// Inputs:
/// - Raw CLI arguments for `repl --help`.
///
/// Output:
/// - Test assertion only; no files are read or written.
///
/// Transformation:
/// - Checks the generic command-local help detector directly so `repl
///   --help` is accepted without treating `--help` as a seed path.
#[test]
fn command_local_help_accepts_repl_help() {
    let args = vec!["repl".to_string(), "--help".to_string()];

    assert_eq!(command_local_help_request(&args), Some("repl"));
}

/// Verifies REPL short help exits successfully through command routing.
///
/// Inputs:
/// - Synthetic CLI arguments for `repl -h`.
///
/// Output:
/// - Successful exit code.
///
/// Transformation:
/// - Runs the public CLI dispatcher and confirms REPL-owned short help is
///   routed to the REPL command instead of being treated as a seed path.
#[test]
fn run_cli_accepts_repl_short_help() {
    assert_eq!(
        run_cli(vec!["repl".to_string(), "-h".to_string()]),
        ExitCode::SUCCESS
    );
}
