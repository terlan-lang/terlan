/// Converts a syntax-output try expression into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output try expression with body, `of` clauses, `catch`
///   clauses, and optional cleanup branch.
///
/// Output:
/// - `Some(CoreExpr::Try)` when the body, every clause, and optional cleanup
///   branch lower into typed Core.
/// - `None` when the node is not try syntax or any child remains unsupported.
///
/// Transformation:
/// - Preserves try body, success clauses, catch clauses, and optional cleanup
///   branch as a backend-neutral CoreIR keyword expression.
fn core_try_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::Try) || expr.children.len() != 1 {
        return None;
    }

    Some(CoreExpr::Try {
        body: Box::new(core_expr_from_syntax(&expr.children[0])?),
        of_clauses: core_case_clauses_from_syntax(expr)?,
        catch_clauses: expr
            .catch_clauses
            .iter()
            .map(core_case_clause_from_syntax)
            .collect::<Option<Vec<_>>>()?,
        after_clause: match expr.try_after.as_ref() {
            Some(after_clause) => Some(core_try_after_from_syntax(after_clause)?),
            None => None,
        },
    })
}

/// Converts a syntax-output try cleanup branch into typed Core.
///
/// Inputs:
/// - `after_clause`: syntax-output try cleanup trigger/body payload.
///
/// Output:
/// - `Some(CoreTryAfter)` when both trigger and body lower into typed Core.
/// - `None` when either expression remains unsupported.
///
/// Transformation:
/// - Preserves cleanup trigger and body as a try-specific CoreIR branch without
///   backend cleanup semantics.
fn core_try_after_from_syntax(
    after_clause: &crate::terlan_syntax::syntax_output::SyntaxTryAfterOutput,
) -> Option<CoreTryAfter> {
    Some(CoreTryAfter {
        trigger: Box::new(core_expr_from_syntax(&after_clause.trigger)?),
        body: Box::new(core_expr_from_syntax(&after_clause.body)?),
    })
}

/// Converts a syntax-output if expression into typed Core.
///
/// Inputs:
/// - `expr`: syntax-output if expression whose clauses carry conditions in
///   `guard` and branch bodies in `body`.
///
/// Output:
/// - `Some(CoreExpr::If)` when every condition and body lowers into typed Core.
/// - `None` when the node is not an if expression, contains pattern payloads,
///   lacks a condition, or contains unsupported condition/body expressions.
///
/// Transformation:
/// - Reconstructs condition/body branches from syntax-output clauses without
///   treating them as pattern-matching case clauses.
fn core_if_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if !matches!(expr.kind, SyntaxExprKind::If) {
        return None;
    }

    expr.clauses
        .iter()
        .map(core_if_clause_from_syntax)
        .collect::<Option<Vec<_>>>()
        .map(|clauses| CoreExpr::If { clauses })
}

/// Converts one syntax-output if clause into typed Core.
///
/// Inputs:
/// - `clause`: syntax-output if clause with no patterns, condition in `guard`,
///   and branch body in `body`.
///
/// Output:
/// - `Some(CoreIfClause)` when condition and body are typed Core expressions.
/// - `None` when patterns are present, condition is missing, or either
///   expression remains unsupported.
///
/// Transformation:
/// - Lowers the condition/body pair while preserving the if-specific branch
///   shape independently from case-pattern clauses.
fn core_if_clause_from_syntax(
    clause: &crate::terlan_syntax::SyntaxClauseOutput,
) -> Option<CoreIfClause> {
    if !clause.patterns.is_empty() {
        return None;
    }
    Some(CoreIfClause {
        condition: core_expr_from_syntax(clause.guard.as_ref()?.as_ref())?,
        body: core_expr_from_syntax(&clause.body)?,
    })
}

/// Converts one syntax-output case clause into a typed Core case clause.
///
/// Inputs:
/// - `clause`: syntax-output case clause.
///
/// Output:
/// - `Some(CoreCaseClause)` for one-pattern clauses in the current typed
///   subset, including supported guarded forms.
/// - `None` for multi-pattern clauses, unsupported patterns, unsupported
///   guards, or unsupported bodies.
///
/// Transformation:
/// - Lowers the branch pattern and body without using backend syntax or
///   rendered summary text.
fn core_case_clause_from_syntax(
    clause: &crate::terlan_syntax::SyntaxClauseOutput,
) -> Option<CoreCaseClause> {
    if clause.patterns.len() != 1 {
        return None;
    }
    let guard = clause
        .guard
        .as_ref()
        .and_then(|guard| core_expr_from_syntax(guard.as_ref()));
    Some(CoreCaseClause {
        pattern: core_pattern_from_syntax(&clause.patterns[0])?,
        guard,
        body: core_expr_from_syntax(&clause.body)?,
    })
}
