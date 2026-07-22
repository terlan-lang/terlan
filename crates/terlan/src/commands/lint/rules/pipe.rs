use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::terlan_syntax::{
    parse_module_as_syntax_output, SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput,
    SyntaxImportKind, SyntaxModuleOutput,
};

use crate::commands::lint::diagnostic::{LintDiagnostic, Severity};

const PIPE_CANDIDATE_RULE_ID: &str = "TL1002";
const PIPE_CANDIDATE_RULE_NAME: &str = "format-boundary.pipe-fix";

/// One proven-safe pipe rewrite candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PipeCandidate {
    original: String,
    start: usize,
    end: usize,
    stages: Vec<String>,
}

/// Builds pipe-canonicalization diagnostics from parsed syntax output.
pub(super) fn pipe_candidate_diagnostics(path: &Path, source: &str) -> Vec<LintDiagnostic> {
    located_pipe_candidates(source)
        .into_iter()
        .map(|(start, _candidate)| {
            let (line, column) = source_line_column_at(source, start);
            LintDiagnostic {
                path: path.to_path_buf(),
                line,
                column,
                rule_id: PIPE_CANDIDATE_RULE_ID,
                rule_name: PIPE_CANDIDATE_RULE_NAME,
                severity: Severity::Suggestion,
                message: "prefer pipe form for a safe nested first-argument call chain",
                fix_available: true,
            }
        })
        .collect()
}

/// Applies safe pipe-canonicalization fixes.
pub(super) fn fix_pipe_candidates(source: &str) -> String {
    let mut fixed = source.to_string();
    let mut replacements = located_pipe_candidates(source);
    replacements.sort_by(|left, right| right.0.cmp(&left.0));
    for (start, candidate) in replacements {
        let end = start + candidate.original.len();
        let Some(current) = fixed.get(start..end) else {
            continue;
        };
        if current != candidate.original {
            continue;
        }
        let indent = line_indent_at(&fixed, start);
        let replacement = render_pipe_replacement(&candidate.stages, indent);
        fixed.replace_range(start..end, &replacement);
    }
    fixed
}

fn located_pipe_candidates(source: &str) -> Vec<(usize, PipeCandidate)> {
    let mut next_search_start = HashMap::<(usize, usize, String), usize>::new();
    pipe_candidates(source)
        .into_iter()
        .filter_map(|candidate| {
            let key = (candidate.start, candidate.end, candidate.original.clone());
            let search_start = *next_search_start.get(&key).unwrap_or(&candidate.start);
            let start = candidate_source_start(source, &candidate, search_start)?;
            next_search_start.insert(key, start + candidate.original.len());
            Some((start, candidate))
        })
        .collect()
}

fn candidate_source_start(
    source: &str,
    candidate: &PipeCandidate,
    search_start: usize,
) -> Option<usize> {
    if let Some(start) = candidate_source_start_in_range(
        source,
        candidate,
        search_start,
        candidate.end.min(source.len()),
    ) {
        return Some(start);
    }

    candidate_source_start_in_range(source, candidate, search_start, source.len())
}

fn candidate_source_start_in_range(
    source: &str,
    candidate: &PipeCandidate,
    search_start: usize,
    end: usize,
) -> Option<usize> {
    if search_start > end {
        return None;
    }

    let window = source.get(search_start..end)?;
    let mut offset = 0;
    while offset <= window.len() {
        let Some(relative) = window[offset..].find(&candidate.original) else {
            break;
        };
        let absolute = search_start + offset + relative;
        if !is_inside_string_literal(source, absolute) && !is_inside_line_comment(source, absolute)
        {
            return Some(absolute);
        }
        offset += relative + candidate.original.len();
    }
    None
}

fn is_inside_string_literal(source: &str, offset: usize) -> bool {
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let mut escaped = false;
    let mut quote_count = 0usize;
    for ch in source[line_start..offset].chars() {
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == '"' {
            quote_count += 1;
        }
    }
    quote_count % 2 == 1
}

fn is_inside_line_comment(source: &str, offset: usize) -> bool {
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    source[line_start..offset].contains("//")
}

/// Collects safe nested first-argument call chains from source.
fn pipe_candidates(source: &str) -> Vec<PipeCandidate> {
    let Ok(module) = parse_module_as_syntax_output(source) else {
        return Vec::new();
    };

    let local_call_names = local_pipe_call_names(&module);
    let mut candidates = Vec::new();
    for declaration in module.declarations {
        match declaration.payload {
            SyntaxDeclarationPayload::Function { clauses, .. }
            | SyntaxDeclarationPayload::Method { clauses, .. } => {
                for clause in clauses {
                    collect_body_pipe_candidates(&clause.body, &local_call_names, &mut candidates);
                }
            }
            _ => {}
        }
    }
    candidates
}

