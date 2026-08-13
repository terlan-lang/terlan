use crate::terlan_typeck::{CoreExpr, CorePattern};

pub(crate) fn expr_is_scalar(expr: &CoreExpr) -> bool {
    if super::super::template_values::managed_template_operation_type(expr).is_some() {
        let CoreExpr::RemoteCall { args, .. } = expr else {
            unreachable!("managed template operations are remote calls");
        };
        return args.iter().all(expr_is_scalar);
    }
    if super::super::http_values::managed_http_operation_type(expr).is_some() {
        let CoreExpr::RemoteCall { args, .. } = expr else {
            unreachable!("managed HTTP operations are remote calls");
        };
        return args.iter().all(expr_is_scalar);
    }
    match expr {
        CoreExpr::Int(_) | CoreExpr::Float(_) | CoreExpr::Binary(_) | CoreExpr::Var(_) => true,
        CoreExpr::Atom(_) => true,
        CoreExpr::Call { args, .. } | CoreExpr::ConstructorCall { args, .. } => {
            args.iter().all(expr_is_scalar)
        }
        CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            !super::super::transitions::is_process_transition(expr)
                && args.iter().all(expr_is_scalar)
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) => items.iter().all(expr_is_scalar),
        CoreExpr::ListCons { head, tail } => expr_is_scalar(head) && expr_is_scalar(tail),
        CoreExpr::Map(fields) => fields.iter().all(|field| expr_is_scalar(&field.value)),
        CoreExpr::FunctionCall { callee, args } => {
            expr_is_scalar(callee) && args.iter().all(expr_is_scalar)
        }
        CoreExpr::Cast { expr, .. } => expr_is_scalar(expr),
        CoreExpr::RecordConstruct { fields, .. } => {
            fields.iter().all(|field| expr_is_scalar(&field.value))
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            expr_is_scalar(base) && fields.iter().all(|field| expr_is_scalar(&field.value))
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            expr_is_scalar(base)
        }
        CoreExpr::UnaryOp { operator, operand } => {
            matches!(operator.as_str(), "-" | "not" | "!") && expr_is_scalar(operand)
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => {
            matches!(
                operator.as_str(),
                "+" | "-"
                    | "*"
                    | "/"
                    | "div"
                    | "rem"
                    | "=="
                    | "!="
                    | "<"
                    | "<="
                    | ">"
                    | ">="
                    | "and"
                    | "or"
            ) && expr_is_scalar(left)
                && expr_is_scalar(right)
        }
        CoreExpr::Let { bindings, body } => {
            bindings.iter().all(|binding| {
                matches!(binding.pattern, CorePattern::Var(_)) && expr_is_scalar(&binding.value)
            }) && expr_is_scalar(body)
        }
        CoreExpr::If { clauses } => {
            !clauses.is_empty()
                && clauses
                    .iter()
                    .all(|clause| expr_is_scalar(&clause.condition) && expr_is_scalar(&clause.body))
        }
        _ => false,
    }
}
