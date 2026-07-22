//! Mandatory elimination of constructor-chain CoreIR before native admission.

use crate::terlan_typeck::{CoreExpr, CoreLetBinding, CoreModule, CorePattern};

pub(super) fn lower_constructor_chains(core: &mut CoreModule) {
    let mut ordinal = 0usize;
    for function in &mut core.functions {
        for clause in &mut function.clauses {
            if let Some(body) = clause.body.core_expr.as_mut() {
                rewrite(body, &mut ordinal);
            }
        }
    }
}

fn rewrite(expr: &mut CoreExpr, ordinal: &mut usize) {
    rewrite_children(expr, ordinal);
    let CoreExpr::ConstructorChain {
        base,
        base_constructor_identity,
        args,
        record,
    } = expr
    else {
        return;
    };
    let base_value = CoreExpr::ConstructorCall {
        constructor: base.clone(),
        constructor_identity: base_constructor_identity.clone(),
        args: std::mem::take(args),
    };
    let binding = format!("$native_constructor_chain_{}", *ordinal);
    *ordinal = ordinal.saturating_add(1);
    *expr = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var(binding),
            value: base_value,
        }],
        body: record.clone(),
    };
}

fn rewrite_children(expr: &mut CoreExpr, ordinal: &mut usize) {
    match expr {
        CoreExpr::Call { args, .. }
        | CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            args.iter_mut().for_each(|arg| rewrite(arg, ordinal));
        }
        CoreExpr::FunctionCall { callee, args } => {
            rewrite(callee, ordinal);
            args.iter_mut().for_each(|arg| rewrite(arg, ordinal));
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            rewrite(receiver, ordinal);
            args.iter_mut().for_each(|arg| rewrite(arg, ordinal));
        }
        CoreExpr::List(items) | CoreExpr::Tuple(items) | CoreExpr::FixedArray(items) => {
            items.iter_mut().for_each(|item| rewrite(item, ordinal));
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::BinaryOp {
            left: head,
            right: tail,
            ..
        } => {
            rewrite(head, ordinal);
            rewrite(tail, ordinal);
        }
        CoreExpr::Index { base, index } => {
            rewrite(base, ordinal);
            rewrite(index, ordinal);
        }
        CoreExpr::ListComprehension {
            expr: yield_expr,
            generators,
            guards,
            ..
        } => {
            rewrite(yield_expr, ordinal);
            generators
                .iter_mut()
                .for_each(|generator| rewrite(&mut generator.source, ordinal));
            guards.iter_mut().for_each(|guard| rewrite(guard, ordinal));
        }
        CoreExpr::Map(fields) => {
            fields
                .iter_mut()
                .for_each(|field| rewrite(&mut field.value, ordinal));
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            fields
                .iter_mut()
                .for_each(|field| rewrite(&mut field.value, ordinal));
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            rewrite(base, ordinal);
            fields
                .iter_mut()
                .for_each(|field| rewrite(&mut field.value, ordinal));
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::UnaryOp { operand: base, .. }
        | CoreExpr::Lam { body: base, .. } => rewrite(base, ordinal),
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter_mut()
                .for_each(|binding| rewrite(&mut binding.value, ordinal));
            rewrite(body, ordinal);
        }
        CoreExpr::If { clauses } => clauses.iter_mut().for_each(|clause| {
            rewrite(&mut clause.condition, ordinal);
            rewrite(&mut clause.body, ordinal);
        }),
        CoreExpr::Case { scrutinee, clauses } => {
            rewrite(scrutinee, ordinal);
            clauses.iter_mut().for_each(|clause| {
                if let Some(guard) = &mut clause.guard {
                    rewrite(guard, ordinal);
                }
                rewrite(&mut clause.body, ordinal);
            });
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            rewrite(body, ordinal);
            of_clauses
                .iter_mut()
                .chain(catch_clauses)
                .for_each(|clause| {
                    if let Some(guard) = &mut clause.guard {
                        rewrite(guard, ordinal);
                    }
                    rewrite(&mut clause.body, ordinal);
                });
            if let Some(after) = after_clause {
                rewrite(&mut after.trigger, ordinal);
                rewrite(&mut after.body, ordinal);
            }
        }
        CoreExpr::SqlQuery { parameters, .. } => {
            parameters
                .iter_mut()
                .for_each(|parameter| rewrite(parameter, ordinal));
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter_mut().for_each(|arg| rewrite(arg, ordinal));
            rewrite(record, ordinal);
        }
        _ => {}
    }
}
