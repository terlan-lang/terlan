/// Renders an Erlang atom expression.
///
/// Input is an atom name without source-level punctuation. Output is Erlang
/// atom syntax. The transformation leaves known keyword atoms bare and quotes
/// all other atoms with Erlang atom escaping.
pub(crate) fn render_atom_expr(name: &str) -> String {
    if is_atom_keyword(name) {
        name.to_string()
    } else {
        quote_erlang_atom_literal(name)
    }
}

/// Renders raw text as a quoted Erlang atom literal.
///
/// Inputs:
/// - `name`: raw atom text without source-level punctuation.
///
/// Output:
/// - Single-quoted Erlang atom literal.
///
/// Transformation:
/// - Escapes backslashes and single quotes before wrapping the result in
///   Erlang single-quoted atom syntax.
pub(crate) fn quote_erlang_atom_literal(name: &str) -> String {
    format!("'{}'", escape_quoted_atom(name))
}

/// Checks whether an atom can render without quoting.
///
/// Input is an atom name. Output is true only for the backend's small keyword
/// allowlist. The transformation is a pure membership check used before quoted
/// atom rendering.
fn is_atom_keyword(name: &str) -> bool {
    matches!(name, "true" | "false" | "ok" | "error" | "nil" | "unit")
}

/// Escapes atom text for single-quoted Erlang atom syntax.
///
/// Input is raw atom text. Output is escaped atom text without surrounding
/// quotes. The transformation escapes backslashes and single quotes.
fn escape_quoted_atom(name: &str) -> String {
    name.replace('\\', "\\\\").replace('\'', "\\'")
}
