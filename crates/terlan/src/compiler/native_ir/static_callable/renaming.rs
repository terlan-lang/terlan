//! Capture-avoiding renaming for statically resolved callable bodies.

use crate::terlan_typeck::{CoreExpr, CoreLetBinding, CorePattern};
use std::collections::{HashMap, HashSet};

/// Renames free variables while preserving sequential lexical shadowing.
pub(in super::super) fn rename_free_variables(
    expr: &CoreExpr,
    renames: &HashMap<String, String>,
    bound: &mut HashSet<String>,
) -> CoreExpr {
    match expr {
        CoreExpr::Var(name) if !bound.contains(name) => renames
            .get(name)
            .cloned()
            .map(CoreExpr::Var)
            .unwrap_or_else(|| expr.clone()),
        CoreExpr::Tuple(items) => CoreExpr::Tuple(rename_many(items, renames, bound)),
        CoreExpr::List(items) => CoreExpr::List(rename_many(items, renames, bound)),
        CoreExpr::FixedArray(items) => CoreExpr::FixedArray(rename_many(items, renames, bound)),
        CoreExpr::ListCons { head, tail } => CoreExpr::ListCons {
            head: Box::new(rename_free_variables(head, renames, bound)),
            tail: Box::new(rename_free_variables(tail, renames, bound)),
        },
        CoreExpr::Index { base, index } => CoreExpr::Index {
            base: Box::new(rename_free_variables(base, renames, bound)),
            index: Box::new(rename_free_variables(index, renames, bound)),
        },
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            lift,
        } => {
            let original = bound.clone();
            let generators = generators
                .iter()
                .map(|generator| {
                    let lowered = crate::terlan_typeck::CoreListComprehensionGenerator {
                        pattern: generator.pattern.clone(),
                        source: rename_free_variables(&generator.source, renames, bound),
                    };
                    bind_static_pattern(&generator.pattern, bound);
                    lowered
                })
                .collect();
            let guards = rename_many(guards, renames, bound);
            let expr = Box::new(rename_free_variables(expr, renames, bound));
            *bound = original;
            CoreExpr::ListComprehension {
                expr,
                generators,
                guards,
                lift: lift.clone(),
            }
        }
        CoreExpr::Map(fields) => CoreExpr::Map(
            fields
                .iter()
                .map(|field| {
                    let mut field = field.clone();
                    field.value = rename_free_variables(&field.value, renames, bound);
                    field
                })
                .collect(),
        ),
        CoreExpr::Call { function, args } => CoreExpr::Call {
            function: function.clone(),
            args: rename_many(args, renames, bound),
        },
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } => CoreExpr::RemoteCall {
            module: module.clone(),
            function: function.clone(),
            args: rename_many(args, renames, bound),
        },
        CoreExpr::ConstructorCall {
            constructor,
            constructor_identity,
            args,
        } => CoreExpr::ConstructorCall {
            constructor: constructor.clone(),
            constructor_identity: constructor_identity.clone(),
            args: rename_many(args, renames, bound),
        },
        CoreExpr::MutableReceiverCall {
            receiver,
            method,
            args,
            effects,
        } => CoreExpr::MutableReceiverCall {
            receiver: Box::new(rename_free_variables(receiver, renames, bound)),
            method: method.clone(),
            args: rename_many(args, renames, bound),
            effects: effects.clone(),
        },
        CoreExpr::Intrinsic(call) => {
            let mut call = call.clone();
            call.args = rename_many(&call.args, renames, bound);
            CoreExpr::Intrinsic(call)
        }
        CoreExpr::FunctionCall { callee, args } => CoreExpr::FunctionCall {
            callee: Box::new(rename_free_variables(callee, renames, bound)),
            args: rename_many(args, renames, bound),
        },
        CoreExpr::RecordConstruct { name, fields } => CoreExpr::RecordConstruct {
            name: name.clone(),
            fields: rename_record_fields(fields, renames, bound),
        },
        CoreExpr::TemplateInstantiate { name, fields } => CoreExpr::TemplateInstantiate {
            name: name.clone(),
            fields: rename_record_fields(fields, renames, bound),
        },
        CoreExpr::RecordUpdate { base, name, fields } => CoreExpr::RecordUpdate {
            base: Box::new(rename_free_variables(base, renames, bound)),
            name: name.clone(),
            fields: rename_record_fields(fields, renames, bound),
        },
        CoreExpr::FieldAccess { base, field } => CoreExpr::FieldAccess {
            base: Box::new(rename_free_variables(base, renames, bound)),
            field: field.clone(),
        },
        CoreExpr::RecordAccess { base, name, field } => CoreExpr::RecordAccess {
            base: Box::new(rename_free_variables(base, renames, bound)),
            name: name.clone(),
            field: field.clone(),
        },
        CoreExpr::ConstructorChain {
            base,
            base_constructor_identity,
            args,
            record,
        } => CoreExpr::ConstructorChain {
            base: base.clone(),
            base_constructor_identity: base_constructor_identity.clone(),
            args: rename_many(args, renames, bound),
            record: Box::new(rename_free_variables(record, renames, bound)),
        },
        CoreExpr::UnaryOp { operator, operand } => CoreExpr::UnaryOp {
            operator: operator.clone(),
            operand: Box::new(rename_free_variables(operand, renames, bound)),
        },
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => CoreExpr::BinaryOp {
            operator: operator.clone(),
            left: Box::new(rename_free_variables(left, renames, bound)),
            right: Box::new(rename_free_variables(right, renames, bound)),
        },
        CoreExpr::Let { bindings, body } => {
            let original = bound.clone();
            let mut lowered = Vec::with_capacity(bindings.len());
            for binding in bindings {
                lowered.push(CoreLetBinding {
                    pattern: binding.pattern.clone(),
                    value: rename_free_variables(&binding.value, renames, bound),
                });
                bind_static_pattern(&binding.pattern, bound);
            }
            let body = rename_free_variables(body, renames, bound);
            *bound = original;
            CoreExpr::Let {
                bindings: lowered,
                body: Box::new(body),
            }
        }
        CoreExpr::Cast { expr, target_type } => CoreExpr::Cast {
            expr: Box::new(rename_free_variables(expr, renames, bound)),
            target_type: target_type.clone(),
        },
        CoreExpr::If { clauses } => CoreExpr::If {
            clauses: clauses
                .iter()
                .map(|clause| {
                    let mut clause = clause.clone();
                    clause.condition = rename_free_variables(&clause.condition, renames, bound);
                    clause.body = rename_free_variables(&clause.body, renames, bound);
                    clause
                })
                .collect(),
        },
        CoreExpr::Case { scrutinee, clauses } => CoreExpr::Case {
            scrutinee: Box::new(rename_free_variables(scrutinee, renames, bound)),
            clauses: rename_case_clauses(clauses, renames, bound),
        },
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => CoreExpr::Try {
            body: Box::new(rename_free_variables(body, renames, bound)),
            of_clauses: rename_case_clauses(of_clauses, renames, bound),
            catch_clauses: rename_case_clauses(catch_clauses, renames, bound),
            after_clause: after_clause.as_ref().map(|after| {
                let mut after = after.clone();
                after.trigger = Box::new(rename_free_variables(&after.trigger, renames, bound));
                after.body = Box::new(rename_free_variables(&after.body, renames, bound));
                after
            }),
        },
        CoreExpr::Lam {
            params,
            parameter_types,
            body,
        } => {
            let original = bound.clone();
            for pattern in params {
                bind_static_pattern(pattern, bound);
            }
            let body = Box::new(rename_free_variables(body, renames, bound));
            *bound = original;
            CoreExpr::Lam {
                params: params.clone(),
                parameter_types: parameter_types.clone(),
                body,
            }
        }
        CoreExpr::SqlQuery { parameters, .. } => {
            let mut query = expr.clone();
            let CoreExpr::SqlQuery {
                parameters: lowered,
                ..
            } = &mut query
            else {
                unreachable!()
            };
            *lowered = rename_many(parameters, renames, bound);
            query
        }
        _ => expr.clone(),
    }
}

