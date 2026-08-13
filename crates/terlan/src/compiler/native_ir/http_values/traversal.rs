//! Recursive child traversal for managed HTTP value normalization.

use super::*;

/// Rewrites all child expressions while preserving the parent node.
pub(super) fn rewrite_children(expr: &mut CoreExpr, features: HttpFeatures) -> Result<(), String> {
    match expr {
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            rewrite_many(items, features)?
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => {
            **head = rewrite(head, features)?;
            **tail = rewrite(tail, features)?;
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            **expr = rewrite(expr, features)?;
            for generator in generators {
                generator.source = rewrite(&generator.source, features)?;
            }
            rewrite_many(guards, features)?;
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                binding.value = rewrite(&binding.value, features)?;
            }
            **body = rewrite(body, features)?;
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                field.value = rewrite(&field.value, features)?;
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            rewrite_fields(fields, features)?
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            **base = rewrite(base, features)?
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            **base = rewrite(base, features)?;
            rewrite_fields(fields, features)?;
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            rewrite_many(args, features)?;
            **record = rewrite(record, features)?;
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Call { args, .. } => rewrite_many(args, features)?,
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            **receiver = rewrite(receiver, features)?;
            rewrite_many(args, features)?;
        }
        CoreExpr::FunctionCall { callee, args } => {
            **callee = rewrite(callee, features)?;
            rewrite_many(args, features)?;
        }
        CoreExpr::Cast { expr, .. } => **expr = rewrite(expr, features)?,
        CoreExpr::Intrinsic(call) => rewrite_many(&mut call.args, features)?,
        CoreExpr::SqlQuery { parameters, .. } => rewrite_many(parameters, features)?,
        CoreExpr::Case { scrutinee, clauses } => {
            **scrutinee = rewrite(scrutinee, features)?;
            rewrite_clauses(clauses, features)?;
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            **body = rewrite(body, features)?;
            rewrite_clauses(of_clauses, features)?;
            rewrite_clauses(catch_clauses, features)?;
            if let Some(after) = after_clause {
                *after.trigger = rewrite(&after.trigger, features)?;
                *after.body = rewrite(&after.body, features)?;
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                clause.condition = rewrite(&clause.condition, features)?;
                clause.body = rewrite(&clause.body, features)?;
            }
        }
        CoreExpr::Lam { body, .. } => **body = rewrite(body, features)?,
        CoreExpr::UnaryOp { operand, .. } => **operand = rewrite(operand, features)?,
        CoreExpr::BinaryOp { left, right, .. } => {
            **left = rewrite(left, features)?;
            **right = rewrite(right, features)?;
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

fn rewrite_many(expressions: &mut [CoreExpr], features: HttpFeatures) -> Result<(), String> {
    for expression in expressions {
        *expression = rewrite(expression, features)?;
    }
    Ok(())
}

fn rewrite_fields(
    fields: &mut [crate::terlan_typeck::CoreRecordExprField],
    features: HttpFeatures,
) -> Result<(), String> {
    for field in fields {
        field.value = rewrite(&field.value, features)?;
    }
    Ok(())
}

fn rewrite_clauses(clauses: &mut [CoreCaseClause], features: HttpFeatures) -> Result<(), String> {
    for clause in clauses {
        if let Some(guard) = &mut clause.guard {
            *guard = rewrite(guard, features)?;
        }
        clause.body = rewrite(&clause.body, features)?;
    }
    Ok(())
}
