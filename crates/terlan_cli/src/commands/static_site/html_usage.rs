use terlan_syntax::{
    SyntaxDeclarationPayload, SyntaxExprKind, SyntaxExprOutput, SyntaxHtmlAttrValueOutput,
    SyntaxHtmlNodeOutput, SyntaxModuleOutput,
};

/// Returns whether a formal syntax-output module uses static HTML constructs.
///
/// Inputs:
/// - `module`: formal syntax output module.
///
/// Output:
/// - `true` when a function or constructor body contains an HTML block or
///   template instantiation.
///
/// Transformation:
/// - Walks function and constructor clauses and delegates expression traversal
///   to `syntax_expr_uses_html`.
pub(crate) fn syntax_module_uses_html(module: &SyntaxModuleOutput) -> bool {
    module
        .declarations
        .iter()
        .any(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Function { clauses, .. } => clauses.iter().any(|clause| {
                syntax_expr_uses_html(&clause.body)
                    || clause.guard.as_ref().is_some_and(syntax_expr_uses_html)
            }),
            SyntaxDeclarationPayload::Constructor { clauses, .. } => clauses
                .iter()
                .any(|clause| syntax_expr_uses_html(&clause.body)),
            _ => false,
        })
}

/// Returns whether an expression tree uses static HTML constructs.
///
/// Inputs:
/// - `expr`: formal syntax-output expression.
///
/// Output:
/// - `true` when the expression or any nested expression/html node uses an HTML
///   block or template instantiation.
///
/// Transformation:
/// - Recursively checks expression children, fields, clauses, guards, and HTML
///   nodes.
fn syntax_expr_uses_html(expr: &SyntaxExprOutput) -> bool {
    if matches!(
        expr.kind,
        SyntaxExprKind::HtmlBlock | SyntaxExprKind::TemplateInstantiate
    ) {
        return true;
    }

    expr.children.iter().any(syntax_expr_uses_html)
        || expr
            .fields
            .iter()
            .any(|field| syntax_expr_uses_html(&field.value))
        || expr.clauses.iter().any(|clause| {
            syntax_expr_uses_html(&clause.body)
                || clause
                    .guard
                    .as_ref()
                    .is_some_and(|guard| syntax_expr_uses_html(guard))
        })
        || expr.html_nodes.iter().any(syntax_html_node_uses_html)
}

/// Returns whether an HTML syntax node contains dynamic HTML usage.
///
/// Inputs:
/// - `node`: formal syntax-output HTML node.
///
/// Output:
/// - `true` when an expression child, expression attribute, child node, or
///   named-slot child uses static HTML constructs.
///
/// Transformation:
/// - Recursively walks HTML node structure and delegates embedded expressions
///   to `syntax_expr_uses_html`.
fn syntax_html_node_uses_html(node: &SyntaxHtmlNodeOutput) -> bool {
    match node {
        SyntaxHtmlNodeOutput::Text { .. } => false,
        SyntaxHtmlNodeOutput::Expr { expr } => syntax_expr_uses_html(expr),
        SyntaxHtmlNodeOutput::Element { element } => {
            element.attrs.iter().any(|attr| {
                matches!(
                    &attr.value,
                    Some(SyntaxHtmlAttrValueOutput::Expr { expr })
                        if syntax_expr_uses_html(expr)
                )
            }) || element.children.iter().any(syntax_html_node_uses_html)
        }
        SyntaxHtmlNodeOutput::NamedSlot { slot } => {
            slot.children.iter().any(syntax_html_node_uses_html)
        }
    }
}
