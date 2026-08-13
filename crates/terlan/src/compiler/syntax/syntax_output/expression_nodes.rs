use super::*;

/// Converts binary-expression trees without consuming the native stack.
pub(super) fn binary_expr_output_with_span(expr: &Expr, span: EbnfSourceSpan) -> SyntaxExprOutput {
    let mut pending = vec![(expr, false)];
    let mut outputs = Vec::new();
    while let Some((current, visited)) = pending.pop() {
        let Expr::BinaryOp { op, left, right } = current else {
            outputs.push(expr_output_with_span(current, span));
            continue;
        };
        if !visited {
            pending.push((current, true));
            pending.push((right, false));
            pending.push((left, false));
            continue;
        }
        let right = outputs.pop().expect("binary right output");
        let left = outputs.pop().expect("binary left output");
        outputs.push(expr_node!(
            SyntaxExprKind::BinaryOp,
            None,
            Some(binary_op_text(op).to_string()),
            None,
            vec![left, right],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            span,
        ));
    }
    outputs.pop().expect("binary expression output")
}

/// Builds a leaf expression node with no raw spelling override.
///
/// Inputs: expression kind, optional text, and span. Output: syntax-output leaf
/// expression. Transformation: delegates to the general node builder with empty
/// child/pattern/field/clause collections.
pub(super) fn expr_leaf_with_span(
    kind: SyntaxExprKind,
    text: Option<String>,
    span: EbnfSourceSpan,
) -> SyntaxExprOutput {
    expr_node!(
        kind,
        text,
        None,
        None,
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        span,
    )
}

/// Builds a leaf expression node while preserving raw source spelling.
///
/// Inputs:
/// - `kind`: syntax-output expression kind.
/// - `text`: normalized expression payload.
/// - `raw`: canonical source spelling that should survive the syntax boundary.
/// - `span`: source span for diagnostics.
///
/// Output:
/// - A `SyntaxExprOutput` leaf with no children and the supplied raw payload.
///
/// Transformation:
/// - Starts from the standard expression-node shape and overrides only `raw`
///   so downstream phases can distinguish explicit source forms that share the
///   same semantic kind.
pub(super) fn expr_leaf_with_span_and_raw(
    kind: SyntaxExprKind,
    text: Option<String>,
    raw: Option<String>,
    span: EbnfSourceSpan,
) -> SyntaxExprOutput {
    let mut output = expr_leaf_with_span(kind, text, span);
    output.raw = raw;
    output
}

/// Renders the canonical raw syntax for an atom literal expression.
///
/// Inputs:
/// - `payload`: unescaped atom payload text.
///
/// Output:
/// - Canonical `Atom["..."]` source spelling.
///
/// Transformation:
/// - Escapes only the characters that need stable representation inside a
///   normal Terlan string literal.
pub(super) fn format_canonical_atom_literal_raw(payload: &str) -> String {
    let escaped = payload
        .chars()
        .flat_map(|ch| match ch {
            '"' => "\\\"".chars().collect::<Vec<_>>(),
            '\\' => "\\\\".chars().collect::<Vec<_>>(),
            '\n' => "\\n".chars().collect::<Vec<_>>(),
            '\r' => "\\r".chars().collect::<Vec<_>>(),
            '\t' => "\\t".chars().collect::<Vec<_>>(),
            other => vec![other],
        })
        .collect::<String>();
    format!("Atom[\"{escaped}\"]")
}

/// Computes default expression-node arity.
///
/// Inputs: child, pattern, field, and clause collections. Output: maximum
/// collection length. Transformation: treats the widest structural collection
/// as the node's default arity.
pub(crate) fn node_arity(
    children: &[SyntaxExprOutput],
    patterns: &[SyntaxPatternOutput],
    fields: &[SyntaxExprFieldOutput],
    clauses: &[SyntaxClauseOutput],
) -> usize {
    fields
        .len()
        .max(clauses.len())
        .max(patterns.len())
        .max(children.len())
}

/// Extension trait for overriding syntax-output expression arity.
///
/// Inputs: expression output and explicit arity. Output: expression output with
/// replaced arity. Transformation: supports source forms where semantic arity
/// differs from the widest child collection.
pub(super) trait SyntaxExprArity {
    /// Overrides expression arity.
    ///
    /// Inputs: expression output and new arity. Output: updated expression.
    /// Transformation: replaces only the `arity` field.
    fn with_arity(self, arity: usize) -> Self;
}

impl SyntaxExprArity for SyntaxExprOutput {
    /// Overrides expression arity.
    ///
    /// Inputs: expression output and new arity. Output: updated expression.
    /// Transformation: mutates only the `arity` field before returning `self`.
    fn with_arity(mut self, arity: usize) -> Self {
        self.arity = arity;
        self
    }
}
