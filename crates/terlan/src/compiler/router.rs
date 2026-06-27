use crate::terlan_syntax::{SyntaxExprKind, SyntaxExprOutput};

/// Extracts a router receiver-method name from a call callee.
///
/// Inputs:
/// - `callee`: first child of a syntax call expression.
///
/// Output:
/// - Router builder method name when the callee is a field access.
///
/// Transformation:
/// - Reads `router.get(...)` and `Router.new().get(...)` as source-level
///   receiver calls without requiring the full typechecker.
pub(crate) fn router_receiver_method_name(callee: &SyntaxExprOutput) -> Option<&str> {
    if callee.kind != SyntaxExprKind::FieldAccess {
        return None;
    }
    let method = callee.text.as_deref()?;
    matches!(
        method,
        "get"
            | "post"
            | "put"
            | "patch"
            | "delete"
            | "head"
            | "options"
            | "use"
            | "fallback"
            | "error"
            | "group"
    )
    .then_some(method)
}
