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
    let variants = as_exhaustive_union_variants(&expand_type_aliases(&expected, aliases));
    if variants.len() <= 1 {
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
        remaining.retain(|variant| !syntax_pattern_subsumes_variant(pattern, variant, aliases));
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
