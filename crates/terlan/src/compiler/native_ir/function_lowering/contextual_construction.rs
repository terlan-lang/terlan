//! Retains declared result types on managed constructions in tail position.

use crate::terlan_typeck::{CoreExpr, CoreType};

/// Annotates only tail constructions, preserving evaluation order and non-tail inference.
pub(super) fn contextualize_tail_construction(expr: &CoreExpr, target: &CoreType) -> CoreExpr {
    match expr {
        CoreExpr::ConstructorCall { .. } | CoreExpr::RecordConstruct { .. } => CoreExpr::Cast {
            expr: Box::new(expr.clone()),
            target_type: target.clone(),
        },
        CoreExpr::Let { bindings, body } => CoreExpr::Let {
            bindings: bindings.clone(),
            body: Box::new(contextualize_tail_construction(body, target)),
        },
        CoreExpr::If { clauses } => CoreExpr::If {
            clauses: clauses
                .iter()
                .cloned()
                .map(|mut clause| {
                    clause.body = contextualize_tail_construction(&clause.body, target);
                    clause
                })
                .collect(),
        },
        CoreExpr::Case { scrutinee, clauses } => CoreExpr::Case {
            scrutinee: scrutinee.clone(),
            clauses: clauses
                .iter()
                .cloned()
                .map(|mut clause| {
                    clause.body = contextualize_tail_construction(&clause.body, target);
                    clause
                })
                .collect(),
        },
        _ => expr.clone(),
    }
}
