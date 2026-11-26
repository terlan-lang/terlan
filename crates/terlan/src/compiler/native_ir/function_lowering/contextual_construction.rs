//! Retains declared result types on managed constructions in tail position.

use crate::terlan_typeck::{CoreExpr, CoreType};

/// Annotates only tail constructions, preserving evaluation order and non-tail inference.
pub(super) fn contextualize_tail_construction(expr: &CoreExpr, target: &CoreType) -> CoreExpr {
    match expr {
        CoreExpr::ConstructorCall { .. } | CoreExpr::RecordConstruct { .. } => CoreExpr::Cast {
            expr: Box::new(expr.clone()),
            target_type: target.clone(),
        },
        CoreExpr::Let { bindings, body } => {
            let mut bindings = bindings.clone();
            if let CoreExpr::Var(result) = body.as_ref() {
                if let Some(binding) = bindings.iter_mut().rev().find(|binding| {
                    matches!(&binding.pattern, crate::terlan_typeck::CorePattern::Var(name) if name == result)
                }) {
                    binding.value = super::super::case_lowering::contextualize_control_result(
                        binding.value.clone(),
                        target,
                    );
                    return CoreExpr::Let {
                        bindings,
                        body: body.clone(),
                    };
                }
            }
            CoreExpr::Let {
                bindings,
                body: Box::new(contextualize_tail_construction(body, target)),
            }
        }
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
