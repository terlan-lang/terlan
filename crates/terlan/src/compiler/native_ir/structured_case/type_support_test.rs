//! Disjointness must prove every closed alternative impossible before pruning.

use super::type_support::{core_expr_type, type_excludes_pattern};
use crate::terlan_typeck::{CoreExpr, CoreIfClause, CorePattern, CoreTupleTypeElem, CoreType};
use std::collections::HashMap;

fn tagged(tag: &str, value: CoreType) -> CoreType {
    CoreType::Tuple(vec![
        CoreTupleTypeElem::Type(CoreType::AtomLiteral(tag.into())),
        CoreTupleTypeElem::Type(value),
    ])
}

fn error_pattern() -> CorePattern {
    CorePattern::Tuple(vec![
        CorePattern::Atom("error".into()),
        CorePattern::Var("reason".into()),
    ])
}

#[test]
fn disjoint_closed_union_is_excluded_but_overlapping_union_is_retained() {
    let option = CoreType::Union(vec![
        CoreType::AtomLiteral("none".into()),
        tagged("some", CoreType::String),
    ]);
    assert!(type_excludes_pattern(&error_pattern(), Some(&option)));
    let result = CoreType::Union(vec![
        tagged("ok", CoreType::Int),
        tagged("error", CoreType::String),
    ]);
    assert!(!type_excludes_pattern(&error_pattern(), Some(&result)));
}

#[test]
fn unknown_nominal_and_dynamic_alternatives_are_not_pruned() {
    for ty in [
        CoreType::Dynamic,
        CoreType::Named("External".into()),
        CoreType::Union(vec![tagged("some", CoreType::Int), CoreType::Dynamic]),
    ] {
        assert!(!type_excludes_pattern(&error_pattern(), Some(&ty)));
    }
    assert!(!type_excludes_pattern(&error_pattern(), None));
}

#[test]
fn tuple_arity_and_aliases_use_the_same_disjointness_proof() {
    let alias = CorePattern::Alias {
        alias: "failure".into(),
        pattern: Box::new(error_pattern()),
    };
    assert!(type_excludes_pattern(&alias, Some(&CoreType::Bool)));
    assert!(type_excludes_pattern(
        &alias,
        Some(&CoreType::Tuple(vec![]))
    ));
    assert!(!type_excludes_pattern(
        &alias,
        Some(&tagged("error", CoreType::Int))
    ));
}

#[test]
fn singleton_atom_patterns_are_disjoint_from_products_but_not_unknowns() {
    let pattern = CorePattern::Atom("none".into());
    assert!(type_excludes_pattern(
        &pattern,
        Some(&tagged("some", CoreType::Int))
    ));
    assert!(type_excludes_pattern(
        &pattern,
        Some(&CoreType::AtomLiteral("absent".into()))
    ));
    for ty in [
        CoreType::Atom,
        CoreType::Dynamic,
        CoreType::AtomLiteral("none".into()),
    ] {
        assert!(!type_excludes_pattern(&pattern, Some(&ty)));
    }
}

#[test]
fn unit_expression_alias_and_call_results_share_one_control_join_type() {
    let functions = HashMap::from([(("finish".into(), 0), CoreType::AtomLiteral("unit".into()))]);
    for expression in [CoreExpr::Atom("Unit".into()), CoreExpr::Var("Unit".into())] {
        let joined = CoreExpr::If {
            clauses: vec![
                CoreIfClause {
                    condition: CoreExpr::Atom("true".into()),
                    body: expression,
                },
                CoreIfClause {
                    condition: CoreExpr::Atom("true".into()),
                    body: CoreExpr::Call {
                        type_args: Vec::new(),
                        function: "finish".into(),
                        args: vec![],
                    },
                },
            ],
        };
        assert_eq!(
            core_expr_type(&joined, &HashMap::new(), &functions),
            Some(CoreType::Named("Unit".into()))
        );
    }
    assert_eq!(
        core_expr_type(&CoreExpr::Atom("none".into()), &HashMap::new(), &functions),
        Some(CoreType::AtomLiteral("none".into()))
    );
}
