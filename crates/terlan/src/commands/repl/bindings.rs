/// One persistent value binding entered in the REPL.
///
/// Inputs:
/// - Constructed from `let pattern = expr.` REPL entries.
///
/// Output:
/// - Binding pattern and source expression used to rebuild later REPL entries.
///
/// Transformation:
/// - Keeps user-entered source available so each later expression can go
///   through the normal parser, typechecker, and CoreIR lowering path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ReplValueBinding {
    pub(super) pattern: String,
    pub(super) value: String,
}

/// Parses the REPL-only persistent value binding form.
///
/// Inputs:
/// - `entry`: terminator-stripped REPL source entry.
///
/// Output:
/// - Parsed binding when the entry has shape `let name = expr`.
/// - `None` for ordinary Terlan expressions/declarations.
///
/// Transformation:
/// - Recognizes a single pattern binding without treating full source `let`
///   expressions as declarations. The right-hand expression is validated later
///   through the formal compiler path together with the pattern before the
///   binding is persisted.
pub(super) fn parse_repl_value_binding(entry: &str) -> Option<ReplValueBinding> {
    let rest = entry.trim().strip_prefix("let ")?;
    if rest.contains(';') {
        return None;
    }
    let (pattern, value) = rest.split_once('=')?;
    let pattern = pattern.trim();
    if pattern.is_empty() {
        return None;
    }
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    Some(ReplValueBinding {
        pattern: pattern.to_string(),
        value: value.to_string(),
    })
}

/// Builds the generated expression body for one REPL evaluation.
///
/// Inputs:
/// - `expression`: current expression source.
/// - `value_bindings`: persisted REPL value bindings.
///
/// Output:
/// - Source expression that evaluates previous bindings before the current
///   expression.
///
/// Transformation:
/// - Converts REPL state into an ordinary Terlan `let` expression so parsing,
///   typechecking, CoreIR lowering, and evaluation stay on the normal compiler
///   path.
pub(super) fn repl_expression_with_bindings(
    expression: &str,
    value_bindings: &[ReplValueBinding],
) -> String {
    if value_bindings.is_empty() {
        return expression.to_string();
    }

    let bindings = value_bindings
        .iter()
        .map(|binding| format!("{} = ({})", binding.pattern, binding.value))
        .collect::<Vec<_>>()
        .join("; ");
    format!("let {bindings}; {expression}")
}
