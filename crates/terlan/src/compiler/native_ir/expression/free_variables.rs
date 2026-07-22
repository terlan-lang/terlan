//! Free-variable discovery for checked CoreIR expressions.

use std::collections::HashSet;

use crate::terlan_typeck::{CoreExpr, CorePattern};

/// Returns the names read by an expression outside its lexical bindings.
pub(in crate::compiler::native_ir) fn free_variables(expr: &CoreExpr) -> HashSet<String> {
    let mut free = HashSet::new();
    collect_free_variables(expr, &mut HashSet::new(), &mut free);
    free
}

/// Traverses one expression while tracking lexical bindings and free reads.
fn collect_free_variables(
    expr: &CoreExpr,
    bound: &mut HashSet<String>,
    free: &mut HashSet<String>,
) {
    match expr {
        CoreExpr::Var(name) if !matches!(name.as_str(), "Unit" | "true" | "false") => {
            if !bound.contains(name) {
                free.insert(name.clone());
            }
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            collect_many(items, bound, free);
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
            collect_free_variables(head, bound, free);
            collect_free_variables(tail, bound, free);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            let original = bound.clone();
            for generator in generators {
                collect_free_variables(&generator.source, bound, free);
                bind_pattern(&generator.pattern, bound);
            }
            collect_many(guards, bound, free);
            collect_free_variables(expr, bound, free);
            *bound = original;
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                collect_free_variables(&field.value, bound, free);
            }
        }
        CoreExpr::RemoteCall { args, .. }
        | CoreExpr::Call { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            collect_many(args, bound, free);
        }
        CoreExpr::FunctionCall { callee, args } => {
            collect_free_variables(callee, bound, free);
            collect_many(args, bound, free);
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                collect_free_variables(&field.value, bound, free);
            }
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            collect_free_variables(base, bound, free);
            for field in fields {
                collect_free_variables(&field.value, bound, free);
            }
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            collect_many(args, bound, free);
            collect_free_variables(record, bound, free);
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            collect_free_variables(receiver, bound, free);
            collect_many(args, bound, free);
        }
        CoreExpr::UnaryOp { operand, .. } => collect_free_variables(operand, bound, free),
        CoreExpr::FieldAccess { base, .. }
        | CoreExpr::RecordAccess { base, .. }
        | CoreExpr::Cast { expr: base, .. } => collect_free_variables(base, bound, free),
        CoreExpr::Let { bindings, body } => {
            let original = bound.clone();
            for binding in bindings {
                collect_free_variables(&binding.value, bound, free);
                bind_pattern(&binding.pattern, bound);
            }
            collect_free_variables(body, bound, free);
            *bound = original;
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                collect_free_variables(&clause.condition, bound, free);
                collect_free_variables(&clause.body, bound, free);
            }
        }
        CoreExpr::Case { scrutinee, clauses } => {
            collect_free_variables(scrutinee, bound, free);
            collect_case_clauses(clauses, bound, free);
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            collect_free_variables(body, bound, free);
            collect_case_clauses(of_clauses, bound, free);
            collect_case_clauses(catch_clauses, bound, free);
            if let Some(after) = after_clause {
                collect_free_variables(&after.trigger, bound, free);
                collect_free_variables(&after.body, bound, free);
            }
        }
        CoreExpr::Lam { params, body } => {
            let original = bound.clone();
            for pattern in params {
                bind_pattern(pattern, bound);
            }
            collect_free_variables(body, bound, free);
            *bound = original;
        }
        CoreExpr::SqlQuery { parameters, .. } => collect_many(parameters, bound, free),
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::RemoteFunRef { .. }
        | CoreExpr::Var(_) => {}
    }
}

fn collect_many(expressions: &[CoreExpr], bound: &mut HashSet<String>, free: &mut HashSet<String>) {
    for expression in expressions {
        collect_free_variables(expression, bound, free);
    }
}

fn collect_case_clauses(
    clauses: &[crate::terlan_typeck::CoreCaseClause],
    bound: &mut HashSet<String>,
    free: &mut HashSet<String>,
) {
    for clause in clauses {
        let original = bound.clone();
        bind_pattern(&clause.pattern, bound);
        if let Some(guard) = &clause.guard {
            collect_free_variables(guard, bound, free);
        }
        collect_free_variables(&clause.body, bound, free);
        *bound = original;
    }
}

fn bind_pattern(pattern: &CorePattern, bound: &mut HashSet<String>) {
    match pattern {
        CorePattern::Var(name) => {
            bound.insert(name.clone());
        }
        CorePattern::Alias { alias, pattern } => {
            bound.insert(alias.clone());
            bind_pattern(pattern, bound);
        }
        CorePattern::Tuple(patterns) | CorePattern::List(patterns) => {
            for pattern in patterns {
                bind_pattern(pattern, bound);
            }
        }
        CorePattern::ListCons { head, tail } => {
            bind_pattern(head, bound);
            bind_pattern(tail, bound);
        }
        CorePattern::Map(fields) => {
            for field in fields {
                bind_pattern(&field.value, bound);
            }
        }
        CorePattern::Record { fields, .. } => {
            for field in fields {
                bind_pattern(&field.value, bound);
            }
        }
        CorePattern::Constructor { args, .. } => {
            for pattern in args {
                bind_pattern(pattern, bound);
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
