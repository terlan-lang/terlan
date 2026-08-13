//! Tests for fail-closed concrete type recovery at dynamic compiler boundaries.

use std::collections::HashMap;

use crate::terlan_typeck::{CoreExpr, CoreLetBinding, CorePattern, CoreTupleTypeElem, CoreType};

use super::dynamic_return::infer_expression_type;

/// Structural recovery admits homogeneous managed lists and nested tuples.
#[test]
fn dynamic_return_recovers_concrete_managed_shapes() {
    let expression = CoreExpr::Tuple(vec![
        CoreExpr::List(vec![CoreExpr::Int(1), CoreExpr::Int(2)]),
        CoreExpr::Binary("native".to_string()),
    ]);

    assert_eq!(
        infer_expression_type(&expression, &HashMap::new()),
        Some(CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::List(Box::new(CoreType::Int))),
            CoreTupleTypeElem::Type(CoreType::String),
        ]))
    );
}

/// Ambiguous and heterogeneous collection boundaries remain unsupported.
#[test]
fn dynamic_return_rejects_ambiguous_managed_shapes() {
    assert_eq!(
        infer_expression_type(&CoreExpr::List(Vec::new()), &HashMap::new()),
        None
    );
    assert_eq!(
        infer_expression_type(
            &CoreExpr::List(vec![CoreExpr::Int(1), CoreExpr::Binary("two".to_string())]),
            &HashMap::new(),
        ),
        None
    );
}

/// Effectful sequence temporaries do not obscure an independent final value.
#[test]
fn dynamic_return_ignores_unreferenced_unknown_sequence_results() {
    let expression = CoreExpr::Let {
        bindings: vec![
            CoreLetBinding {
                pattern: CorePattern::Var("answer".to_string()),
                value: CoreExpr::Int(42),
            },
            CoreLetBinding {
                pattern: CorePattern::Var("_script_effect".to_string()),
                value: CoreExpr::RemoteCall {
                    module: "std.vm.Process".to_string(),
                    function: "fail".to_string(),
                    args: vec![CoreExpr::Int(1)],
                },
            },
        ],
        body: Box::new(CoreExpr::Var("answer".to_string())),
    };

    assert_eq!(
        infer_expression_type(&expression, &HashMap::new()),
        Some(CoreType::Int)
    );
}

/// Unknown shadowing remains unknown instead of leaking the outer type.
#[test]
fn dynamic_return_rejects_unknown_shadowed_result() {
    let variables = HashMap::from([("answer".to_string(), CoreType::Int)]);
    let expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("answer".to_string()),
            value: CoreExpr::RemoteCall {
                module: "unknown.Module".to_string(),
                function: "value".to_string(),
                args: Vec::new(),
            },
        }],
        body: Box::new(CoreExpr::Var("answer".to_string())),
    };

    assert_eq!(infer_expression_type(&expression, &variables), None);
}
