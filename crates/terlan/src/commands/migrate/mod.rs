use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use regex::Regex;
use serde_json::json;

use crate::CliCommand;

const MIGRATION_ID_INVALID_ALIAS_STYLE: &str =
    "migration.function_head_pattern.invalid_alias_style";

/// One planned or applied function-head migration action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MigrationChange {
    pub(crate) path: PathBuf,
    pub(crate) line: usize,
    pub(crate) function_name: String,
    pub(crate) arity: usize,
    pub(crate) migration_id: &'static str,
    pub(crate) status: MigrationStatus,
    pub(crate) reason: String,
    pub(crate) before: String,
    pub(crate) after: String,
}

/// Migration outcome for one candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MigrationStatus {
    Planned,
    Applied,
    SafeRejected,
}

/// Report produced by `terlc migrate pattern-head`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MigrationReport {
    pub(crate) changed_file_count: usize,
    pub(crate) planned_count: usize,
    pub(crate) applied_count: usize,
    pub(crate) safe_rejected_count: usize,
    pub(crate) changes: Vec<MigrationChange>,
}

struct MigrateOptions {
    write: bool,
    json: bool,
    path: PathBuf,
}

struct FunctionHead {
    name: String,
    arity: usize,
    params_start: usize,
    params_end: usize,
}

