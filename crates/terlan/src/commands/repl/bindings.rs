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
        .map(|binding| format!("let {} = ({})", binding.pattern, binding.value))
        .collect::<Vec<_>>()
        .join("; ");
    format!("{bindings}; {expression}")
}

/// Finds a persisted binding updated by a simple mutable receiver expression.
///
/// Inputs:
/// - `expression`: terminator-stripped REPL expression source.
/// - `value_bindings`: persisted REPL bindings.
///
/// Output:
/// - Receiver binding name when the expression has shape `name.mutator(...)`
///   and `name` is a persisted simple binding.
/// - `None` for ordinary expressions.
///
/// Transformation:
/// - Recognizes the collection mutators whose public return value is `Unit`
///   while the receiver binding is updated by compiler lowering. This keeps
///   the interactive REPL aligned with compiled mutable-receiver semantics
///   without changing Terlan expression syntax.
pub(super) fn mutable_receiver_binding_name(
    expression: &str,
    value_bindings: &[ReplValueBinding],
) -> Option<String> {
    let expression = expression.trim();
    let (receiver, rest) = expression.split_once('.')?;
    let receiver = receiver.trim();
    if !is_simple_binding_name(receiver) {
        return None;
    }
    let method = rest.trim_start().split_once('(')?.0.trim();
    if !is_known_mutator(method) {
        return None;
    }
    value_bindings
        .iter()
        .any(|binding| binding.pattern == receiver)
        .then(|| receiver.to_string())
}

/// Updates one persisted REPL binding with its latest rendered value.
///
/// Inputs:
/// - `value_bindings`: mutable REPL state.
/// - `name`: simple binding name.
/// - `value`: rendered Terlan source-facing value.
///
/// Output:
/// - `true` when a binding was updated.
///
/// Transformation:
/// - Replaces the stored source expression for the simple binding so the next
///   prompt rebuilds state from the updated value.
pub(super) fn update_repl_value_binding(
    value_bindings: &mut [ReplValueBinding],
    name: &str,
    value: String,
) -> bool {
    if let Some(binding) = value_bindings
        .iter_mut()
        .find(|binding| binding.pattern == name)
    {
        binding.value = value;
        return true;
    }
    false
}

fn is_simple_binding_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_lowercase() || first == '_') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_known_mutator(method: &str) -> bool {
    matches!(
        method,
        "add" | "clear" | "insert" | "pop" | "push" | "put" | "remove" | "set" | "set_at" | "swap"
    )
}
