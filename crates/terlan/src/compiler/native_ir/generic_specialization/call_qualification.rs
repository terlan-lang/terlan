use super::*;

pub(super) fn qualify_local_calls(
    function: &mut CoreFunction,
    module: &str,
    local: &HashSet<(String, usize)>,
) {
    for clause in &mut function.clauses {
        if let Some(guard) = clause
            .guard
            .as_mut()
            .and_then(|summary| summary.core_expr.as_mut())
        {
            qualify_expr_calls(guard, module, local);
        }
        if let Some(body) = clause.body.core_expr.as_mut() {
            qualify_expr_calls(body, module, local);
        }
    }
}

fn qualify_expr_calls(expr: &mut CoreExpr, module: &str, local: &HashSet<(String, usize)>) {
    match expr {
        CoreExpr::Call { function, args } => {
            for arg in args.iter_mut() {
                qualify_expr_calls(arg, module, local);
            }
            if !function.contains('.') && local.contains(&(function.clone(), args.len())) {
                *function = format!("{module}.{function}");
            }
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            for arg in args {
                qualify_expr_calls(arg, module, local);
            }
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. }
        | CoreExpr::FunctionCall {
            callee: receiver,
            args,
        } => {
            qualify_expr_calls(receiver, module, local);
            for arg in args {
                qualify_expr_calls(arg, module, local);
            }
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            for item in items {
                qualify_expr_calls(item, module, local);
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
            qualify_expr_calls(head, module, local);
            qualify_expr_calls(tail, module, local);
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                qualify_expr_calls(&mut field.value, module, local);
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                qualify_expr_calls(&mut field.value, module, local);
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            qualify_expr_calls(base, module, local);
            for field in fields {
                qualify_expr_calls(&mut field.value, module, local);
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::UnaryOp { operand: base, .. }
        | CoreExpr::Lam { body: base, .. } => qualify_expr_calls(base, module, local),
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                qualify_expr_calls(&mut binding.value, module, local);
            }
            qualify_expr_calls(body, module, local);
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                qualify_expr_calls(&mut clause.condition, module, local);
                qualify_expr_calls(&mut clause.body, module, local);
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            qualify_expr_calls(scrutinee, module, local);
            for clause in clauses {
                if let Some(guard) = &mut clause.guard {
                    qualify_expr_calls(guard, module, local);
                }
                qualify_expr_calls(&mut clause.body, module, local);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            qualify_expr_calls(body, module, local);
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                if let Some(guard) = &mut clause.guard {
                    qualify_expr_calls(guard, module, local);
                }
                qualify_expr_calls(&mut clause.body, module, local);
            }
            if let Some(after) = after_clause {
                qualify_expr_calls(&mut after.trigger, module, local);
                qualify_expr_calls(&mut after.body, module, local);
            }
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            qualify_expr_calls(expr, module, local);
            for generator in generators {
                qualify_expr_calls(&mut generator.source, module, local);
            }
            for guard in guards {
                qualify_expr_calls(guard, module, local);
            }
        }
        CoreExpr::SqlQuery { parameters, .. } => {
            for parameter in parameters {
                qualify_expr_calls(parameter, module, local);
            }
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            for arg in args {
                qualify_expr_calls(arg, module, local);
            }
            qualify_expr_calls(record, module, local);
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
}
