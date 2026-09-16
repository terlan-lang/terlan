//! Exhaustive post-order mutable traversal of checked expression children.

use super::CoreExpr;

/// Visits each expression after its children, including guards and cleanup paths.
pub(crate) fn visit_core_expr_mut(expr: &mut CoreExpr, visit: &mut impl FnMut(&mut CoreExpr)) {
    match expr {
        CoreExpr::Call { args, .. }
        | CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(super::CoreIntrinsicCall { args, .. }) => {
            for arg in args {
                visit_core_expr_mut(arg, visit);
            }
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. }
        | CoreExpr::FunctionCall {
            callee: receiver,
            args,
        } => {
            visit_core_expr_mut(receiver, visit);
            for arg in args {
                visit_core_expr_mut(arg, visit);
            }
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            for item in items {
                visit_core_expr_mut(item, visit);
            }
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        }
        | CoreExpr::BinaryOp {
            left: head,
            right: tail,
            ..
        } => {
            visit_core_expr_mut(head, visit);
            visit_core_expr_mut(tail, visit);
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                visit_core_expr_mut(&mut field.value, visit);
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                visit_core_expr_mut(&mut field.value, visit);
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            visit_core_expr_mut(base, visit);
            for field in fields {
                visit_core_expr_mut(&mut field.value, visit);
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::UnaryOp { operand: base, .. }
        | CoreExpr::Lam { body: base, .. } => visit_core_expr_mut(base, visit),
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                visit_core_expr_mut(&mut binding.value, visit);
            }
            visit_core_expr_mut(body, visit);
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                visit_core_expr_mut(&mut clause.condition, visit);
                visit_core_expr_mut(&mut clause.body, visit);
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            visit_core_expr_mut(scrutinee, visit);
            for clause in clauses {
                if let Some(guard) = &mut clause.guard {
                    visit_core_expr_mut(guard, visit);
                }
                visit_core_expr_mut(&mut clause.body, visit);
            }
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            for arg in args {
                visit_core_expr_mut(arg, visit);
            }
            visit_core_expr_mut(record, visit);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            visit_core_expr_mut(expr, visit);
            for generator in generators {
                visit_core_expr_mut(&mut generator.source, visit);
            }
            for guard in guards {
                visit_core_expr_mut(guard, visit);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            visit_core_expr_mut(body, visit);
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                if let Some(guard) = &mut clause.guard {
                    visit_core_expr_mut(guard, visit);
                }
                visit_core_expr_mut(&mut clause.body, visit);
            }
            if let Some(after) = after_clause {
                visit_core_expr_mut(&mut after.trigger, visit);
                visit_core_expr_mut(&mut after.body, visit);
            }
        }
        CoreExpr::SqlQuery { parameters, .. } => {
            for parameter in parameters {
                visit_core_expr_mut(parameter, visit);
            }
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
    visit(expr);
}
