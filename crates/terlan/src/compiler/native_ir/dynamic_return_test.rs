//! Tests for fail-closed concrete type recovery at dynamic compiler boundaries.

use std::collections::HashMap;

use crate::terlan_typeck::{CoreExpr, CoreTupleTypeElem, CoreType};

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
