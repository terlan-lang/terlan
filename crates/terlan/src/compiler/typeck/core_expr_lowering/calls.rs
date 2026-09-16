//! Named, remote and function-value call lowering with explicit type arguments.

use super::*;

/// Lowers a resolved remote function or imported constructor into typed CoreIR,
/// retaining canonical constructor identity for uppercase imported callees.
pub(super) fn core_remote_call_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    let module = expr.remote.clone()?;
    let (callee, args) = expr.children.split_first()?;
    let function = match core_expr_from_syntax(callee)? {
        CoreExpr::Atom(function) | CoreExpr::Var(function) => function,
        _ => return None,
    };
    let args = args
        .iter()
        .map(core_expr_from_syntax)
        .collect::<Option<Vec<_>>>()?;
    if starts_with_ascii_uppercase(&function) {
        let identity = format!("{module}.{function}");
        Some(CoreExpr::ConstructorCall {
            constructor: function,
            constructor_identity: Some(identity),
            args,
        })
    } else {
        Some(CoreExpr::RemoteCall {
            type_args: expr
                .type_args
                .iter()
                .map(|ty| core_type_from_text(&ty.text))
                .collect::<Option<Vec<_>>>()?,
            module,
            function,
            args,
        })
    }
}

/// Converts a syntax-output named call into a typed Core call candidate.
///
/// Inputs:
/// - `expr`: syntax-output `Call` expression with no remote target.
///
/// Output:
/// - `Some(CoreExpr::Call)` when the callee is a lowercase local function name
///   and all arguments lower to typed Core expressions.
/// - `Some(CoreExpr::ConstructorCall)` when the callee is an uppercase
///   constructor-like name and all arguments lower to typed Core expressions.
/// - `None` for non-name callees, empty call payloads, remote calls, or
///   unsupported argument expressions.
///
/// Transformation:
/// - Preserves lowercase function calls and uppercase constructor-call
///   candidates as separate backend-neutral CoreIR nodes without resolving
///   constructor eligibility.
pub(super) fn core_named_call_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if expr.kind != SyntaxExprKind::Call || expr.remote.is_some() {
        return None;
    }

    let (callee, args) = expr.children.split_first()?;
    let name = match callee.kind {
        SyntaxExprKind::Var | SyntaxExprKind::Atom => callee.text.clone()?,
        _ => return None,
    };
    let args = args
        .iter()
        .map(core_expr_from_syntax)
        .collect::<Option<Vec<_>>>()?;

    if starts_with_ascii_lowercase(&name) {
        Some(CoreExpr::Call {
            type_args: expr
                .type_args
                .iter()
                .map(|ty| core_type_from_text(&ty.text))
                .collect::<Option<Vec<_>>>()?,
            function: name,
            args,
        })
    } else if starts_with_ascii_uppercase(&name) {
        Some(CoreExpr::ConstructorCall {
            constructor: name,
            constructor_identity: None,
            args,
        })
    } else {
        None
    }
}

/// Converts a syntax-output function-value invocation into typed CoreIR.
///
/// Inputs:
/// - `expr`: syntax-output `FunctionCall` expression created from `callee(args)`.
///
/// Output:
/// - `Some(CoreExpr::FunctionCall)` when the callee and every argument are
///   representable in the current typed Core subset.
/// - `None` for malformed function-call payloads or unsupported child
///   expressions.
///
/// Transformation:
/// - Preserves the callable expression separately from named calls so later
///   target profiles and backends can distinguish `f(x)` from `f(x)`.
pub(super) fn core_function_call_expr_from_syntax(expr: &SyntaxExprOutput) -> Option<CoreExpr> {
    if expr.kind != SyntaxExprKind::FunctionCall || expr.remote.is_some() {
        return None;
    }

    let (callee, args) = expr.children.split_first()?;
    Some(CoreExpr::FunctionCall {
        callee: Box::new(core_expr_from_syntax(callee)?),
        args: args
            .iter()
            .map(core_expr_from_syntax)
            .collect::<Option<Vec<_>>>()?,
    })
}