/// Collects candidates only from expression bodies, not nested argument lists.
fn collect_body_pipe_candidates(
    expr: &SyntaxExprOutput,
    local_call_names: &HashSet<String>,
    candidates: &mut Vec<PipeCandidate>,
) {
    match expr.kind {
        SyntaxExprKind::Sequence => {
            for child in &expr.children {
                collect_body_pipe_candidates(child, local_call_names, candidates);
            }
        }
        SyntaxExprKind::Let => {
            for child in expr.children.iter().skip(expr.patterns.len()) {
                collect_body_pipe_candidates(child, local_call_names, candidates);
            }
        }
        _ => {
            if let Some(candidate) = pipe_candidate_for_expr(expr, local_call_names) {
                candidates.push(candidate);
            }
        }
    }
}

/// Converts one safe nested call into pipe stages.
fn pipe_candidate_for_expr(
    expr: &SyntaxExprOutput,
    local_call_names: &HashSet<String>,
) -> Option<PipeCandidate> {
    let outer = named_call_parts(expr, local_call_names)?;
    if outer.args.len() <= 1 {
        return None;
    }
    let mut stages = pipe_stage_chain_for_expr(outer.args.first()?, local_call_names)?;
    if stages.len() < 2 {
        return None;
    }
    stages.push(render_named_stage(
        &outer,
        &outer.args[1..],
        local_call_names,
    )?);

    Some(PipeCandidate {
        original: render_expr_subset(expr, local_call_names)?,
        start: expr.span.start,
        end: expr.span.end,
        stages,
    })
}

/// Converts a nested first-argument call chain into pipe stages.
fn pipe_stage_chain_for_expr(
    expr: &SyntaxExprOutput,
    local_call_names: &HashSet<String>,
) -> Option<Vec<String>> {
    if is_simple_pipe_base(expr) {
        return Some(vec![render_expr_subset(expr, local_call_names)?]);
    }

    if let Some(call) = named_call_parts(expr, local_call_names) {
        let mut stages = pipe_stage_chain_for_expr(call.args.first()?, local_call_names)?;
        stages.push(render_named_stage(
            &call,
            &call.args[1..],
            local_call_names,
        )?);
        return Some(stages);
    }

    let call = receiver_call_parts(expr)?;
    let mut stages = pipe_stage_chain_for_expr(call.receiver, local_call_names)?;
    stages.push(render_receiver_stage(&call, local_call_names)?);
    Some(stages)
}

/// Returns whether an expression is a safe first pipe stage for this rule.
fn is_simple_pipe_base(expr: &SyntaxExprOutput) -> bool {
    matches!(
        expr.kind,
        SyntaxExprKind::Var
            | SyntaxExprKind::Atom
            | SyntaxExprKind::Int
            | SyntaxExprKind::Float
            | SyntaxExprKind::Binary
    )
}

/// Borrowed module-call view over a syntax-output call.
struct NamedCallParts<'a> {
    remote: Option<&'a str>,
    name: &'a str,
    args: &'a [SyntaxExprOutput],
}

/// Borrowed receiver-call view over a syntax-output call.
struct ReceiverCallParts<'a> {
    receiver: &'a SyntaxExprOutput,
    name: &'a str,
    args: &'a [SyntaxExprOutput],
}

/// Returns safe named call parts for pipe linting.
fn named_call_parts<'a>(
    expr: &'a SyntaxExprOutput,
    local_call_names: &HashSet<String>,
) -> Option<NamedCallParts<'a>> {
    if expr.kind != SyntaxExprKind::Call
        || !expr.type_args.is_empty()
        || expr.arg_names.iter().any(Option::is_some)
    {
        return None;
    }

    let callee = expr.children.first()?;
    if !matches!(callee.kind, SyntaxExprKind::Atom | SyntaxExprKind::Var) {
        return None;
    }
    let name = callee.text.as_deref()?;
    if expr.remote.is_none() && !local_call_names.contains(name) {
        return None;
    }
    Some(NamedCallParts {
        remote: expr.remote.as_deref(),
        name,
        args: &expr.children[1..],
    })
}

