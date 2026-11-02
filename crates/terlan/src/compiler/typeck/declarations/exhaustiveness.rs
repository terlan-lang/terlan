use super::*;

pub(super) fn check_syntax_function_clause_exhaustiveness(
    function_name: &str,
    first_param: Option<&str>,
    arity: usize,
    alias_names: &HashSet<String>,
    clauses: &[(Vec<SyntaxPatternOutput>, Span, bool)],
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
    let patterns = clauses
        .iter()
        .filter_map(|(patterns, _, guarded)| {
            patterns.first().map(|pattern| (pattern.clone(), *guarded))
        })
        .collect::<Vec<_>>();
    let analysis = coverage_observations::analyze(&expected, &patterns, aliases);
    coverage_observations::record(
        "function_clauses",
        clauses
            .first()
            .map(|clause| clause.1)
            .unwrap_or(Span { start: 0, end: 0 }),
        None,
        None,
        &expected,
        &analysis,
    );
    if let Some((index, message)) = &analysis.error {
        diagnostics.push(Diagnostic {
            span: clauses[*index].1,
            message: message.clone(),
            severity: DiagSeverity::Error,
        });
        return;
    }
    if analysis.exhaustive == Some(false) {
        diagnostics.push(Diagnostic {
            span: clauses[0].1,
            message: format!(
                "non-exhaustive function {}\nmissing:\n  {}",
                function_name,
                analysis
                    .remaining
                    .iter()
                    .map(pretty_type)
                    .collect::<Vec<_>>()
                    .join("\n  ")
            ),
            severity: DiagSeverity::Warning,
        });
    }
}

#[cfg(test)]
#[path = "exhaustiveness_test.rs"]
mod tests;
