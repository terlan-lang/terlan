use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use terlan_syntax::{
    cached_canonical_terlan_syntax_contract_artifact,
    cached_canonical_terlan_syntax_contract_artifact_json,
    check_syntax_contract_artifact_against_current, SyntaxContractArtifactCheck,
};

/// Executes the `syntax-contract` CLI command.
///
/// Inputs:
/// - `args`: command-local arguments after the `syntax-contract` verb.
///
/// Output:
/// - `ExitCode::SUCCESS` when artifact emission or checking succeeds.
/// - Non-zero `ExitCode` when arguments are invalid or artifact work fails.
///
/// Transformation:
/// - Parses command-local arguments, routes to emit/check behavior, prints
///   user-facing command output, and returns the resulting process status.
pub(crate) fn run(args: &[String]) -> ExitCode {
    match parse_syntax_contract_command(args) {
        Ok(SyntaxContractCommand::Emit { mode, out_path }) => {
            run_syntax_contract_emit(mode, out_path)
        }
        Ok(SyntaxContractCommand::Check { path }) => run_syntax_contract_check(&path),
        Err(SyntaxContractCommandParseError) => {
            crate::print_usage();
            ExitCode::from(2)
        }
    }
}

/// Output mode for the `syntax-contract` command.
///
/// Inputs:
/// - Parsed command flags such as `--fingerprint`.
///
/// Output:
/// - Command-local mode used by artifact emission.
///
/// Transformation:
/// - Separates full artifact JSON output from compact fingerprint output while
///   keeping both modes behind the same command implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SyntaxContractOutputMode {
    ArtifactJson,
    Fingerprint,
}

/// Parsed `syntax-contract` command plan.
///
/// Inputs:
/// - Flat command-local argument list.
///
/// Output:
/// - Either artifact/fingerprint emission or artifact checking with a path.
///
/// Transformation:
/// - Converts mutually exclusive CLI flags into a typed command enum so the
///   runner can dispatch without revalidating argument combinations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SyntaxContractCommand {
    Emit {
        mode: SyntaxContractOutputMode,
        out_path: Option<PathBuf>,
    },
    Check {
        path: PathBuf,
    },
}

/// Marker error for invalid `syntax-contract` arguments.
///
/// Inputs:
/// - Invalid command-local argument state detected by the parser.
///
/// Output:
/// - Zero-sized parse error consumed by the command runner.
///
/// Transformation:
/// - Keeps the command parser's public error shape intentionally small because
///   invalid syntax-contract arguments all route to normal CLI usage output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SyntaxContractCommandParseError;

/// Parses command-local `syntax-contract` arguments.
///
/// Inputs:
/// - `args`: command-local argument strings after the verb.
///
/// Output:
/// - `Ok(SyntaxContractCommand)` describing either artifact emission or
///   artifact checking.
/// - `Err(SyntaxContractCommandParseError)` when flags are unsupported,
///   incomplete, duplicated, or mutually exclusive.
///
/// Transformation:
/// - Converts flat CLI arguments into a typed command plan while enforcing
///   `--fingerprint`, `--out`, and `--check` compatibility rules.
pub(crate) fn parse_syntax_contract_command(
    args: &[String],
) -> Result<SyntaxContractCommand, SyntaxContractCommandParseError> {
    let mut mode = SyntaxContractOutputMode::ArtifactJson;
    let mut out_path = None;
    let mut check_path = None;
    let mut index = 0;

    while index < args.len() {
        match args[index].as_str() {
            "--fingerprint" => {
                if mode == SyntaxContractOutputMode::Fingerprint || check_path.is_some() {
                    return Err(SyntaxContractCommandParseError);
                }
                mode = SyntaxContractOutputMode::Fingerprint;
                index += 1;
            }
            "--out" => {
                if out_path.is_some() || check_path.is_some() || index + 1 >= args.len() {
                    return Err(SyntaxContractCommandParseError);
                }
                out_path = Some(PathBuf::from(&args[index + 1]));
                index += 2;
            }
            "--check" => {
                if check_path.is_some()
                    || out_path.is_some()
                    || mode != SyntaxContractOutputMode::ArtifactJson
                    || index + 1 >= args.len()
                {
                    return Err(SyntaxContractCommandParseError);
                }
                check_path = Some(PathBuf::from(&args[index + 1]));
                index += 2;
            }
            _ => return Err(SyntaxContractCommandParseError),
        }
    }

    if let Some(path) = check_path {
        Ok(SyntaxContractCommand::Check { path })
    } else {
        Ok(SyntaxContractCommand::Emit { mode, out_path })
    }
}

