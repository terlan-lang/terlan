//! Exhaustive mutable traversal of checked expression children.

use super::CoreExpr;

/// Visits each expression after its children, including guards and cleanup paths.
pub(crate) fn visit_core_expr_mut(expr: &mut CoreExpr, visit: &mut impl FnMut(&mut CoreExpr)) {
    visit_core_expr_children_mut(expr, &mut |child| visit_core_expr_mut(child, visit));
    visit(expr);
}

/// Visits immediate children once; scope-aware passes own the recursion.
pub(crate) fn visit_core_expr_children_mut(
    expr: &mut CoreExpr,
    visit: &mut impl FnMut(&mut CoreExpr),
) {
    match expr {
        CoreExpr::Call { args, .. }
        | CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(super::CoreIntrinsicCall { args, .. }) => {
            for arg in args {
                visit(arg);
            }
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. }
        | CoreExpr::FunctionCall {
            callee: receiver,
            args,
        } => {
            visit(receiver);
            for arg in args {
                visit(arg);
            }
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            for item in items {
                visit(item);
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
            visit(head);
            visit(tail);
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                visit(&mut field.value);
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                visit(&mut field.value);
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            visit(base);
            for field in fields {
                visit(&mut field.value);
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::UnaryOp { operand: base, .. }
        | CoreExpr::Lam { body: base, .. } => visit(base),
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                visit(&mut binding.value);
            }
            visit(body);
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                visit(&mut clause.condition);
                visit(&mut clause.body);
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            visit(scrutinee);
            for clause in clauses {
                if let Some(guard) = &mut clause.guard {
                    visit(guard);
                }
                visit(&mut clause.body);
            }
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            for arg in args {
                visit(arg);
            }
            visit(record);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            visit(expr);
            for generator in generators {
                visit(&mut generator.source);
            }
            for guard in guards {
                visit(guard);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            visit(body);
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                if let Some(guard) = &mut clause.guard {
                    visit(guard);
                }
                visit(&mut clause.body);
            }
            if let Some(after) = after_clause {
                visit(&mut after.trigger);
                visit(&mut after.body);
            }
        }
        CoreExpr::SqlQuery { parameters, .. } => {
            for parameter in parameters {
                visit(parameter);
            }
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
}
