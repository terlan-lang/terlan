use super::*;

pub(super) fn check_syntax_function_clause_exhaustiveness(
    function_name: &str,
    first_param: Option<&str>,
    arity: usize,
    alias_names: &HashSet<String>,
    clauses: &[(Vec<SyntaxPatternOutput>, Span)],
    aliases: &HashMap<String, TypeAlias>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if arity != 1 {
        return;
    }
    let Some(first_param_annotation) = first_param else {
        return;
    };
    let mut vars = HashMap::new();
    let mut next_var: TypeVarId = 0;
    let expected = parse_type_expr(
        first_param_annotation,
        alias_names,
        &mut vars,
        &mut next_var,
    )
    .unwrap_or(Type::Dynamic);
    let expanded = expand_type_aliases(&expected, aliases);
    let variants = as_exhaustive_union_variants(&expanded);
    if variants.len() <= 1 && !has_finite_fields(&expanded) {
        return;
    }
    let mut remaining = variants;
    for (patterns, span) in clauses {
        if patterns.is_empty() {
            continue;
        }
        let pattern = &patterns[0];
        if matches!(
            pattern.kind,
            SyntaxPatternKind::Wildcard
                | SyntaxPatternKind::Ignore
                | SyntaxPatternKind::Placeholder
                | SyntaxPatternKind::Var
        ) {
            return;
        }
        remaining = match subtract_finite_pattern(remaining, pattern, aliases) {
            Ok(remaining) => remaining,
            Err(message) => {
                diagnostics.push(Diagnostic {
                    span: *span,
                    message: message.to_string(),
                    severity: DiagSeverity::Error,
                });
                return;
            }
        };
        if remaining.is_empty() {
            return;
        }
        if patterns.len() > 1 {
            let _ = span;
        }
    }
    if !remaining.is_empty() {
        diagnostics.push(Diagnostic {
            span: clauses[0].1,
            message: format!(
                "non-exhaustive function {}\nmissing:\n  {}",
                function_name,
                remaining
                    .iter()
                    .map(pretty_type)
                    .collect::<Vec<_>>()
                    .join("\n  ")
            ),
            severity: DiagSeverity::Warning,
        });
    }
}
