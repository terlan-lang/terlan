use super::*;
use crate::terlan_typeck::{CoreCaseClause, CoreIfClause};

fn control(branches: Vec<CoreExpr>, conditional: bool) -> CoreExpr {
    if conditional {
        CoreExpr::If {
            clauses: branches
                .into_iter()
                .map(|body| CoreIfClause {
                    condition: CoreExpr::Atom("true".into()),
                    body,
                })
                .collect(),
        }
    } else {
        CoreExpr::Case {
            scrutinee: Box::new(CoreExpr::Int(0)),
            clauses: branches
                .into_iter()
                .map(|body| CoreCaseClause {
                    pattern: CorePattern::Wildcard,
                    guard: None,
                    body,
                })
                .collect(),
        }
    }
}

fn infer(expression: &mut CoreExpr) -> Option<CoreType> {
    specialize_expr(expression, &HashMap::new(), &HashMap::new(), "app.Branches")
}

#[test]
fn empty_branch_does_not_erase_nonempty_branch_in_either_order() {
    for conditional in [false, true] {
        for reversed in [false, true] {
            let mut branches = vec![
                CoreExpr::List(vec![]),
                CoreExpr::List(vec![CoreExpr::Int(7)]),
            ];
            if reversed {
                branches.reverse();
            }
            let mut expression = CoreExpr::Let {
                bindings: vec![CoreLetBinding {
                    pattern: CorePattern::Var("values".into()),
                    value: control(branches, conditional),
                }],
                body: Box::new(CoreExpr::Var("values".into())),
            };
            assert_eq!(
                infer(&mut expression),
                Some(CoreType::List(Box::new(CoreType::Int)))
            );
            let CoreExpr::Let { body, .. } = &expression else {
                unreachable!()
            };
            assert_eq!(body.as_ref(), &CoreExpr::Var("values".into()));
            let once = expression.clone();
            infer(&mut expression);
            assert_eq!(expression, once);
        }
    }
}

#[test]
fn missing_or_incompatible_branch_witness_cannot_be_recovered_by_later_branches() {
    for conditional in [false, true] {
        for unknown in [
            CoreExpr::Var("unknown".into()),
            CoreExpr::List(vec![CoreExpr::Binary("\"text\"".into())]),
        ] {
            for index in 0..3 {
                let mut branches = vec![CoreExpr::List(vec![CoreExpr::Int(7)]); 2];
                branches.insert(index, unknown.clone());
                assert_eq!(infer(&mut control(branches, conditional)), None);
            }
        }
        assert_eq!(infer(&mut control(vec![], conditional)), None);
        assert_eq!(
            infer(&mut control(vec![CoreExpr::List(vec![]); 2], conditional)),
            Some(CoreType::List(Box::new(CoreType::Never)))
        );
    }
}

#[test]
fn unknown_element_invalidates_collection_witness_without_skipping_later_elements() {
    let mut expression = CoreExpr::List(vec![
        CoreExpr::Var("unknown".into()),
        CoreExpr::Index {
            base: Box::new(CoreExpr::Var("values".into())),
            index: Box::new(CoreExpr::Int(0)),
        },
    ]);
    let variables = HashMap::from([("values".into(), CoreType::List(Box::new(CoreType::Int)))]);
    assert_eq!(
        specialize_expr(&mut expression, &variables, &HashMap::new(), "app.Branches"),
        Some(CoreType::List(Box::new(CoreType::Dynamic)))
    );
    let CoreExpr::List(items) = expression else {
        panic!("unknown element must not invent a list schema")
    };
    assert!(matches!(&items[1], CoreExpr::Intrinsic(call)
        if call.id == CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListGet)));
}

#[test]
fn closure_list_branch_retains_its_callable_witness() {
    let closure = CoreExpr::Lam {
        params: vec![CorePattern::Var("item".into())],
        parameter_types: vec![Some(CoreType::Int)],
        body: Box::new(CoreExpr::Var("item".into())),
    };
    for conditional in [false, true] {
        let mut expression = control(
            vec![
                CoreExpr::List(vec![]),
                CoreExpr::List(vec![closure.clone()]),
            ],
            conditional,
        );
        assert_eq!(
            infer(&mut expression),
            Some(CoreType::List(Box::new(CoreType::Arrow {
                params: vec![CoreType::Int],
                return_type: Box::new(CoreType::Int),
            })))
        );
    }
    let mut unknown = CoreExpr::Lam {
        params: vec![CorePattern::Var("item".into())],
        parameter_types: vec![None],
        body: Box::new(CoreExpr::Var("item".into())),
    };
    assert_eq!(
        specialize_expr(
            &mut unknown,
            &HashMap::from([("item".into(), CoreType::Int)]),
            &HashMap::new(),
            "app.Branches"
        ),
        None
    );
}
