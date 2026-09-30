//! Recursive child traversal for managed HTTP value normalization.

use super::*;

/// Rewrites all child expressions while preserving the parent node.
pub(super) fn rewrite_children(expr: &mut CoreExpr) -> Result<(), String> {
    match expr {
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            rewrite_many(items)?
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => {
            **head = rewrite(head)?;
            **tail = rewrite(tail)?;
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            **expr = rewrite(expr)?;
            for generator in generators {
                generator.source = rewrite(&generator.source)?;
            }
            rewrite_many(guards)?;
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                binding.value = rewrite(&binding.value)?;
            }
            **body = rewrite(body)?;
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                field.value = rewrite(&field.value)?;
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            rewrite_fields(fields)?
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            **base = rewrite(base)?
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            **base = rewrite(base)?;
            rewrite_fields(fields)?;
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            rewrite_many(args)?;
            **record = rewrite(record)?;
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Call { args, .. } => rewrite_many(args)?,
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            **receiver = rewrite(receiver)?;
            rewrite_many(args)?;
        }
        CoreExpr::FunctionCall { callee, args } => {
            **callee = rewrite(callee)?;
            rewrite_many(args)?;
        }
        CoreExpr::Cast { expr, .. } => **expr = rewrite(expr)?,
        CoreExpr::Intrinsic(call) => rewrite_many(&mut call.args)?,
        CoreExpr::SqlQuery { parameters, .. } => rewrite_many(parameters)?,
        CoreExpr::Case { scrutinee, clauses } => {
            **scrutinee = rewrite(scrutinee)?;
            rewrite_clauses(clauses)?;
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            **body = rewrite(body)?;
            rewrite_clauses(of_clauses)?;
            rewrite_clauses(catch_clauses)?;
            if let Some(after) = after_clause {
                *after.trigger = rewrite(&after.trigger)?;
                *after.body = rewrite(&after.body)?;
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                clause.condition = rewrite(&clause.condition)?;
                clause.body = rewrite(&clause.body)?;
            }
        }
        CoreExpr::Lam { body, .. } => **body = rewrite(body)?,
        CoreExpr::UnaryOp { operand, .. } => **operand = rewrite(operand)?,
        CoreExpr::BinaryOp { left, right, .. } => {
            **left = rewrite(left)?;
            **right = rewrite(right)?;
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
    Ok(())
}

fn rewrite_many(expressions: &mut [CoreExpr]) -> Result<(), String> {
    for expression in expressions {
        *expression = rewrite(expression)?;
    }
    Ok(())
}

fn rewrite_fields(fields: &mut [crate::terlan_typeck::CoreRecordExprField]) -> Result<(), String> {
    for field in fields {
        field.value = rewrite(&field.value)?;
    }
    Ok(())
}

fn rewrite_clauses(clauses: &mut [CoreCaseClause]) -> Result<(), String> {
    for clause in clauses {
        if let Some(guard) = &mut clause.guard {
            *guard = rewrite(guard)?;
        }
        clause.body = rewrite(&clause.body)?;
    }
    Ok(())
}
