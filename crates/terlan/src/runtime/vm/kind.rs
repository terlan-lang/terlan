use crate::terlan_typeck::{CoreExpr, CorePattern};

/// Returns a compact name for an unsupported CoreIR expression.
///
/// Inputs:
/// - `expr`: unsupported CoreIR expression.
///
/// Output:
/// - Stable variant-like label for diagnostics.
///
/// Transformation:
/// - Maps broad CoreIR shapes to readable evaluator diagnostics.
pub(super) fn core_expr_kind(expr: &CoreExpr) -> &'static str {
    match expr {
        CoreExpr::Int(_) => "Int",
        CoreExpr::Float(_) => "Float",
        CoreExpr::Binary(_) => "Binary",
        CoreExpr::Atom(_) => "Atom",
        CoreExpr::Var(_) => "Var",
        CoreExpr::Tuple(_) => "Tuple",
        CoreExpr::List(_) => "List",
        CoreExpr::ListCons { .. } => "ListCons",
        CoreExpr::FixedArray(_) => "FixedArray",
        CoreExpr::Index { .. } => "Index",
        CoreExpr::ListComprehension { .. } => "ListComprehension",
        CoreExpr::Let { .. } => "Let",
        CoreExpr::Map(_) => "Map",
        CoreExpr::RecordConstruct { .. } => "RecordConstruct",
        CoreExpr::FieldAccess { .. } => "FieldAccess",
        CoreExpr::RecordAccess { .. } => "RecordAccess",
        CoreExpr::RecordUpdate { .. } => "RecordUpdate",
        CoreExpr::TemplateInstantiate { .. } => "TemplateInstantiate",
        CoreExpr::ConstructorChain { .. } => "ConstructorChain",
        CoreExpr::RemoteFunRef { .. } => "RemoteFunRef",
        CoreExpr::RemoteCall { .. } => "RemoteCall",
        CoreExpr::ConstructorCall { .. } => "ConstructorCall",
        CoreExpr::Call { .. } => "Call",
        CoreExpr::MutableReceiverCall { .. } => "MutableReceiverCall",
        CoreExpr::FunctionCall { .. } => "FunctionCall",
        CoreExpr::Cast { .. } => "Cast",
        CoreExpr::Intrinsic(_) => "Intrinsic",
        CoreExpr::SqlQuery { .. } => "SqlQuery",
        CoreExpr::Case { .. } => "Case",
        CoreExpr::Try { .. } => "Try",
        CoreExpr::If { .. } => "If",
        CoreExpr::Lam { .. } => "Lam",
        CoreExpr::UnaryOp { .. } => "UnaryOp",
        CoreExpr::BinaryOp { .. } => "BinaryOp",
    }
}

/// Returns a compact name for an unsupported CoreIR pattern.
///
/// Inputs:
/// - `pattern`: unsupported function-call pattern.
///
/// Output:
/// - Stable variant-like label for diagnostics.
///
/// Transformation:
/// - Maps broad pattern shapes to readable evaluator diagnostics.
pub(super) fn core_pattern_kind(pattern: &CorePattern) -> &'static str {
    match pattern {
        CorePattern::Wildcard => "Wildcard",
        CorePattern::Var(_) => "Var",
        CorePattern::Int(_) => "Int",
        CorePattern::Float(_) => "Float",
        CorePattern::Atom(_) => "Atom",
        CorePattern::Tuple(_) => "Tuple",
        CorePattern::List(_) => "List",
        CorePattern::ListCons { .. } => "ListCons",
        CorePattern::Map(_) => "Map",
        CorePattern::Record { .. } => "Record",
        CorePattern::Constructor { .. } => "Constructor",
    }
}
