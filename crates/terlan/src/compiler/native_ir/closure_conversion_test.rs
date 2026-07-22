//! Focused checks for bounded escaping-lambda closure conversion.

use std::collections::{HashMap, HashSet};

use crate::terlan_typeck::{CoreExpr, CoreIfClause, CorePattern, CoreType};

use super::{
    closure_conversion::{lower_escaping_closure, lower_escaping_lambda, NativeCallableShape},
    NativeBinaryOperator, NativeConstructorLayouts, NativeExpr, NativeType,
};

fn arrow(arity: usize) -> CoreType {
    CoreType::Arrow {
        params: vec![CoreType::Int; arity],
        return_type: Box::new(CoreType::Int),
    }
}

fn lower(
    body: &CoreExpr,
    expected: &CoreType,
    outer_params: &HashMap<String, usize>,
    outer_types: &HashMap<String, NativeType>,
    identities: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    suspending: &HashSet<(String, usize)>,
) -> Result<Option<(NativeExpr, super::NativeFunction)>, String> {
    lower_escaping_lambda(
        body,
        Some(expected),
        outer_params,
        outer_types,
        identities,
        function_types,
        &NativeConstructorLayouts::new(),
        suspending,
        "closure_test",
        "make",
        outer_params.len(),
    )
}

#[test]
fn captured_parameters_are_snapshotted_in_stable_name_order() {
    let lambda = CoreExpr::Lam {
        params: vec![CorePattern::Var("value".to_string())],
        body: Box::new(CoreExpr::BinaryOp {
            operator: "+".to_string(),
            left: Box::new(CoreExpr::BinaryOp {
                operator: "+".to_string(),
                left: Box::new(CoreExpr::Var("value".to_string())),
                right: Box::new(CoreExpr::Var("zebra".to_string())),
            }),
            right: Box::new(CoreExpr::Var("alpha".to_string())),
        }),
    };
    let outer_params = HashMap::from([("zebra".to_string(), 0), ("alpha".to_string(), 1)]);
    let outer_types = HashMap::from([
        ("zebra".to_string(), NativeType::Int),
        ("alpha".to_string(), NativeType::Int),
    ]);

    let (maker, lifted) = lower(
        &lambda,
        &arrow(1),
        &outer_params,
        &outer_types,
        &HashMap::new(),
        &HashMap::new(),
        &HashSet::new(),
    )
    .expect("closure conversion")
    .expect("escaping lambda");

    let NativeExpr::MakeClosure { captures, .. } = maker else {
        panic!("expected closure allocation");
    };
    assert_eq!(captures, vec![NativeExpr::Param(1), NativeExpr::Param(0)]);
    assert_eq!(
        lifted.callable_captures,
        vec![NativeType::Int, NativeType::Int]
    );
    assert_eq!(
        lifted.params,
        vec![NativeType::Int, NativeType::Int, NativeType::Int]
    );
}

