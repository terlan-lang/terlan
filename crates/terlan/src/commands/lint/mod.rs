use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

mod diagnostic;
mod paths;
mod rules;

use diagnostic::{render_diagnostic, LintDiagnostic};
use paths::collect_lint_paths;
use rules::apply_safe_fixes;

#[cfg(test)]
use rules::{fix_semicolon_chains, lint_source};

/// Executes the `lint` CLI command.
///
/// Inputs:
/// - `args`: command-local arguments after the `lint` verb.
///
/// Output:
/// - Success when no diagnostics remain.
/// - Exit code `1` when diagnostics are reported or a filesystem operation
///   fails.
/// - Exit code `2` for malformed command arguments.
///
/// Transformation:
/// - Walks one file or directory for `.terl`/`.terli` sources, emits stable
///   rule diagnostics, and applies only narrow source-preserving fixes when
///   `--fix` is requested.
pub(crate) fn run(args: &[String]) -> ExitCode {
    let options = match parse_args(args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            eprintln!("usage: terlc lint [--fix] <file.terl|file.terli|dir>");
            return ExitCode::from(2);
        }
    };

    match run_lint(&options.path, options.fix) {
        Ok(diagnostics) if diagnostics.is_empty() => ExitCode::SUCCESS,
        Ok(diagnostics) => {
            for diagnostic in diagnostics {
                eprintln!("{}", render_diagnostic(&diagnostic));
            }
            ExitCode::from(1)
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

/// Parsed lint command options.
struct LintOptions {
    fix: bool,
    path: PathBuf,
}

/// Parses command-local lint arguments.
fn parse_args(args: &[String]) -> Result<LintOptions, String> {
    let mut fix = false;
    let mut paths = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--fix" => fix = true,
            flag if flag.starts_with('-') => return Err(format!("unknown lint option: {flag}")),
            _ => paths.push(PathBuf::from(arg)),
        }
    }

    match paths.as_slice() {
        [path] => Ok(LintOptions {
            fix,
            path: path.clone(),
        }),
        [] => Err("missing path argument".to_string()),
        _ => Err("terlc lint accepts exactly one path".to_string()),
    }
}

/// Runs lint analysis and optional safe fixes over one path.
fn run_lint(root: &Path, fix: bool) -> Result<Vec<LintDiagnostic>, String> {
    let paths = collect_lint_paths(root)?;
    if fix {
        apply_safe_fixes(&paths)?;
    }

    let mut diagnostics = Vec::new();
    for path in paths {
        let source = fs::read_to_string(&path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        diagnostics.extend(rules::lint_source(&path, &source));
    }
    Ok(diagnostics)
}

#[cfg(test)]
#[path = "lint_test.rs"]
mod lint_test;
