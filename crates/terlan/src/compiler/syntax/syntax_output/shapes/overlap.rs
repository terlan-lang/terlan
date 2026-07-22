use std::collections::BTreeSet;

use super::{
    EbnfCompileError, EbnfCompileResult, GuardPredicateDefinitions, SyntaxClauseOutput,
    SyntaxExprOutput, SyntaxFunctionClauseOutput, SyntaxPatternKind, SyntaxPatternOutput,
};

mod guard_implication;
use guard_implication::{guard_relation, GuardRelation};

struct ClausePattern<'a> {
    patterns: &'a [SyntaxPatternOutput],
    guarded: bool,
    guard: Option<&'a SyntaxExprOutput>,
    origins: &'a BTreeSet<String>,
}

pub(super) fn validate_function_clause_overlap(
    clauses: &[SyntaxFunctionClauseOutput],
    origins: &[BTreeSet<String>],
    guard_predicates: &GuardPredicateDefinitions,
) -> EbnfCompileResult<()> {
    let clauses = clauses
        .iter()
        .zip(origins)
        .map(|(clause, origins)| ClausePattern {
            patterns: &clause.patterns,
            guarded: clause.has_guard,
            guard: clause.guard.as_ref(),
            origins,
        })
        .collect::<Vec<_>>();
    validate_distinct_shape_overlap(&clauses, guard_predicates)
}

pub(super) fn validate_expr_clause_overlap(
    clauses: &[SyntaxClauseOutput],
    origins: &[BTreeSet<String>],
    guard_predicates: &GuardPredicateDefinitions,
) -> EbnfCompileResult<()> {
    let clauses = clauses
        .iter()
        .zip(origins)
        .map(|(clause, origins)| ClausePattern {
            patterns: &clause.patterns,
            guarded: clause.guard.is_some(),
            guard: clause.guard.as_deref(),
            origins,
        })
        .collect::<Vec<_>>();
    validate_distinct_shape_overlap(&clauses, guard_predicates)
}

fn validate_distinct_shape_overlap(
    clauses: &[ClausePattern<'_>],
    guard_predicates: &GuardPredicateDefinitions,
) -> EbnfCompileResult<()> {
    for (index, left) in clauses.iter().enumerate() {
        if left.origins.is_empty() {
            continue;
        }
        let left_pattern = canonical_patterns(left.patterns);
        for right in clauses.iter().skip(index + 1) {
            if right.origins.is_empty() || left.origins == right.origins {
                continue;
            }
            if left.guarded {
                if right.guarded && patterns_subsume(left.patterns, right.patterns) {
                    match guard_relation(
                        left.guard,
                        left.patterns,
                        right.guard,
                        right.patterns,
                        guard_predicates,
                    ) {
                        GuardRelation::Equivalent => {
                            return Err(EbnfCompileError::Serialize(format!(
                                "unreachable shape expansion: earlier alias `{}` subsumes later alias `{}` with an equivalent guard",
                                origin_label(left.origins),
                                origin_label(right.origins)
                            )));
                        }
                        GuardRelation::Implied => {
                            return Err(EbnfCompileError::Serialize(format!(
                                "unreachable shape expansion: later guard for alias `{}` implies the earlier guard for subsuming alias `{}`",
                                origin_label(right.origins),
                                origin_label(left.origins)
                            )));
                        }
                        GuardRelation::Unknown => {}
                    }
                }
                continue;
            }
            if !right.guarded && left_pattern == canonical_patterns(right.patterns) {
                return Err(EbnfCompileError::Serialize(format!(
                    "ambiguous shape expansion: distinct aliases `{}` and `{}` produce equivalent unguarded clause patterns",
                    origin_label(left.origins),
                    origin_label(right.origins)
                )));
            }
            if patterns_subsume(left.patterns, right.patterns) {
                return Err(EbnfCompileError::Serialize(format!(
                    "unreachable shape expansion: earlier alias `{}` subsumes later alias `{}`",
                    origin_label(left.origins),
                    origin_label(right.origins)
                )));
            }
        }
    }
    Ok(())
}

fn patterns_subsume(left: &[SyntaxPatternOutput], right: &[SyntaxPatternOutput]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| pattern_subsumes(left, right))
}