#[test]
fn scalar_lexical_prefix_is_evaluated_before_local_capture_snapshot() {
    let expression = CoreExpr::Let {
        bindings: vec![crate::terlan_typeck::CoreLetBinding {
            pattern: CorePattern::Var("offset".to_string()),
            value: CoreExpr::BinaryOp {
                operator: "+".to_string(),
                left: Box::new(CoreExpr::Var("seed".to_string())),
                right: Box::new(CoreExpr::Int(1)),
            },
        }],
        body: Box::new(CoreExpr::Lam {
            params: vec![CorePattern::Var("value".to_string())],
            body: Box::new(CoreExpr::BinaryOp {
                operator: "+".to_string(),
                left: Box::new(CoreExpr::Var("value".to_string())),
                right: Box::new(CoreExpr::Var("offset".to_string())),
            }),
        }),
    };
    let available = HashMap::from([("seed".to_string(), 0)]);
    let available_types = HashMap::from([("seed".to_string(), NativeType::Int)]);

    let (maker, lifted) = lower_escaping_closure(
        &expression,
        Some(&arrow(1)),
        &available,
        &available_types,
        &HashMap::new(),
        &HashMap::new(),
        &NativeConstructorLayouts::new(),
        &HashSet::new(),
        &HashMap::new(),
        "closure_test",
        "make",
        1,
    )
    .expect("closure conversion")
    .expect("escaping closure");

    let NativeExpr::Let { bindings, body } = maker else {
        panic!("expected lexical prefix");
    };
    assert_eq!(bindings.len(), 1);
    assert!(matches!(
        &bindings[0],
        NativeExpr::Binary {
            operator: NativeBinaryOperator::Add,
            left,
            right,
            ..
        } if **left == NativeExpr::Param(0) && **right == NativeExpr::Int(1)
    ));
    assert!(matches!(
        body.as_ref(),
        NativeExpr::MakeClosure { captures, .. }
            if captures == &vec![NativeExpr::Param(1)]
    ));
    assert_eq!(lifted.len(), 1);
    assert_eq!(lifted[0].callable_captures, vec![NativeType::Int]);
}

#[test]
fn non_closure_let_bypasses_closure_prefix_validation() {
    let expression = CoreExpr::Let {
        bindings: vec![crate::terlan_typeck::CoreLetBinding {
            pattern: CorePattern::Var("value".to_string()),
            value: CoreExpr::Call {
                function: "pause".to_string(),
                args: Vec::new(),
            },
        }],
        body: Box::new(CoreExpr::Var("value".to_string())),
    };
    let suspending = HashSet::from([("pause".to_string(), 0)]);

    assert_eq!(
        lower_escaping_closure(
            &expression,
            Some(&CoreType::Int),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &NativeConstructorLayouts::new(),
            &suspending,
            &HashMap::new(),
            "closure_test",
            "ordinary",
            0,
        )
        .expect("non-closure expression"),
        None
    );
}

#[test]
fn closure_branches_receive_distinct_ordered_lifted_identities() {
    let lambda = |operator: &str| CoreExpr::Lam {
        params: vec![CorePattern::Var("value".to_string())],
        body: Box::new(CoreExpr::BinaryOp {
            operator: operator.to_string(),
            left: Box::new(CoreExpr::Var("value".to_string())),
            right: Box::new(CoreExpr::Var("seed".to_string())),
        }),
    };
    let expression = CoreExpr::If {
        clauses: vec![
            CoreIfClause {
                condition: CoreExpr::Var("forward".to_string()),
                body: lambda("+"),
            },
            CoreIfClause {
                condition: CoreExpr::Atom("true".to_string()),
                body: lambda("-"),
            },
        ],
    };
    let available = HashMap::from([("forward".to_string(), 0), ("seed".to_string(), 1)]);
    let available_types = HashMap::from([
        ("forward".to_string(), NativeType::Bool),
        ("seed".to_string(), NativeType::Int),
    ]);

    let (maker, lifted) = lower_escaping_closure(
        &expression,
        Some(&arrow(1)),
        &available,
        &available_types,
        &HashMap::new(),
        &HashMap::new(),
        &NativeConstructorLayouts::new(),
        &HashSet::new(),
        &HashMap::new(),
        "closure_test",
        "choose",
        2,
    )
    .expect("branch closure conversion")
    .expect("escaping branch closures");

    let NativeExpr::If { clauses } = maker else {
        panic!("expected native closure branch");
    };
    assert_eq!(clauses.len(), 2);
    assert_eq!(lifted.len(), 2);
    assert_eq!(lifted[0].name, "$closure_choose_2_0");
    assert_eq!(lifted[1].name, "$closure_choose_2_1");
    assert_ne!(lifted[0].export_id, lifted[1].export_id);
    assert!(lifted
        .iter()
        .all(|function| function.callable_captures == vec![NativeType::Int]));
}

