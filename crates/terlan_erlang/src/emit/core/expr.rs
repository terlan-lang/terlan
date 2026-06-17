//! CoreIR expression lowering for CoreIR Erlang emission.
//!
//! Inputs:
//! - Backend-neutral CoreIR expression trees from the formal compiler path.
//!
//! Outputs:
//! - Erlang AST expressions for the currently supported CoreIR expression
//!   subset.
//!
//! Transformations:
//! - Converts literals, calls, operators, lambdas, list comprehensions, and
//!   intrinsic calls into the Erlang emitter model without consulting source
//!   syntax.

use terlan_typeck::CoreExpr;

use super::super::erl::{ErlExpr, ErlFunctionClause};
use super::super::{
    lower_syntax_binary_op, lower_syntax_unary_op, sanitize_erlang_fn_name, sanitize_erlang_var,
};
use super::{
    erl_remote_call, lower_core_intrinsic_call_to_erlang, lower_core_pattern_to_erlang,
    lower_core_patterns_to_erlang,
};

/// Lowers a supported backend-neutral CoreIR expression into an Erlang AST expression.
///
/// Inputs:
/// - `expr`: CoreIR expression produced after syntax-output lowering.
///
/// Output:
/// - `Some(ErlExpr)` when the expression belongs to the currently supported
///   Erlang CoreIR backend subset.
/// - `None` when the expression still needs a dedicated backend lowering rule.
///
/// Transformation:
/// - Maps backend-neutral CoreIR literals, calls, remote calls, operators,
///   tuples, lists, list cons cells, list comprehensions, fixed arrays, and
///   primitive intrinsics into the emitter's Erlang expression model without
///   consulting source syntax.
#[allow(dead_code)]
pub(in crate::emit) fn lower_core_expr_to_erlang(expr: &CoreExpr) -> Option<ErlExpr> {
    match expr {
        CoreExpr::Int(value) => Some(ErlExpr::Int(*value)),
        CoreExpr::Float(value) => Some(ErlExpr::Float(value.clone())),
        CoreExpr::Binary(value) => Some(ErlExpr::Binary(value.clone())),
        CoreExpr::Atom(name) => Some(ErlExpr::Atom(name.clone())),
        CoreExpr::Var(name) => Some(ErlExpr::Var(sanitize_erlang_var(name))),
        CoreExpr::Tuple(items) => lower_core_exprs_to_erlang(items).map(ErlExpr::Tuple),
        CoreExpr::List(items) => lower_core_exprs_to_erlang(items).map(ErlExpr::List),
        CoreExpr::FixedArray(items) => lower_core_exprs_to_erlang(items).map(ErlExpr::FixedArray),
        CoreExpr::ListCons { head, tail } => Some(ErlExpr::ListCons(
            Box::new(lower_core_expr_to_erlang(head)?),
            Box::new(lower_core_expr_to_erlang(tail)?),
        )),
        CoreExpr::ListComprehension {
            expr,
            pattern,
            source,
            guard,
        } => Some(ErlExpr::ListComprehension {
            expr: Box::new(lower_core_expr_to_erlang(expr)?),
            pattern: lower_core_pattern_to_erlang(pattern)?,
            source: Box::new(lower_core_expr_to_erlang(source)?),
            guard: match guard.as_deref() {
                Some(guard) => Some(Box::new(lower_core_expr_to_erlang(guard)?)),
                None => None,
            },
        }),
        CoreExpr::Call { function, args } if !is_index_trait_core_call(function) => {
            Some(ErlExpr::Call {
                module: None,
                function: sanitize_erlang_fn_name(function),
                args: lower_core_exprs_to_erlang(args)?,
            })
        }
        CoreExpr::FunctionCall { callee, args } => Some(ErlExpr::Apply {
            callee: Box::new(lower_core_expr_to_erlang(callee)?),
            args: lower_core_exprs_to_erlang(args)?,
        }),
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } => Some(erl_remote_call(
            module,
            &sanitize_erlang_fn_name(function),
            lower_core_exprs_to_erlang(args)?,
        )),
        CoreExpr::UnaryOp { operator, operand } => Some(ErlExpr::UnaryOp {
            op: lower_syntax_unary_op(Some(operator)),
            expr: Box::new(lower_core_expr_to_erlang(operand)?),
        }),
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => Some(ErlExpr::BinaryOp {
            op: lower_syntax_binary_op(Some(operator)),
            left: Box::new(lower_core_expr_to_erlang(left)?),
            right: Box::new(lower_core_expr_to_erlang(right)?),
        }),
        CoreExpr::Intrinsic(call) => lower_core_intrinsic_call_to_erlang(call),
        CoreExpr::Call { .. }
        | CoreExpr::Index { .. }
        | CoreExpr::Let { .. }
        | CoreExpr::Map(_)
        | CoreExpr::RecordConstruct { .. }
        | CoreExpr::FieldAccess { .. }
        | CoreExpr::RecordAccess { .. }
        | CoreExpr::RecordUpdate { .. }
        | CoreExpr::TemplateInstantiate { .. }
        | CoreExpr::ConstructorChain { .. }
        | CoreExpr::RemoteFunRef { .. }
        | CoreExpr::ConstructorCall { .. }
        | CoreExpr::MutableReceiverCall { .. }
        | CoreExpr::Case { .. }
        | CoreExpr::Try { .. }
        | CoreExpr::If { .. } => None,
        CoreExpr::Lam { params, body } => Some(ErlExpr::Fun(vec![ErlFunctionClause {
            patterns: lower_core_patterns_to_erlang(params)?,
            guard: None,
            body: lower_core_expr_to_erlang(body)?,
        }])),
    }
}

/// Returns whether a Core call is a canonical indexed trait dispatch.
///
/// Inputs:
/// - `function`: Core function identity from `CoreExpr::Call`.
///
/// Output:
/// - `true` for compiler-owned `IndexGet.get_at` and `IndexSet.set_at`
///   lowering targets.
/// - `false` for ordinary local function calls.
///
/// Transformation:
/// - Separates trait-backed bracket reads and writes from ordinary calls so the
///   Core Erlang backend cannot accidentally emit local placeholder functions
///   before it has module-aware trait-wrapper dispatch.
fn is_index_trait_core_call(function: &str) -> bool {
    matches!(function, "IndexGet.get_at" | "IndexSet.set_at")
}

/// Lowers a list of CoreIR expressions into Erlang expressions.
///
/// Inputs:
/// - `args`: CoreIR expression slice to lower in order.
///
/// Output:
/// - `Some(Vec<ErlExpr>)` when every expression is supported by the current
///   Erlang CoreIR backend subset.
/// - `None` when any expression is outside the current subset.
///
/// Transformation:
/// - Applies `lower_core_expr_to_erlang` element-wise and preserves argument
///   order for call and intrinsic lowering.
pub(super) fn lower_core_exprs_to_erlang(args: &[CoreExpr]) -> Option<Vec<ErlExpr>> {
    args.iter().map(lower_core_expr_to_erlang).collect()
}