/// Executes the `migrate` CLI command.
pub(crate) fn run(cmd: CliCommand) -> ExitCode {
    let options = match parse_args(&cmd.args) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            eprintln!(
                "usage: terlc migrate pattern-head [--write] [--json] <file.terl|file.terli|dir>"
            );
            return ExitCode::from(2);
        }
    };

    match run_pattern_head_migration(&options.path, options.write) {
        Ok(report) => {
            if options.json {
                println!("{}", render_json_report(&report));
            } else {
                print_text_report(&report, options.write);
            }
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}

fn parse_args(args: &[String]) -> Result<MigrateOptions, String> {
    let Some((subcommand, rest)) = args.split_first() else {
        return Err("missing migration subcommand".to_string());
    };
    if subcommand != "pattern-head" {
        return Err(format!("unknown migration subcommand: {subcommand}"));
    }

    let mut write = false;
    let mut json = false;
    let mut paths = Vec::new();
    for arg in rest {
        match arg.as_str() {
            "--write" => write = true,
            "--json" => json = true,
            flag if flag.starts_with('-') => return Err(format!("unknown migrate option: {flag}")),
            _ => paths.push(PathBuf::from(arg)),
        }
    }

    match paths.as_slice() {
        [path] => Ok(MigrateOptions {
            write,
            json,
            path: path.clone(),
        }),
        [] => Err("missing path argument".to_string()),
        _ => Err("terlc migrate pattern-head accepts exactly one path".to_string()),
    }
}

/// Runs conservative function-head pattern migration on one file or directory.
pub(crate) fn run_pattern_head_migration(
    path: &Path,
    write: bool,
) -> Result<MigrationReport, String> {
    let files = collect_terlan_sources(path)?;
    let mut all_changes = Vec::new();
    let mut changed_file_count = 0;

    for file in files {
        let source = fs::read_to_string(&file)
            .map_err(|err| format!("failed to read {}: {err}", file.display()))?;
        let MigrationFileResult {
            output,
            changes,
            changed,
        } = migrate_source(&file, &source, write)?;
        if write && changed {
            fs::write(&file, output)
                .map_err(|err| format!("failed to write {}: {err}", file.display()))?;
            changed_file_count += 1;
        } else if changed {
            changed_file_count += 1;
        }
        all_changes.extend(changes);
    }

    Ok(MigrationReport {
        changed_file_count,
        planned_count: all_changes
            .iter()
            .filter(|change| change.status == MigrationStatus::Planned)
            .count(),
        applied_count: all_changes
            .iter()
            .filter(|change| change.status == MigrationStatus::Applied)
            .count(),
        safe_rejected_count: all_changes
            .iter()
            .filter(|change| change.status == MigrationStatus::SafeRejected)
            .count(),
        changes: all_changes,
    })
}

struct MigrationFileResult {
    output: String,
    changes: Vec<MigrationChange>,
    changed: bool,
}

fn migrate_source(path: &Path, source: &str, write: bool) -> Result<MigrationFileResult, String> {
    let mut changed = false;
    let mut output_lines = Vec::new();
    let mut changes = Vec::new();

    for (line_index, line) in source.lines().enumerate() {
        let Some(head) = parse_function_head(line) else {
            output_lines.push(line.to_string());
            continue;
        };
        let params = &line[head.params_start..head.params_end];
        let MigratedParams {
            params,
            changes: mut line_changes,
            changed: line_changed,
        } = migrate_params(path, line_index + 1, &head, params, write)?;
        if line_changed {
            let mut next = String::new();
            next.push_str(&line[..head.params_start]);
            next.push_str(&params);
            next.push_str(&line[head.params_end..]);
            output_lines.push(next);
            changed = true;
        } else {
            output_lines.push(line.to_string());
        }
        changes.append(&mut line_changes);
    }

    let mut output = output_lines.join("\n");
    if source.ends_with('\n') {
        output.push('\n');
    }
    Ok(MigrationFileResult {
        output,
        changes,
        changed,
    })
}

struct MigratedParams {
    params: String,
    changes: Vec<MigrationChange>,
    changed: bool,
}

fn migrate_params(
    path: &Path,
    line: usize,
    head: &FunctionHead,
    params: &str,
    write: bool,
) -> Result<MigratedParams, String> {
    let parts = split_top_level_params(params);
    let mut changed = false;
    let mut changes = Vec::new();
    let mut migrated_parts = Vec::new();

    for part in parts {
        match migrate_param(&part)? {
            ParamMigration::Changed { before, after } => {
                changed = true;
                changes.push(MigrationChange {
                    path: path.to_path_buf(),
                    line,
                    function_name: head.name.clone(),
                    arity: head.arity,
                    migration_id: MIGRATION_ID_INVALID_ALIAS_STYLE,
                    status: if write {
                        MigrationStatus::Applied
                    } else {
                        MigrationStatus::Planned
                    },
                    reason: "safe reverse-alias rewrite".to_string(),
                    before,
                    after: after.clone(),
                });
                migrated_parts.push(after);
            }
            ParamMigration::Rejected { before, reason } => {
                changes.push(MigrationChange {
                    path: path.to_path_buf(),
                    line,
                    function_name: head.name.clone(),
                    arity: head.arity,
                    migration_id: MIGRATION_ID_INVALID_ALIAS_STYLE,
                    status: MigrationStatus::SafeRejected,
                    reason,
                    before: before.clone(),
                    after: before.clone(),
                });
                migrated_parts.push(before);
            }
            ParamMigration::Unchanged(param) => migrated_parts.push(param),
        }
    }

    Ok(MigratedParams {
        params: migrated_parts.join(", "),
        changes,
        changed,
    })
}

enum ParamMigration {
    Changed { before: String, after: String },
    Rejected { before: String, reason: String },
    Unchanged(String),
}

fn migrate_param(param: &str) -> Result<ParamMigration, String> {
    let trimmed = param.trim();
    if !trimmed.contains('=') {
        return Ok(ParamMigration::Unchanged(param.to_string()));
    }
    if is_pattern_first_alias(trimmed) {
        return Ok(ParamMigration::Unchanged(param.to_string()));
    }

    let re = Regex::new(
        r"^(?P<alias>[a-z][A-Za-z0-9_]*)\s*=\s*(?P<pattern>\{.*\}|\[.*\]|[A-Z][A-Za-z0-9_]*\s*\(.*\))\s*:\s*(?P<ty>.+)$",
    )
    .map_err(|err| format!("invalid migration regex: {err}"))?;

    let Some(captures) = re.captures(trimmed) else {
        return Ok(ParamMigration::Rejected {
            before: param.to_string(),
            reason: "ambiguous reverse-alias shape requires manual review".to_string(),
        });
    };

    let alias = captures.name("alias").expect("alias").as_str();
    let pattern = captures.name("pattern").expect("pattern").as_str().trim();
    let ty = captures.name("ty").expect("ty").as_str().trim();
    if !is_balanced(pattern) || !is_supported_pattern(pattern) {
        return Ok(ParamMigration::Rejected {
            before: param.to_string(),
            reason: "pattern shape is not safe for automatic rewrite".to_string(),
        });
    }

    let leading = &param[..param.len() - param.trim_start().len()];
    let trailing = &param[param.trim_end().len()..];
    Ok(ParamMigration::Changed {
        before: param.to_string(),
        after: format!("{leading}{pattern} = {alias}: {ty}{trailing}"),
    })
}

fn parse_function_head(line: &str) -> Option<FunctionHead> {
    let re = Regex::new(r"^\s*(?:pub\s+)?(?P<name>[a-z][A-Za-z0-9_]*)\s*\(").ok()?;
    let captures = re.captures(line)?;
    let name = captures.name("name")?.as_str().to_string();
    let open = line.find('(')?;
    let close = find_matching_paren(line, open)?;
    let suffix = line[close + 1..].trim_start();
    if !(suffix.starts_with(':') || suffix.starts_with("->") || suffix.starts_with('.')) {
        return None;
    }
    let params = &line[open + 1..close];
    Some(FunctionHead {
        name,
        arity: split_top_level_params(params)
            .into_iter()
            .filter(|part| !part.trim().is_empty())
            .count(),
        params_start: open + 1,
        params_end: close,
    })
}

fn find_matching_paren(line: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (offset, ch) in line[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(open + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_top_level_params(params: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0usize;
    let mut paren = 0usize;
    let mut brace = 0usize;
    let mut bracket = 0usize;
    for (index, ch) in params.char_indices() {
        match ch {
            '(' => paren += 1,
            ')' => paren = paren.saturating_sub(1),
            '{' => brace += 1,
            '}' => brace = brace.saturating_sub(1),
            '[' => bracket += 1,
            ']' => bracket = bracket.saturating_sub(1),
            ',' if paren == 0 && brace == 0 && bracket == 0 => {
                parts.push(params[start..index].to_string());
                start = index + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(params[start..].to_string());
    parts
}

fn is_supported_pattern(pattern: &str) -> bool {
    (pattern.starts_with('{') && pattern.ends_with('}'))
        || (pattern.starts_with('[') && pattern.ends_with(']'))
        || Regex::new(r"^[A-Z][A-Za-z0-9_]*\s*\(.*\)$")
            .ok()
            .is_some_and(|re| re.is_match(pattern))
}

fn is_pattern_first_alias(param: &str) -> bool {
    let Some((pattern, rest)) = param.split_once('=') else {
        return false;
    };
    let pattern = pattern.trim();
    let rest = rest.trim();
    is_supported_pattern(pattern)
        && Regex::new(r"^[a-z][A-Za-z0-9_]*\s*:")
            .ok()
            .is_some_and(|re| re.is_match(rest))
}

fn is_balanced(text: &str) -> bool {
    let mut stack = Vec::new();
    for ch in text.chars() {
        match ch {
            '(' | '[' | '{' => stack.push(ch),
            ')' => {
                if stack.pop() != Some('(') {
                    return false;
                }
            }
            ']' => {
                if stack.pop() != Some('[') {
                    return false;
                }
            }
            '}' if stack.pop() != Some('{') => {
                return false;
            }
            _ => {}
        }
    }
    stack.is_empty()
}

fn collect_terlan_sources(path: &Path) -> Result<Vec<PathBuf>, String> {
    if path.is_file() {
        if is_terlan_source(path) {
            return Ok(vec![path.to_path_buf()]);
        }
        return Err(format!("not a Terlan source file: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("path does not exist: {}", path.display()));
    }

    let mut paths = Vec::new();
    collect_terlan_sources_into(path, &mut paths)?;
    paths.sort();
    Ok(paths)
}

fn collect_terlan_sources_into(dir: &Path, paths: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|err| format!("failed to read directory {}: {err}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|err| format!("failed to read directory entry: {err}"))?;
        let path = entry.path();
        if path.is_dir() {
            collect_terlan_sources_into(&path, paths)?;
        } else if is_terlan_source(&path) {
            paths.push(path);
        }
    }
    Ok(())
}

fn is_terlan_source(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("terl" | "terli")
    )
}

fn print_text_report(report: &MigrationReport, write: bool) {
    let mode = if write { "write" } else { "dry-run" };
    println!(
        "function-head pattern migration {mode}: {} planned, {} applied, {} safe-rejected, {} changed files",
        report.planned_count,
        report.applied_count,
        report.safe_rejected_count,
        report.changed_file_count
    );
    for change in &report.changes {
        println!(
            "{}:{} {} {}/{} {}: {}",
            change.path.display(),
            change.line,
            status_text(change.status),
            change.function_name,
            change.arity,
            change.migration_id,
            change.reason
        );
    }
}

fn status_text(status: MigrationStatus) -> &'static str {
    match status {
        MigrationStatus::Planned => "planned",
        MigrationStatus::Applied => "applied",
        MigrationStatus::SafeRejected => "safe-rejected",
    }
}

fn render_json_report(report: &MigrationReport) -> String {
    let changes = report
        .changes
        .iter()
        .map(|change| {
            json!({
                "path": change.path,
                "line": change.line,
                "function_name": change.function_name,
                "arity": change.arity,
                "migration_id": change.migration_id,
                "status": status_text(change.status),
                "reason": change.reason,
                "before": change.before,
                "after": change.after
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema": "terlan.function-head-pattern-migration-assist-report.v1",
        "planned_count": report.planned_count,
        "applied_count": report.applied_count,
        "safe_rejected_count": report.safe_rejected_count,
        "changed_file_count": report.changed_file_count,
        "changes": changes
    })
    .to_string()
}

#[cfg(test)]
#[path = "migrate_test.rs"]
#[cfg(test)]
mod migrate_test;
