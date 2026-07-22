//! Tests for compiler-owned finite atom inventory.

use std::collections::BTreeSet;

use crate::terlan_typeck::{
    CoreCaseClause, CoreExpr, CoreMapTypeField, CorePattern, CoreStructTypeField,
    CoreTupleTypeElem, CoreType,
};

use super::atom_inventory::{collect_expr, collect_pattern, collect_type};

/// Proves recursive type inventory is canonical and omits compact scalar singletons.
#[test]
fn type_inventory_collects_nested_atom_literals_canonically() {
    let ty = CoreType::Arrow {
        params: vec![
            CoreType::AtomLiteral("ready".to_owned()),
            CoreType::Tuple(vec![
                CoreTupleTypeElem::Type(CoreType::AtomLiteral("error".to_owned())),
                CoreTupleTypeElem::Field {
                    name: "state".to_owned(),
                    ty: CoreType::AtomLiteral("ready".to_owned()),
                },
            ]),
            CoreType::Struct {
                name: "State".to_owned(),
                fields: vec![CoreStructTypeField {
                    name: "value".to_owned(),
                    ty: CoreType::AtomLiteral("true".to_owned()),
                    is_private: false,
                }],
            },
        ],
        return_type: Box::new(CoreType::Map(vec![CoreMapTypeField {
            key: "status".to_owned(),
            operator: ":".to_owned(),
            value: CoreType::Union(vec![
                CoreType::AtomLiteral("done".to_owned()),
                CoreType::AtomLiteral("Unit".to_owned()),
            ]),
        }])),
    };
    let mut atoms = BTreeSet::new();
    collect_type(&ty, &mut atoms);
    assert_eq!(
        atoms.into_iter().collect::<Vec<_>>(),
        ["done", "error", "ready"]
    );
}

/// Proves expression and pattern inventory reaches branch-local atom identities.
#[test]
fn expression_inventory_collects_branch_patterns_guards_and_bodies() {
    let expr = CoreExpr::Case {
        scrutinee: Box::new(CoreExpr::Atom("pending".to_owned())),
        clauses: vec![CoreCaseClause {
            pattern: CorePattern::Constructor {
                name: "Result".to_owned(),
                constructor_identity: None,
                args: vec![CorePattern::Atom("ready".to_owned())],
            },
            guard: Some(CoreExpr::Atom("true".to_owned())),
            body: CoreExpr::Cast {
                expr: Box::new(CoreExpr::Atom("complete".to_owned())),
                target_type: CoreType::AtomLiteral("complete".to_owned()),
            },
        }],
    };
    let mut atoms = BTreeSet::new();
    collect_expr(&expr, &mut atoms);
    collect_pattern(&CorePattern::Atom("error".to_owned()), &mut atoms);
    assert_eq!(
        atoms.into_iter().collect::<Vec<_>>(),
        ["complete", "error", "pending", "ready"]
    );
}
