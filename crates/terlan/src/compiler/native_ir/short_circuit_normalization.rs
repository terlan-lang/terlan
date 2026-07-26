//! Canonicalizes associative short-circuit trees for linear AOT composition.

use crate::terlan_typeck::{CoreExpr, CoreModule};

/// Reassociates homogeneous boolean chains while preserving left-to-right
/// evaluation and short-circuit behavior.
pub(super) fn right_associate_short_circuit_chains(cores: &mut [CoreModule]) {
    for core in cores {
        for function in &mut core.functions {
            for clause in &mut function.clauses {
                if let Some(guard) = clause
                    .guard
                    .as_mut()
                    .and_then(|guard| guard.core_expr.as_mut())
                {
                    normalize(guard);
                }
                if let Some(body) = clause.body.core_expr.as_mut() {
                    normalize(body);
                }
            }
        }
    }
}

pub(super) fn normalize(expr: &mut CoreExpr) {
    match expr {
        CoreExpr::Call { args, .. }
        | CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. } => args.iter_mut().for_each(normalize),
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            items.iter_mut().for_each(normalize);
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => {
            normalize(head);
            normalize(tail);
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            normalize(left);
            normalize(right);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            normalize(expr);
            generators
                .iter_mut()
                .for_each(|generator| normalize(&mut generator.source));
            guards.iter_mut().for_each(normalize);
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter_mut()
                .for_each(|binding| normalize(&mut binding.value));
            normalize(body);
        }
        CoreExpr::Map(fields) => fields
            .iter_mut()
            .for_each(|field| normalize(&mut field.value)),
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            fields
                .iter_mut()
                .for_each(|field| normalize(&mut field.value));
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            normalize(base);
            fields
                .iter_mut()
                .for_each(|field| normalize(&mut field.value));
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::UnaryOp { operand: base, .. }
        | CoreExpr::Cast { expr: base, .. } => normalize(base),
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter_mut().for_each(normalize);
            normalize(record);
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            normalize(receiver);
            args.iter_mut().for_each(normalize);
        }
        CoreExpr::FunctionCall { callee, args } => {
            normalize(callee);
            args.iter_mut().for_each(normalize);
        }
        CoreExpr::Intrinsic(call) => call.args.iter_mut().for_each(normalize),
        CoreExpr::SqlQuery { parameters, .. } => parameters.iter_mut().for_each(normalize),
        CoreExpr::Case { scrutinee, clauses } => {
            normalize(scrutinee);
            for clause in clauses {
                if let Some(guard) = &mut clause.guard {
                    normalize(guard);
                }
                normalize(&mut clause.body);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            normalize(body);
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                if let Some(guard) = &mut clause.guard {
                    normalize(guard);
                }
                normalize(&mut clause.body);
            }
            if let Some(after) = after_clause {
                normalize(&mut after.trigger);
                normalize(&mut after.body);
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                normalize(&mut clause.condition);
                normalize(&mut clause.body);
            }
        }
        CoreExpr::Lam { body, .. } => normalize(body),
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }

    let CoreExpr::BinaryOp {
        operator,
        left,
        right,
    } = expr
    else {
        return;
    };
    if !matches!(operator.as_str(), "and" | "&&" | "or" | "||") {
        return;
    }
    let operator = operator.clone();
    let left = std::mem::replace(left, Box::new(CoreExpr::Atom("false".to_string())));
    let right = std::mem::replace(right, Box::new(CoreExpr::Atom("false".to_string())));
    let mut terms = Vec::new();
    collect_terms(*left, &operator, &mut terms);
    collect_terms(*right, &operator, &mut terms);
    let mut terms = terms.into_iter().rev();
    let Some(mut rebuilt) = terms.next() else {
        return;
    };
    for term in terms {
        rebuilt = CoreExpr::BinaryOp {
            operator: operator.clone(),
            left: Box::new(term),
            right: Box::new(rebuilt),
        };
    }
    *expr = rebuilt;
}

fn collect_terms(expr: CoreExpr, operator: &str, terms: &mut Vec<CoreExpr>) {
    match expr {
        CoreExpr::BinaryOp {
            operator: nested,
            left,
            right,
        } if nested == operator => {
            collect_terms(*left, operator, terms);
            collect_terms(*right, operator, terms);
        }
        term => terms.push(term),
    }
}
