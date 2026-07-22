use crate::terlan_syntax::SyntaxExprOutput;

/// Renders a constant expression using its preserved source text.
pub(crate) fn render_const_expr_text(expr: &SyntaxExprOutput) -> String {
    expr.raw
        .clone()
        .or_else(|| expr.text.clone())
        .unwrap_or_else(|| format!("{:?}", expr.kind))
}
