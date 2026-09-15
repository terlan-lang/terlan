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
