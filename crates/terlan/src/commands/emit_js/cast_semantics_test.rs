use super::*;

#[test]
fn typed_lambda_js_casts_preserve_parameter_contracts() {
    let lambda = CoreExpr::Lam {
        params: vec![crate::terlan_typeck::CorePattern::Var("value".into())],
        parameter_types: vec![Some(CoreType::Int)],
        body: Box::new(CoreExpr::Int(1)),
    };
    let arrow = |parameter| CoreType::Arrow {
        params: vec![parameter],
        return_type: Box::new(CoreType::Int),
    };
    assert!(cast_can_lower_as_js_identity(
        &lambda,
        &arrow(CoreType::Int)
    ));
    assert!(!cast_can_lower_as_js_identity(
        &lambda,
        &arrow(CoreType::String)
    ));
}

#[test]
fn checked_empty_list_witnesses_are_identity_casts_only_for_list_targets() {
    let integers = CoreType::List(Box::new(CoreType::Int));
    let empty = CoreExpr::List(Vec::new());
    assert!(cast_can_lower_as_js_identity(&empty, &integers));
    assert!(!cast_can_lower_as_js_identity(&empty, &CoreType::Int));
    assert!(!cast_can_lower_as_js_identity(
        &empty,
        &CoreType::Apply {
            constructor: "std.js.Array.Array".into(),
            args: vec![CoreType::Int],
        }
    ));
    assert!(!cast_can_lower_as_js_identity(
        &CoreExpr::List(vec![CoreExpr::Var("unknown".into())]),
        &integers,
    ));
    assert!(!cast_can_lower_as_js_identity(
        &CoreExpr::Cast {
            expr: Box::new(CoreExpr::Var("values".into())),
            target_type: CoreType::List(Box::new(CoreType::Dynamic)),
        },
        &integers,
    ));
}
