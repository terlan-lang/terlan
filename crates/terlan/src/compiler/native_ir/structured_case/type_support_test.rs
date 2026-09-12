//! Disjointness must prove every closed alternative impossible before pruning.

use super::type_support::type_excludes_pattern;
use crate::terlan_typeck::{CorePattern, CoreTupleTypeElem, CoreType};

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