#[test]
fn closure_branch_rejects_a_non_callable_arm() {
    let expression = CoreExpr::If {
        clauses: vec![CoreIfClause {
            condition: CoreExpr::Atom("true".to_string()),
            body: CoreExpr::Int(1),
        }],
    };

    assert_eq!(
        lower_escaping_closure(
            &expression,
            Some(&arrow(1)),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &NativeConstructorLayouts::new(),
            &HashSet::new(),
            &HashMap::new(),
            "closure_test",
            "choose",
            0,
        )
        .unwrap_err(),
        "error[native_ir.closure_branch]: every escaping closure branch must produce a callable value"
    );
}

#[test]
fn closure_branch_rejects_a_suspending_condition() {
    let expression = CoreExpr::If {
        clauses: vec![CoreIfClause {
            condition: CoreExpr::Call {
                function: "pause".to_string(),
                args: Vec::new(),
            },
            body: CoreExpr::Lam {
                params: vec![CorePattern::Var("value".to_string())],
                body: Box::new(CoreExpr::Var("value".to_string())),
            },
        }],
    };

    assert_eq!(
        lower_escaping_closure(
            &expression,
            Some(&arrow(1)),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &NativeConstructorLayouts::new(),
            &HashSet::from([("pause".to_string(), 0)]),
            &HashMap::new(),
            "closure_test",
            "choose",
            0,
        )
        .unwrap_err(),
        "error[native_ir.closure_branch_suspension]: escaping closure branch condition cannot suspend"
    );
}

#[test]
fn closure_branch_budget_rejects_more_than_sixty_four_clauses() {
    let expression = CoreExpr::If {
        clauses: (0..65)
            .map(|_| CoreIfClause {
                condition: CoreExpr::Atom("true".to_string()),
                body: CoreExpr::Lam {
                    params: vec![CorePattern::Var("value".to_string())],
                    body: Box::new(CoreExpr::Var("value".to_string())),
                },
            })
            .collect(),
    };

    assert_eq!(
        lower_escaping_closure(
            &expression,
            Some(&arrow(1)),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &NativeConstructorLayouts::new(),
            &HashSet::new(),
            &HashMap::new(),
            "closure_test",
            "choose",
            0,
        )
        .unwrap_err(),
        "error[native_ir.closure_branch_limit]: escaping closure branch has 65 clauses; limit is 64"
    );
}

#[test]
fn closure_branch_can_mix_named_and_lifted_targets() {
    let expression = CoreExpr::If {
        clauses: vec![
            CoreIfClause {
                condition: CoreExpr::Var("named".to_string()),
                body: CoreExpr::RemoteFunRef {
                    module: "app.Math".to_string(),
                    function: "double".to_string(),
                    arity: 1,
                },
            },
            CoreIfClause {
                condition: CoreExpr::Atom("true".to_string()),
                body: CoreExpr::Lam {
                    params: vec![CorePattern::Var("value".to_string())],
                    body: Box::new(CoreExpr::Var("value".to_string())),
                },
            },
        ],
    };
    let available = HashMap::from([("named".to_string(), 0)]);
    let available_types = HashMap::from([("named".to_string(), NativeType::Bool)]);
    let callable_shapes = HashMap::from([(
        ("app.Math.double".to_string(), 1),
        NativeCallableShape {
            id: 88,
            parameters: vec![NativeType::Int],
            result: NativeType::Int,
        },
    )]);

    let (maker, lifted) = lower_escaping_closure(
        &expression,
        Some(&arrow(1)),
        &available,
        &available_types,
        &HashMap::new(),
        &HashMap::new(),
        &NativeConstructorLayouts::new(),
        &HashSet::new(),
        &callable_shapes,
        "closure_test",
        "choose",
        1,
    )
    .expect("mixed branch conversion")
    .expect("mixed escaping closures");

    assert_eq!(lifted.len(), 1);
    assert_eq!(lifted[0].name, "$closure_choose_1_0");
    let NativeExpr::If { clauses } = maker else {
        panic!("expected mixed closure branch");
    };
    let NativeExpr::MakeClosure { encoded, captures } = &clauses[0].1 else {
        panic!("named branch did not allocate closure");
    };
    assert!(captures.is_empty());
    assert_eq!(
        u64::from_le_bytes(encoded[16..24].try_into().expect("callable identity")),
        88
    );
}

