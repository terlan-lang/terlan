//! Preserve explicit lambda contracts at the syntax-to-Core boundary.

use super::*;

/// Lowers one unguarded lambda without dropping or misaligning annotations.
pub(super) fn lower(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    let clause = expr.clauses.first()?;
    if clause.guard.is_some()
        || (!clause.parameter_types.is_empty()
            && clause.parameter_types.len() != clause.patterns.len())
    {
        return None;
    }
    Some(CoreExpr::Lam {
        params: core_patterns_from_syntax_slice(&clause.patterns)?,
        parameter_types: clause
            .parameter_types
            .iter()
            .map(|annotation| match annotation {
                Some(annotation) => core_type_from_text(&annotation.text).map(Some),
                None => Some(None),
            })
            .collect::<Option<Vec<_>>>()?,
        body: Box::new(core_expr_from_syntax(&clause.body)?),
    })
}

#[cfg(test)]
#[path = "lambda_test.rs"]
mod tests;
