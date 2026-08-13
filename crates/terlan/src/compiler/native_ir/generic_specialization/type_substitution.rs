//! Core expression type substitution for monomorphized function bodies.

use std::collections::HashMap;

use crate::terlan_typeck::{CoreExpr, CoreFunction, CoreIntrinsicId, CoreType};

use super::substitute;

pub(super) fn substitute_function_types(
    function: &mut CoreFunction,
    parameters: &[String],
    values: &HashMap<String, CoreType>,
) {
    for clause in &mut function.clauses {
        if let Some(guard) = clause
            .guard
            .as_mut()
            .and_then(|summary| summary.core_expr.as_mut())
        {
            substitute_expr_types(guard, parameters, values);
        }
        if let Some(body) = clause.body.core_expr.as_mut() {
            substitute_expr_types(body, parameters, values);
        }
    }
}

fn substitute_expr_types(
    expr: &mut CoreExpr,
    parameters: &[String],
    values: &HashMap<String, CoreType>,
) {
    match expr {
        CoreExpr::Intrinsic(call) => {
            call.return_type = substitute(&call.return_type, parameters, values);
            match &mut call.id {
                CoreIntrinsicId::MemoryLayoutOf(ty)
                | CoreIntrinsicId::MemoryShallowSize(ty)
                | CoreIntrinsicId::MemoryRetainedSize(ty)
                | CoreIntrinsicId::VmProcessSendMessage(ty)
                | CoreIntrinsicId::VmProcessReceiveMessage(ty)
                | CoreIntrinsicId::VmProcessSpawn(ty)
                | CoreIntrinsicId::VmProcessEntry(ty)
                | CoreIntrinsicId::VmProcessCurrent(ty)
                | CoreIntrinsicId::VmProcessLink(ty)
                | CoreIntrinsicId::VmProcessMonitor(ty)
                | CoreIntrinsicId::VmProcessAcquireResource(ty)
                | CoreIntrinsicId::VmProcessCancel(ty) => {
                    *ty = substitute(ty, parameters, values);
                }
                CoreIntrinsicId::NativeOperation {
                    parameter_types, ..
                } => {
                    for ty in parameter_types {
                        *ty = substitute(ty, parameters, values);
                    }
                }
                CoreIntrinsicId::Primitive(_) | CoreIntrinsicId::Runtime(_) => {}
            }
            substitute_many(&mut call.args, parameters, values);
        }
        CoreExpr::Cast { expr, target_type } => {
            *target_type = substitute(target_type, parameters, values);
            substitute_expr_types(expr, parameters, values);
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Call { args, .. } => substitute_many(args, parameters, values),
        CoreExpr::MutableReceiverCall { receiver, args, .. }
        | CoreExpr::FunctionCall {
            callee: receiver,
            args,
        } => {
            substitute_expr_types(receiver, parameters, values);
            substitute_many(args, parameters, values);
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            substitute_many(items, parameters, values);
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
            substitute_expr_types(head, parameters, values);
            substitute_expr_types(tail, parameters, values);
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                substitute_expr_types(&mut field.value, parameters, values);
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                substitute_expr_types(&mut field.value, parameters, values);
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            substitute_expr_types(base, parameters, values);
            for field in fields {
                substitute_expr_types(&mut field.value, parameters, values);
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::UnaryOp { operand: base, .. }
        | CoreExpr::Lam { body: base, .. } => substitute_expr_types(base, parameters, values),
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                substitute_expr_types(&mut binding.value, parameters, values);
            }
            substitute_expr_types(body, parameters, values);
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                substitute_expr_types(&mut clause.condition, parameters, values);
                substitute_expr_types(&mut clause.body, parameters, values);
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            substitute_expr_types(scrutinee, parameters, values);
            for clause in clauses {
                if let Some(guard) = &mut clause.guard {
                    substitute_expr_types(guard, parameters, values);
                }
                substitute_expr_types(&mut clause.body, parameters, values);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            substitute_expr_types(body, parameters, values);
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                if let Some(guard) = &mut clause.guard {
                    substitute_expr_types(guard, parameters, values);
                }
                substitute_expr_types(&mut clause.body, parameters, values);
            }
            if let Some(after) = after_clause {
                substitute_expr_types(&mut after.trigger, parameters, values);
                substitute_expr_types(&mut after.body, parameters, values);
            }
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            substitute_expr_types(expr, parameters, values);
            for generator in generators {
                substitute_expr_types(&mut generator.source, parameters, values);
            }
            substitute_many(guards, parameters, values);
        }
        CoreExpr::SqlQuery {
            parameters: query, ..
        } => substitute_many(query, parameters, values),
        CoreExpr::ConstructorChain { args, record, .. } => {
            substitute_many(args, parameters, values);
            substitute_expr_types(record, parameters, values);
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
}

fn substitute_many(
    expressions: &mut [CoreExpr],
    parameters: &[String],
    values: &HashMap<String, CoreType>,
) {
    for expression in expressions {
        substitute_expr_types(expression, parameters, values);
    }
}
