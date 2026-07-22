//! Iterative static-callable budget preflight across the complete CoreIR tree.

use crate::terlan_typeck::CoreExpr;

use super::MAX_STATIC_CALL_EXPANSIONS;

/// Rejects an immediate-call bomb before recursive substitution can consume
/// the compiler thread's stack.
pub(super) fn reject_deep_immediate_callable_chain(expr: &CoreExpr) -> Result<(), String> {
    let mut pending = vec![expr];
    let mut immediate = 0usize;
    while let Some(expr) = pending.pop() {
        if let CoreExpr::FunctionCall { callee, .. } = expr {
            if matches!(
                callee.as_ref(),
                CoreExpr::Lam { .. } | CoreExpr::RemoteFunRef { .. }
            ) {
                immediate = immediate.saturating_add(1);
                if immediate > MAX_STATIC_CALL_EXPANSIONS {
                    return Err(format!(
                        "error[native_ir.specialization_limit]: static function-value expansion exceeds {MAX_STATIC_CALL_EXPANSIONS} calls"
                    ));
                }
            }
        }
        push_children(expr, &mut pending);
    }
    Ok(())
}

fn push_children<'a>(expr: &'a CoreExpr, pending: &mut Vec<&'a CoreExpr>) {
    match expr {
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            pending.extend(items);
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
            pending.push(head);
            pending.push(tail);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            pending.push(expr);
            pending.extend(generators.iter().map(|generator| &generator.source));
            pending.extend(guards);
        }
        CoreExpr::Map(fields) => pending.extend(fields.iter().map(|field| &field.value)),
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::Call { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            pending.extend(args);
        }
        CoreExpr::FunctionCall { callee, args } => {
            pending.push(callee);
            pending.extend(args);
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            pending.extend(fields.iter().map(|field| &field.value));
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            pending.push(base);
            pending.extend(fields.iter().map(|field| &field.value));
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            pending.extend(args);
            pending.push(record);
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            pending.push(receiver);
            pending.extend(args);
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::UnaryOp { operand: base, .. }
        | CoreExpr::Lam { body: base, .. } => pending.push(base),
        CoreExpr::Let { bindings, body } => {
            pending.extend(bindings.iter().map(|binding| &binding.value));
            pending.push(body);
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                pending.push(&clause.condition);
                pending.push(&clause.body);
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            pending.push(scrutinee);
            push_clauses(clauses, pending);
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            pending.push(body);
            push_clauses(of_clauses, pending);
            push_clauses(catch_clauses, pending);
            if let Some(after) = after_clause {
                pending.push(&after.trigger);
                pending.push(&after.body);
            }
        }
        CoreExpr::SqlQuery { parameters, .. } => pending.extend(parameters),
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
}

fn push_clauses<'a>(
    clauses: &'a [crate::terlan_typeck::CoreCaseClause],
    pending: &mut Vec<&'a CoreExpr>,
) {
    for clause in clauses {
        if let Some(guard) = &clause.guard {
            pending.push(guard);
        }
        pending.push(&clause.body);
    }
}