/// Emits the current syntax contract artifact or fingerprint.
///
/// Inputs:
/// - `mode`: output mode selected by parsed command arguments.
/// - `out_path`: optional output file path; absence means write to stdout.
///
/// Output:
/// - `ExitCode::SUCCESS` when output is generated and written.
/// - `ExitCode::from(1)` when contract loading or file writing fails.
///
/// Transformation:
/// - Loads the cached canonical contract output, writes it to a file or stdout,
///   and appends a trailing newline when writing to a file.
fn run_syntax_contract_emit(mode: SyntaxContractOutputMode, out_path: Option<PathBuf>) -> ExitCode {
    let output = match syntax_contract_command_output(mode) {
        Ok(output) => output,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    if let Some(path) = out_path {
        if let Err(error) = fs::write(&path, format!("{output}\n")) {
            eprintln!("failed to write syntax contract artifact: {error}");
            return ExitCode::from(1);
        }
    } else {
        println!("{output}");
    }
    ExitCode::SUCCESS
}

/// Checks an artifact file against the current syntax contract.
///
/// Inputs:
/// - `path`: artifact or fingerprint file path to validate.
///
/// Output:
/// - `ExitCode::SUCCESS` when the artifact matches.
/// - `ExitCode::from(1)` when it mismatches, is invalid, or cannot be loaded.
///
/// Transformation:
/// - Reads and checks the artifact, converts structured match/mismatch results
///   into command output, and returns the command status.
fn run_syntax_contract_check(path: &Path) -> ExitCode {
    match syntax_contract_file_check(path) {
        Ok(SyntaxContractArtifactCheck::Match { .. }) => ExitCode::SUCCESS,
        Ok(SyntaxContractArtifactCheck::Mismatch { expected, found }) => {
            eprintln!(
                "syntax contract fingerprint mismatch: {} (expected {expected}, found {found})",
                path.display()
            );
            ExitCode::from(1)
        }
        Ok(SyntaxContractArtifactCheck::InvalidArtifact) => {
            eprintln!("invalid syntax contract artifact: {}", path.display());
            ExitCode::from(1)
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

/// Checks artifact file contents against the current contract.
///
/// Inputs:
/// - `path`: artifact or fingerprint file path to read.
///
/// Output:
/// - `Ok(SyntaxContractArtifactCheck)` when the artifact is readable and
///   comparable.
/// - `Err(String)` when the file cannot be read or the current contract cannot
///   be loaded.
///
/// Transformation:
/// - Reads artifact text from disk and delegates contract comparison to
///   `terlan_syntax`.
pub(crate) fn syntax_contract_file_check(
    path: &Path,
) -> Result<SyntaxContractArtifactCheck, String> {
    let contents = fs::read_to_string(path)
        .map_err(|error| format!("failed to read syntax contract artifact: {error}"))?;
    check_syntax_contract_artifact_against_current(&contents)
        .map_err(|error| syntax_contract_load_error(format!("{error:?}")))
}

/// Produces syntax-contract command output text.
///
/// Inputs:
/// - `mode`: artifact JSON or fingerprint output mode.
///
/// Output:
/// - `Ok(String)` containing the requested artifact output without a guaranteed
///   trailing newline.
/// - `Err(String)` when the cached canonical syntax contract cannot be loaded.
///
/// Transformation:
/// - Loads cached contract data from `terlan_syntax` and selects either the
///   full artifact JSON or only the fingerprint.
pub(crate) fn syntax_contract_command_output(
    mode: SyntaxContractOutputMode,
) -> Result<String, String> {
    match mode {
        SyntaxContractOutputMode::ArtifactJson => {
            cached_canonical_terlan_syntax_contract_artifact_json()
                .map_err(|error| syntax_contract_load_error(format!("{error:?}")))
        }
        SyntaxContractOutputMode::Fingerprint => {
            let artifact = cached_canonical_terlan_syntax_contract_artifact()
                .map_err(|error| syntax_contract_load_error(format!("{error:?}")))?;
            Ok(artifact.fingerprint)
        }
    }
}

/// Formats syntax-contract load failures for command output.
///
/// Inputs:
/// - `message`: lower-level contract loading or comparison error text.
///
/// Output:
/// - CLI-ready error string with command context.
///
/// Transformation:
/// - Prefixes the lower-level message with the stable syntax-contract load
///   failure label.
fn syntax_contract_load_error(message: String) -> String {
    format!("failed to load syntax contract artifact: {message}")
}