/// Renames free variables in an ordered expression list.
fn rename_many(
    expressions: &[CoreExpr],
    renames: &HashMap<String, String>,
    bound: &mut HashSet<String>,
) -> Vec<CoreExpr> {
    expressions
        .iter()
        .map(|expr| rename_free_variables(expr, renames, bound))
        .collect()
}

fn rename_record_fields(
    fields: &[crate::terlan_typeck::CoreRecordExprField],
    renames: &HashMap<String, String>,
    bound: &mut HashSet<String>,
) -> Vec<crate::terlan_typeck::CoreRecordExprField> {
    fields
        .iter()
        .map(|field| {
            let mut field = field.clone();
            field.value = rename_free_variables(&field.value, renames, bound);
            field
        })
        .collect()
}

fn rename_case_clauses(
    clauses: &[crate::terlan_typeck::CoreCaseClause],
    renames: &HashMap<String, String>,
    bound: &mut HashSet<String>,
) -> Vec<crate::terlan_typeck::CoreCaseClause> {
    clauses
        .iter()
        .map(|clause| {
            let original = bound.clone();
            bind_static_pattern(&clause.pattern, bound);
            let mut clause = clause.clone();
            clause.guard = clause
                .guard
                .as_ref()
                .map(|guard| rename_free_variables(guard, renames, bound));
            clause.body = rename_free_variables(&clause.body, renames, bound);
            *bound = original;
            clause
        })
        .collect()
}

/// Records bindings introduced by a pattern before renaming its lexical scope.
pub(super) fn bind_static_pattern(pattern: &CorePattern, bound: &mut HashSet<String>) {
    match pattern {
        CorePattern::Var(name) => {
            bound.insert(name.clone());
        }
        CorePattern::Alias { alias, pattern } => {
            bound.insert(alias.clone());
            bind_static_pattern(pattern, bound);
        }
        CorePattern::Tuple(patterns) | CorePattern::List(patterns) => {
            for pattern in patterns {
                bind_static_pattern(pattern, bound);
            }
        }
        CorePattern::ListCons { head, tail } => {
            bind_static_pattern(head, bound);
            bind_static_pattern(tail, bound);
        }
        CorePattern::Map(fields) => {
            for field in fields {
                bind_static_pattern(&field.value, bound);
            }
        }
        CorePattern::Record { fields, .. } => {
            for field in fields {
                bind_static_pattern(&field.value, bound);
            }
        }
        CorePattern::Constructor { args, .. } => {
            for pattern in args {
                bind_static_pattern(pattern, bound);
            }
        }
        CorePattern::BinaryLayout { fields, .. } => {
            for field in fields {
                if field.name != "_" {
                    bound.insert(field.name.clone());
                }
            }
        }
        CorePattern::StringPattern(segments) => {
            for segment in segments {
                if let crate::terlan_typeck::CoreStringPatternSegment::Capture(capture) = segment {
                    bound.insert(capture.name.clone());
                }
            }
        }
        CorePattern::Wildcard
        | CorePattern::Int(_)
        | CorePattern::Float(_)
        | CorePattern::String(_)
        | CorePattern::Atom(_) => {}
    }
}
