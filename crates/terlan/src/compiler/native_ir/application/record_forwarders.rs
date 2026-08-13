//! Closed-image inlining for exact record-constructor forwarders.

use std::collections::HashMap;

use crate::terlan_typeck::{CoreExpr, CoreFunction, CoreModule, CorePattern, CoreRecordExprField};

#[derive(Clone)]
struct RecordForwarder {
    fields: Vec<(String, bool)>,
    record: String,
}

pub(super) fn inline_record_forwarders(cores: &mut [CoreModule]) {
    let forwarders = cores
        .iter()
        .flat_map(|core| {
            core.functions.iter().filter_map(move |function| {
                record_forwarder(function).map(|forwarder| {
                    (
                        (core.module.clone(), function.name.clone(), function.arity),
                        forwarder,
                    )
                })
            })
        })
        .collect::<HashMap<_, _>>();
    for core in cores {
        let module = core.module.clone();
        for function in &mut core.functions {
            for clause in &mut function.clauses {
                if let Some(guard) = &mut clause.guard {
                    if let Some(expr) = &mut guard.core_expr {
                        rewrite(expr, &module, &forwarders);
                    }
                }
                if let Some(expr) = &mut clause.body.core_expr {
                    rewrite(expr, &module, &forwarders);
                }
            }
        }
    }
}

fn record_forwarder(function: &CoreFunction) -> Option<RecordForwarder> {
    let [clause] = function.clauses.as_slice() else {
        return None;
    };
    if clause.guard.is_some()
        || clause.core_patterns.len() != function.params.len()
        || !clause
            .core_patterns
            .iter()
            .zip(&function.params)
            .all(|(pattern, parameter)| {
                matches!(pattern, Some(CorePattern::Var(name)) if name == &parameter.name)
            })
    {
        return None;
    }
    let CoreExpr::RecordConstruct { name, fields } = clause.body.core_expr.as_ref()? else {
        return None;
    };
    if fields.len() != function.params.len()
        || !fields
            .iter()
            .zip(&function.params)
            .all(|(field, parameter)| {
                matches!(&field.value, CoreExpr::Var(name) if name == &parameter.name)
            })
    {
        return None;
    }
    Some(RecordForwarder {
        fields: fields
            .iter()
            .map(|field| (field.key.clone(), field.required))
            .collect(),
        record: name.clone(),
    })
}

fn rewrite(
    expr: &mut CoreExpr,
    module: &str,
    forwarders: &HashMap<(String, String, usize), RecordForwarder>,
) {
    match expr {
        CoreExpr::Call { function, args } => {
            for arg in args.iter_mut() {
                rewrite(arg, module, forwarders);
            }
            let target = function
                .rsplit_once('.')
                .map(|(module, function)| (module.to_string(), function.to_string(), args.len()))
                .unwrap_or_else(|| (module.to_string(), function.clone(), args.len()));
            let Some(forwarder) = forwarders.get(&target) else {
                return;
            };
            let values = std::mem::take(args);
            *expr = CoreExpr::RecordConstruct {
                name: forwarder.record.clone(),
                fields: forwarder
                    .fields
                    .iter()
                    .cloned()
                    .zip(values)
                    .map(|((key, required), value)| CoreRecordExprField {
                        key,
                        required,
                        value,
                    })
                    .collect(),
            };
        }
        CoreExpr::RemoteCall { args, .. } | CoreExpr::ConstructorCall { args, .. } => {
            rewrite_many(args, module, forwarders)
        }
        CoreExpr::Intrinsic(call) => rewrite_many(&mut call.args, module, forwarders),
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            rewrite_many(items, module, forwarders)
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
            rewrite(head, module, forwarders);
            rewrite(tail, module, forwarders);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            rewrite(expr, module, forwarders);
            for generator in generators {
                rewrite(&mut generator.source, module, forwarders);
            }
            rewrite_many(guards, module, forwarders);
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                rewrite(&mut field.value, module, forwarders);
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                rewrite(&mut field.value, module, forwarders);
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            rewrite(base, module, forwarders);
            for field in fields {
                rewrite(&mut field.value, module, forwarders);
            }
        }
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. }
        | CoreExpr::UnaryOp { operand: base, .. }
        | CoreExpr::Lam { body: base, .. } => rewrite(base, module, forwarders),
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                rewrite(&mut binding.value, module, forwarders);
            }
            rewrite(body, module, forwarders);
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                rewrite(&mut clause.condition, module, forwarders);
                rewrite(&mut clause.body, module, forwarders);
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            rewrite(scrutinee, module, forwarders);
            for clause in clauses {
                if let Some(guard) = &mut clause.guard {
                    rewrite(guard, module, forwarders);
                }
                rewrite(&mut clause.body, module, forwarders);
            }
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            rewrite(body, module, forwarders);
            for clause in of_clauses.iter_mut().chain(catch_clauses) {
                if let Some(guard) = &mut clause.guard {
                    rewrite(guard, module, forwarders);
                }
                rewrite(&mut clause.body, module, forwarders);
            }
            if let Some(after) = after_clause {
                rewrite(&mut after.trigger, module, forwarders);
                rewrite(&mut after.body, module, forwarders);
            }
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            rewrite_many(args, module, forwarders);
            rewrite(record, module, forwarders);
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            rewrite(receiver, module, forwarders);
            rewrite_many(args, module, forwarders);
        }
        CoreExpr::FunctionCall { callee, args } => {
            rewrite(callee, module, forwarders);
            rewrite_many(args, module, forwarders);
        }
        CoreExpr::SqlQuery { parameters, .. } => rewrite_many(parameters, module, forwarders),
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_)
        | CoreExpr::RemoteFunRef { .. } => {}
    }
}

fn rewrite_many(
    expressions: &mut [CoreExpr],
    module: &str,
    forwarders: &HashMap<(String, String, usize), RecordForwarder>,
) {
    for expression in expressions {
        rewrite(expression, module, forwarders);
    }
}
