use super::*;
use crate::terlan_typeck::{CoreLetBinding, CorePattern};

#[test]
fn lambda_inference_requires_checked_parameters_and_preserves_shadowing() {
    let outer = HashMap::from([("value".to_string(), CoreType::String)]);
    let mut expression = CoreExpr::Lam {
        params: vec![CorePattern::Var("value".to_string())],
        parameter_types: vec![Some(CoreType::Int)],
        body: Box::new(CoreExpr::Var("value".to_string())),
    };
    assert_eq!(
        infer_type(&expression, &outer, &Default::default(), "scope"),
        Some(CoreType::Arrow {
            params: vec![CoreType::Int],
            return_type: Box::new(CoreType::Int)
        })
    );
    let CoreExpr::Lam {
        parameter_types, ..
    } = &mut expression
    else {
        unreachable!()
    };
    parameter_types[0] = None;
    assert_eq!(
        infer_type(&expression, &outer, &Default::default(), "scope"),
        None
    );
    assert_eq!(outer["value"], CoreType::String);
}

#[test]
fn let_inference_uses_local_patterns_and_does_not_leak_shadowed_types() {
    let outer = HashMap::from([("value".to_string(), CoreType::String)]);
    for (value, expected) in [
        (CoreExpr::Int(42), Some(CoreType::Int)),
        (CoreExpr::Var("unknown".to_string()), None),
    ] {
        let expression = CoreExpr::Let {
            bindings: vec![CoreLetBinding {
                pattern: CorePattern::Var("value".to_string()),
                value,
            }],
            body: Box::new(CoreExpr::Var("value".to_string())),
        };
        assert_eq!(
            infer_type(&expression, &outer, &Default::default(), "scope"),
            expected
        );
    }
    assert_eq!(outer.get("value"), Some(&CoreType::String));
    let expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Tuple(vec![
                CorePattern::Var("value".to_string()),
                CorePattern::Var("other".to_string()),
            ]),
            value: CoreExpr::Tuple(vec![CoreExpr::Int(42), CoreExpr::Atom("true".to_string())]),
        }],
        body: Box::new(CoreExpr::Var("other".to_string())),
    };
    assert_eq!(
        infer_type(&expression, &outer, &Default::default(), "scope"),
        Some(CoreType::Bool)
    );
}
