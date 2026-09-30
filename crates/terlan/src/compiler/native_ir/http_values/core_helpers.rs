//! Canonical CoreIR values used by managed HTTP normalization.

use super::{CoreExpr, CoreModule};

/// Finds explicit operations in bodies and guards using the shared CoreIR walker.
pub(super) fn uses_expression(core: &CoreModule, predicate: impl Fn(&CoreExpr) -> bool) -> bool {
    core.functions
        .iter()
        .flat_map(|function| &function.clauses)
        .any(|clause| {
            clause
                .body
                .core_expr
                .iter()
                .chain(
                    clause
                        .guard
                        .iter()
                        .filter_map(|guard| guard.core_expr.as_ref()),
                )
                .any(|expression| {
                    let mut found = false;
                    super::super::application::dynamic_targets::walk_expressions(
                        expression,
                        &mut |expr| {
                            found |= predicate(expr);
                        },
                    );
                    found
                })
        })
}
