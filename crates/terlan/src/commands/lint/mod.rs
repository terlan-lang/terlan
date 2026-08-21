use std::fs;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;
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
/// - Walks one or more files or directories for Terlan sources, deduplicates
///   overlapping inputs, emits stable rule diagnostics, and applies only
///   narrow source-preserving fixes when `--fix` is requested.
pub(crate) fn run(args: &[String]) -> ExitCode {
    let options = match parse_args(args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            eprintln!(
                "usage: terlc lint [--fix] [--only <rule-id>]... <file.terl|file.terli|file.terls|dir>..."
            );
            return ExitCode::from(2);
        }
    };

    match run_lint_rules_many(&options.paths, options.fix, &options.only_rules) {
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
    only_rules: Vec<String>,
    paths: Vec<PathBuf>,
}

/// Parses command-local lint arguments.
fn parse_args(args: &[String]) -> Result<LintOptions, String> {
    let mut fix = false;
    let mut only_rules = Vec::new();
    let mut paths = Vec::new();

    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--fix" => fix = true,
            "--only" => {
                let rule_id = args
                    .next()
                    .ok_or_else(|| "missing rule ID after --only".to_string())?;
                if !only_rules.contains(rule_id) {
                    only_rules.push(rule_id.clone());
                }
            }
            flag if flag.starts_with('-') => return Err(format!("unknown lint option: {flag}")),
            _ => paths.push(PathBuf::from(arg)),
        }
    }

    if fix && !only_rules.is_empty() {
        return Err("--fix and --only cannot be combined".to_string());
    }
    for rule_id in &only_rules {
        if !KNOWN_RULE_IDS.contains(&rule_id.as_str()) {
            return Err(format!("unknown lint rule ID: {rule_id}"));
        }
    }

    match paths.is_empty() {
        false => Ok(LintOptions {
            fix,
            only_rules,
            paths,
        }),
        true => Err("missing path argument".to_string()),
    }
}

const KNOWN_RULE_IDS: &[&str] = &[
    "TL0001", "TL0002", "TL0003", "TL0004", "TL0005", "TL0006", "TL0007", "TL0008", "TL0009",
    "TL0010", "TL0101", "TL0201", "TL0301", "TL0401", "TL0501", "TL0601", "TL0701", "TL0702",
    "TL0801", "TL0804", "TL0805", "TL0901", "TL0902", "TL0903", "TL1001", "TL1002", "TL1003",
];

/// Runs lint analysis and optional safe fixes over one path.
#[cfg(test)]
fn run_lint(root: &Path, fix: bool) -> Result<Vec<LintDiagnostic>, String> {
    run_lint_selected(root, fix, None)
}

#[cfg(test)]
fn run_lint_selected(
    root: &Path,
    fix: bool,
    only_rule: Option<&str>,
) -> Result<Vec<LintDiagnostic>, String> {
    run_lint_selected_many(&[root.to_path_buf()], fix, only_rule)
}

#[cfg(test)]
fn run_lint_selected_many(
    roots: &[PathBuf],
    fix: bool,
    only_rule: Option<&str>,
) -> Result<Vec<LintDiagnostic>, String> {
    let only_rules = only_rule
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    run_lint_rules_many(roots, fix, &only_rules)
}

fn run_lint_rules_many(
    roots: &[PathBuf],
    fix: bool,
    only_rules: &[String],
) -> Result<Vec<LintDiagnostic>, String> {
    let mut paths = Vec::new();
    for root in roots {
        paths.extend(collect_lint_paths(root)?);
    }
    paths.sort();
    paths.dedup();
    if fix {
        apply_safe_fixes(&paths).map_err(|error| error.to_string())?;
    }

    let mut diagnostics = Vec::new();
    for path in paths {
        let source = fs::read_to_string(&path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        diagnostics.extend(if only_rules.is_empty() {
            rules::lint_source(&path, &source)
        } else {
            rules::lint_source_only_many(&path, &source, only_rules)
        });
    }
    Ok(diagnostics)
}

#[cfg(test)]
#[path = "lint_test.rs"]
#[cfg(test)]
mod lint_test;
