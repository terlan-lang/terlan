use crate::terlan_syntax::parse_tree::{Expr, ListComprehensionGenerator};

use super::{format_assignment_child, format_pattern};

/// Formats one list comprehension with canonical generator and guard spacing.
pub(super) fn format_list_comprehension(
    expr: &Expr,
    generators: &[ListComprehensionGenerator],
    guards: &[Expr],
) -> String {
    let generators = generators
        .iter()
        .map(|generator| {
            format!(
                "{} <- {}",
                format_pattern(&generator.pattern),
                format_assignment_child(&generator.source, 0)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    let value = format_assignment_child(expr, 0);
    if guards.is_empty() {
        format!("[{value} | {generators}]")
    } else {
        let guards = guards
            .iter()
            .map(|guard| format_assignment_child(guard, 0))
            .collect::<Vec<_>>()
            .join(", ");
        format!("[{value} | {generators}, {guards}]")
    }
}
