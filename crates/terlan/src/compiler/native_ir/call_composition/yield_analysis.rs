// Recursive yield counts and branch-ambiguity analysis for call composition.

use super::*;

pub(super) fn process_yield_count(expr: &CoreExpr) -> usize {
    let own = usize::from(is_process_transition(expr));
    own + match expr {
        CoreExpr::Call { args, .. }
        | CoreExpr::RemoteCall { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            args.iter().map(process_yield_count).sum()
        }
        CoreExpr::FunctionCall { callee, args } => {
            process_yield_count(callee) + args.iter().map(process_yield_count).sum::<usize>()
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            items.iter().map(process_yield_count).sum()
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => process_yield_count(head) + process_yield_count(tail),
        CoreExpr::Map(fields) => fields
            .iter()
            .map(|field| process_yield_count(&field.value))
            .sum(),
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            fields
                .iter()
                .map(|field| process_yield_count(&field.value))
                .sum()
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            process_yield_count(base)
                + fields
                    .iter()
                    .map(|field| process_yield_count(&field.value))
                    .sum::<usize>()
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            process_yield_count(base)
        }
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            process_yield_count(receiver) + args.iter().map(process_yield_count).sum::<usize>()
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            args.iter().map(process_yield_count).sum::<usize>() + process_yield_count(record)
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            process_yield_count(expr)
                + generators
                    .iter()
                    .map(|generator| process_yield_count(&generator.source))
                    .sum::<usize>()
                + guards.iter().map(process_yield_count).sum::<usize>()
        }
        CoreExpr::UnaryOp { operand, .. } | CoreExpr::Cast { expr: operand, .. } => {
            process_yield_count(operand)
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            process_yield_count(left) + process_yield_count(right)
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .map(|binding| process_yield_count(&binding.value))
                .sum::<usize>()
                + process_yield_count(body)
        }
        CoreExpr::If { clauses } => clauses
            .iter()
            .map(|clause| {
                process_yield_count(&clause.condition) + process_yield_count(&clause.body)
            })
            .sum(),
        CoreExpr::Case { scrutinee, clauses } => {
            process_yield_count(scrutinee)
                + clauses
                    .iter()
                    .map(|clause| {
                        clause.guard.as_ref().map_or(0, process_yield_count)
                            + process_yield_count(&clause.body)
                    })
                    .sum::<usize>()
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            process_yield_count(body)
                + of_clauses
                    .iter()
                    .chain(catch_clauses)
                    .map(|clause| {
                        clause.guard.as_ref().map_or(0, process_yield_count)
                            + process_yield_count(&clause.body)
                    })
                    .sum::<usize>()
                + after_clause.as_ref().map_or(0, |after| {
                    process_yield_count(&after.trigger) + process_yield_count(&after.body)
                })
        }
        CoreExpr::SqlQuery { parameters, .. } => parameters.iter().map(process_yield_count).sum(),
        _ => 0,
    }
}