fn pattern_subsumes(left: &SyntaxPatternOutput, right: &SyntaxPatternOutput) -> bool {
    if left.kind == SyntaxPatternKind::Alias {
        return left
            .children
            .first()
            .is_some_and(|left| pattern_subsumes(left, right));
    }
    if right.kind == SyntaxPatternKind::Alias {
        return right
            .children
            .first()
            .is_some_and(|right| pattern_subsumes(left, right));
    }
    if is_binding_pattern(left.kind) {
        return true;
    }
    if is_binding_pattern(right.kind) || left.kind != right.kind {
        return false;
    }
    match left.kind {
        SyntaxPatternKind::Int
        | SyntaxPatternKind::Float
        | SyntaxPatternKind::String
        | SyntaxPatternKind::Atom => left.text == right.text,
        SyntaxPatternKind::Tuple
        | SyntaxPatternKind::List
        | SyntaxPatternKind::ListCons
        | SyntaxPatternKind::Constructor => {
            left.text == right.text && patterns_subsume(&left.children, &right.children)
        }
        SyntaxPatternKind::Map | SyntaxPatternKind::Record => {
            left.text == right.text && fields_subsume(left, right)
        }
        SyntaxPatternKind::StringPattern
        | SyntaxPatternKind::StringCapture
        | SyntaxPatternKind::MapField
        | SyntaxPatternKind::BinaryLayout => canonical_pattern(left) == canonical_pattern(right),
        SyntaxPatternKind::Wildcard
        | SyntaxPatternKind::Var
        | SyntaxPatternKind::Alias
        | SyntaxPatternKind::Ignore
        | SyntaxPatternKind::Placeholder => true,
    }
}

fn is_binding_pattern(kind: SyntaxPatternKind) -> bool {
    matches!(
        kind,
        SyntaxPatternKind::Wildcard
            | SyntaxPatternKind::Var
            | SyntaxPatternKind::Ignore
            | SyntaxPatternKind::Placeholder
    )
}

fn fields_subsume(left: &SyntaxPatternOutput, right: &SyntaxPatternOutput) -> bool {
    left.fields.iter().all(|left_field| {
        right.fields.iter().any(|right_field| {
            left_field.key == right_field.key
                && left_field.required == right_field.required
                && pattern_subsumes(&left_field.value, &right_field.value)
        })
    })
}

fn origin_label(origins: &BTreeSet<String>) -> String {
    origins.iter().cloned().collect::<Vec<_>>().join(" + ")
}

fn canonical_patterns(patterns: &[SyntaxPatternOutput]) -> String {
    patterns
        .iter()
        .map(canonical_pattern)
        .collect::<Vec<_>>()
        .join("|")
}

fn canonical_pattern(pattern: &SyntaxPatternOutput) -> String {
    if pattern.kind == SyntaxPatternKind::Alias {
        return pattern
            .children
            .first()
            .map(canonical_pattern)
            .unwrap_or_else(|| "binding".to_string());
    }
    let text = match pattern.kind {
        SyntaxPatternKind::Var
        | SyntaxPatternKind::Wildcard
        | SyntaxPatternKind::Ignore
        | SyntaxPatternKind::Placeholder => "binding".to_string(),
        SyntaxPatternKind::StringCapture => pattern
            .text
            .as_deref()
            .and_then(|text| text.split_once(':').map(|(_, ty)| ty.trim()))
            .map_or_else(|| "capture".to_string(), |ty| format!("capture:{ty}")),
        SyntaxPatternKind::StringPattern => {
            canonical_string_pattern(pattern.text.as_deref().unwrap_or_default())
        }
        _ => pattern.text.clone().unwrap_or_default(),
    };
    let children = pattern
        .children
        .iter()
        .map(canonical_pattern)
        .collect::<Vec<_>>()
        .join(",");
    let mut fields = pattern
        .fields
        .iter()
        .map(|field| {
            format!(
                "{}:{}:{}",
                field.key,
                field.required,
                canonical_pattern(&field.value)
            )
        })
        .collect::<Vec<_>>();
    fields.sort();
    format!(
        "{:?}({text})[{children}]{{{}}}",
        pattern.kind,
        fields.join(",")
    )
}

fn canonical_string_pattern(text: &str) -> String {
    let mut output = String::new();
    let mut rest = text;
    while let Some(start) = rest.find("${") {
        output.push_str(&rest[..start]);
        let slot = &rest[start + 2..];
        let Some(end) = slot.find('}') else {
            output.push_str(&rest[start..]);
            return output;
        };
        let annotation = slot[..end]
            .split_once(':')
            .map(|(_, ty)| ty.trim())
            .unwrap_or("String");
        output.push_str("${_:");
        output.push_str(annotation);
        output.push('}');
        rest = &slot[end + 1..];
    }
    output.push_str(rest);
    output
}