#[test]
fn escaping_lambda_rejects_non_variable_parameters() {
    let lambda = CoreExpr::Lam {
        params: vec![CorePattern::Wildcard],
        body: Box::new(CoreExpr::Int(1)),
    };

    assert_eq!(
        lower(
            &lambda,
            &arrow(1),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
        )
        .unwrap_err(),
        "error[native_ir.closure_parameter]: escaping lambda parameters must be variables"
    );
}

#[test]
fn escaping_lambda_rejects_declared_arity_drift() {
    let lambda = CoreExpr::Lam {
        params: Vec::new(),
        body: Box::new(CoreExpr::Int(1)),
    };

    assert_eq!(
        lower(
            &lambda,
            &arrow(1),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
        )
        .unwrap_err(),
        "error[native_ir.closure_arity]: escaping lambda declares 0 parameters but its type requires 1"
    );
}

#[test]
fn escaping_lambda_rejects_untyped_non_parameter_captures() {
    let lambda = CoreExpr::Lam {
        params: vec![CorePattern::Var("value".to_string())],
        body: Box::new(CoreExpr::BinaryOp {
            operator: "+".to_string(),
            left: Box::new(CoreExpr::Var("value".to_string())),
            right: Box::new(CoreExpr::Var("local".to_string())),
        }),
    };

    assert_eq!(
        lower(
            &lambda,
            &arrow(1),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
        )
        .unwrap_err(),
        "error[native_ir.closure_capture]: `local` is not a typed lexical value"
    );
}

#[test]
fn escaping_lambda_capture_budget_has_stable_prelink_rejection() {
    let lambda = CoreExpr::Lam {
        params: Vec::new(),
        body: Box::new(CoreExpr::Tuple(
            (0..65)
                .map(|index| CoreExpr::Var(format!("capture_{index}")))
                .collect(),
        )),
    };
    let outer_params = (0..65)
        .map(|index| (format!("capture_{index}"), index))
        .collect::<HashMap<_, _>>();
    let outer_types = outer_params
        .keys()
        .cloned()
        .map(|name| (name, NativeType::Int))
        .collect::<HashMap<_, _>>();

    assert_eq!(
        lower(
            &lambda,
            &arrow(0),
            &outer_params,
            &outer_types,
            &HashMap::new(),
            &HashMap::new(),
            &HashSet::new(),
        )
        .unwrap_err(),
        "error[native_ir.closure_capture_limit]: escaping lambda captures 65 values; limit is 64"
    );
}

#[test]
fn escaping_lambda_tail_calls_one_admitted_suspending_target() {
    let lambda = CoreExpr::Lam {
        params: Vec::new(),
        body: Box::new(CoreExpr::Call {
            function: "pause".to_string(),
            args: Vec::new(),
        }),
    };
    let identities = HashMap::from([(("pause".to_string(), 0), 0)]);
    let function_types = HashMap::from([(("pause".to_string(), 0), NativeType::Int)]);
    let suspending = HashSet::from([("pause".to_string(), 0)]);

    let (_, lifted) = lower(
        &lambda,
        &arrow(0),
        &HashMap::new(),
        &HashMap::new(),
        &identities,
        &function_types,
        &suspending,
    )
    .expect("suspending closure lowering")
    .expect("escaping closure");
    assert_eq!(
        lifted.body,
        NativeExpr::TailCall {
            function: 0,
            args: vec![],
        }
    );
}