/// Returns safe receiver-call parts for pipe linting.
fn receiver_call_parts(expr: &SyntaxExprOutput) -> Option<ReceiverCallParts<'_>> {
    if expr.kind != SyntaxExprKind::Call
        || expr.remote.is_some()
        || !expr.type_args.is_empty()
        || expr.arg_names.iter().any(Option::is_some)
        || expr.children.is_empty()
    {
        return None;
    }

    let callee = expr.children.first()?;
    if callee.kind != SyntaxExprKind::FieldAccess || callee.children.len() != 1 {
        return None;
    }
    Some(ReceiverCallParts {
        receiver: callee.children.first()?,
        name: callee.text.as_deref()?,
        args: &expr.children[1..],
    })
}

/// Renders a conservative expression subset used by safe pipe lint fixes.
fn render_expr_subset(
    expr: &SyntaxExprOutput,
    local_call_names: &HashSet<String>,
) -> Option<String> {
    match expr.kind {
        SyntaxExprKind::Var
        | SyntaxExprKind::Atom
        | SyntaxExprKind::Int
        | SyntaxExprKind::Float => expr.text.clone(),
        SyntaxExprKind::Binary => expr.text.clone(),
        SyntaxExprKind::Call => {
            if let Some(call) = named_call_parts(expr, local_call_names) {
                let args = render_args_subset(call.args, local_call_names)?;
                return Some(render_named_call(call.remote, call.name, &args));
            }
            let call = receiver_call_parts(expr)?;
            let receiver = render_expr_subset(call.receiver, local_call_names)?;
            let args = render_args_subset(call.args, local_call_names)?;
            Some(format!("{receiver}.{}({args})", call.name))
        }
        _ => None,
    }
}

/// Renders positional arguments for the safe expression subset.
fn render_args_subset(
    args: &[SyntaxExprOutput],
    local_call_names: &HashSet<String>,
) -> Option<String> {
    args.iter()
        .map(|arg| render_expr_subset(arg, local_call_names))
        .collect::<Option<Vec<_>>>()
        .map(|items| items.join(", "))
}

/// Renders one named call stage.
fn render_named_stage(
    call: &NamedCallParts<'_>,
    args: &[SyntaxExprOutput],
    local_call_names: &HashSet<String>,
) -> Option<String> {
    let rendered_args = render_args_subset(args, local_call_names)?;
    Some(render_named_call(call.remote, call.name, &rendered_args))
}

/// Renders one receiver-call stage.
fn render_receiver_stage(
    call: &ReceiverCallParts<'_>,
    local_call_names: &HashSet<String>,
) -> Option<String> {
    let rendered_args = render_args_subset(call.args, local_call_names)?;
    Some(format!("{}({rendered_args})", call.name))
}

/// Renders a local or remote named call.
fn render_named_call(remote: Option<&str>, name: &str, args: &str) -> String {
    match remote {
        Some(remote) => format!("{remote}.{name}({args})"),
        None => format!("{name}({args})"),
    }
}

/// Collects local names that are safe to treat as named function calls.
fn local_pipe_call_names(module: &SyntaxModuleOutput) -> HashSet<String> {
    let mut names = HashSet::new();
    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Function { name, .. } => {
                names.insert(name.clone());
            }
            SyntaxDeclarationPayload::Import {
                import_kind,
                items,
                is_type,
                is_selected,
                ..
            } if *import_kind == SyntaxImportKind::Module && *is_selected && !*is_type => {
                for item in items {
                    names.insert(item.as_alias.clone().unwrap_or_else(|| item.name.clone()));
                }
            }
            _ => {}
        }
    }
    names
}

/// Returns the one-based line and column for a source byte offset.
fn source_line_column_at(source: &str, index: usize) -> (usize, usize) {
    let prefix = &source[..index];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = prefix
        .rsplit_once('\n')
        .map_or(prefix.len(), |(_, tail)| tail.len())
        + 1;
    (line, column)
}

/// Returns whitespace indentation for the line containing an offset.
fn line_indent_at(source: &str, offset: usize) -> &str {
    let line_start = source[..offset].rfind('\n').map_or(0, |index| index + 1);
    let line_prefix = &source[line_start..offset];
    if line_prefix.chars().all(char::is_whitespace) {
        line_prefix
    } else {
        ""
    }
}

/// Renders pipe stages with continuation indentation.
fn render_pipe_replacement(stages: &[String], indent: &str) -> String {
    let mut rendered = stages.first().cloned().unwrap_or_default();
    for stage in stages.iter().skip(1) {
        rendered.push('\n');
        rendered.push_str(indent);
        rendered.push_str("    |> ");
        rendered.push_str(stage);
    }
    rendered
}
