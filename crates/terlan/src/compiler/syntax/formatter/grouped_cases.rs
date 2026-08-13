use super::expression_formatting::format_expr;
use super::let_else::format_let_expr;
use super::{CaseClause, Expr, Pattern};
use crate::terlan_syntax::parse_tree::LetBinding;

/// Formats a linear case pyramid with one value-independent fallback as a
/// grouped refutable binding.
///
/// The rewrite is deliberately structural: every case must have two unguarded
/// clauses, every failure body must be identical, and that body must not read a
/// variable introduced by its failure pattern. These constraints make the
/// wildcard `else` branch equivalent even when successive scrutinees have
/// different types.
pub(super) fn format_as_grouped_let(expr: &Expr, indent: usize) -> Option<String> {
    let (fallback, mut current) = initial_linear_case(expr)?;
    let fallback_text = format_expr(fallback, 0);
    let mut bindings = Vec::new();

    loop {
        let Expr::Case { scrutinee, clauses } = current else {
            return None;
        };
        let (success, failure) = split_case_clauses(clauses, &fallback_text)?;
        if failure_depends_on_pattern(failure) {
            return None;
        }
        bindings.push(LetBinding {
            pattern: success.pattern.clone(),
            value: scrutinee.as_ref().clone(),
        });

        if case_has_fallback(&success.body, &fallback_text) {
            current = &success.body;
        } else {
            if bindings.len() < 2 {
                return None;
            }
            let else_clauses = vec![CaseClause {
                pattern: Pattern::Wildcard,
                guard: None,
                body: fallback.clone(),
            }];
            return Some(format_let_expr(
                &bindings,
                &else_clauses,
                Some(&success.body),
                indent,
            ));
        }
    }
}

/// Identifies the shared failure body and first case in a linear pyramid.
fn initial_linear_case(expr: &Expr) -> Option<(&Expr, &Expr)> {
    let Expr::Case { clauses, .. } = expr else {
        return None;
    };
    if clauses.len() != 2 || clauses.iter().any(|clause| clause.guard.is_some()) {
        return None;
    }

    for nested_index in 0..2 {
        let nested = &clauses[nested_index].body;
        let fallback = &clauses[1 - nested_index].body;
        if case_has_fallback(nested, &format_expr(fallback, 0)) {
            return Some((fallback, expr));
        }
    }
    None
}

/// Splits a two-clause case into its continuing and shared-failure clauses.
fn split_case_clauses<'a>(
    clauses: &'a [CaseClause],
    fallback_text: &str,
) -> Option<(&'a CaseClause, &'a CaseClause)> {
    if clauses.len() != 2 || clauses.iter().any(|clause| clause.guard.is_some()) {
        return None;
    }
    for failure_index in 0..2 {
        if format_expr(&clauses[failure_index].body, 0) == fallback_text {
            return Some((&clauses[1 - failure_index], &clauses[failure_index]));
        }
    }
    None
}

/// Returns whether a nested case contains the shared failure body.
fn case_has_fallback(expr: &Expr, fallback_text: &str) -> bool {
    let Expr::Case { clauses, .. } = expr else {
        return false;
    };
    split_case_clauses(clauses, fallback_text).is_some()
}

/// Returns whether the failure body reads a binding owned by its pattern.
fn failure_depends_on_pattern(clause: &CaseClause) -> bool {
    let body = format_expr(&clause.body, 0);
    let mut names = Vec::new();
    collect_pattern_names(&clause.pattern, &mut names);
    names
        .into_iter()
        .any(|name| source_contains_identifier(&body, name))
}

fn collect_pattern_names<'a>(pattern: &'a Pattern, names: &mut Vec<&'a str>) {
    match pattern {
        Pattern::Var(name) => names.push(name),
        Pattern::Tuple(children) | Pattern::List(children) => {
            for child in children {
                collect_pattern_names(child, names);
            }
        }
        Pattern::Alias { alias, pattern } => {
            names.push(alias);
            collect_pattern_names(pattern, names);
        }
        Pattern::ListCons(head, tail) => {
            collect_pattern_names(head, names);
            collect_pattern_names(tail, names);
        }
        Pattern::Map(fields) | Pattern::Record { fields, .. } => {
            for field in fields {
                collect_pattern_names(&field.value, names);
            }
        }
        Pattern::StringSegments(segments) => {
            for segment in segments {
                if let super::StringPatternSegment::Capture(capture) = segment {
                    names.push(&capture.name);
                }
            }
        }
        Pattern::Wildcard
        | Pattern::Int(_)
        | Pattern::Float(_)
        | Pattern::String(_)
        | Pattern::Atom(_)
        | Pattern::AtomLiteral(_)
        | Pattern::NullaryConstructorCall(_)
        | Pattern::BinaryLayout { .. } => {}
    }
}

/// Finds an identifier token without treating substrings as variable reads.
fn source_contains_identifier(source: &str, expected: &str) -> bool {
    source
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .any(|token| token == expected)
}
