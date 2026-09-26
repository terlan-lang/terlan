//! Nullary Option recognition through checked control wrappers.

use crate::terlan_typeck::{CoreExpr, CoreType};

/// Reports whether an atom-form `None` is being lowered as an `Option` value.
pub(in crate::compiler::native_ir) fn is_none_option_value(
    value: &CoreExpr,
    expected: &CoreType,
) -> bool {
    is_nullary_none_value(value)
        && match expected {
            CoreType::Apply { constructor, args } => {
                constructor.rsplit('.').next() == Some("Option") && args.len() == 1
            }
            CoreType::Union(variants) => variants.iter().any(|variant| {
                matches!(variant, CoreType::AtomLiteral(name) if name == "none")
                    || matches!(
                        variant,
                        CoreType::Named(name)
                            if name.rsplit('.').next() == Some("None")
                    )
            }),
            _ => false,
        }
}

/// Recognizes control wrappers whose every selected value is the nullary
/// `None` variant. Call-region construction may retain a one-clause `if`
/// around a short-circuit bypass, so representation selection must inspect
/// the terminal values rather than only the outer node.
fn is_nullary_none_value(value: &CoreExpr) -> bool {
    match value {
        CoreExpr::Atom(name) | CoreExpr::Var(name) => name.eq_ignore_ascii_case("none"),
        CoreExpr::ConstructorCall {
            constructor, args, ..
        } => args.is_empty() && constructor.rsplit('.').next() == Some("None"),
        CoreExpr::Cast { expr, .. } => is_nullary_none_value(expr),
        CoreExpr::Let { body, .. } => is_nullary_none_value(body),
        CoreExpr::If { clauses } => {
            !clauses.is_empty()
                && clauses
                    .iter()
                    .all(|clause| is_nullary_none_value(&clause.body))
        }
        CoreExpr::Case { clauses, .. } => {
            !clauses.is_empty()
                && clauses
                    .iter()
                    .all(|clause| is_nullary_none_value(&clause.body))
        }
        _ => false,
    }
}
