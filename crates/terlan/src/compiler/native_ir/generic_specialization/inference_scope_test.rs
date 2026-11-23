use super::*;
use crate::terlan_typeck::{CoreLetBinding, CorePattern};

#[test]
fn record_updates_preserve_the_receivers_concrete_generic_type() {
    let ty = CoreType::Apply {
        constructor: "app.Holder".into(),
        args: vec![CoreType::Int],
    };
    let variables = HashMap::from([("base".into(), ty.clone())]);
    let expr = CoreExpr::RecordUpdate {
        base: Box::new(CoreExpr::Var("base".into())),
        name: "app.Holder".into(),
        fields: vec![],
    };
    assert_eq!(
        infer_type(&expr, &variables, &Default::default(), "app"),
        Some(ty)
    );
    assert_eq!(
        infer_type(&expr, &Default::default(), &Default::default(), "app"),
        None
    );
}

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

#[test]
fn numeric_binary_inference_promotes_float_in_either_operand() {
    let variables = HashMap::from([
        ("integer".to_string(), CoreType::Int),
        ("floating".to_string(), CoreType::Float),
        ("number".to_string(), CoreType::Number),
        ("text".to_string(), CoreType::String),
        ("boolean".to_string(), CoreType::Bool),
    ]);
    for (operator, left, right, expected) in [
        ("*", "integer", "floating", CoreType::Float),
        ("*", "floating", "integer", CoreType::Float),
        ("+", "integer", "integer", CoreType::Int),
        ("+", "number", "integer", CoreType::Number),
        ("+", "text", "text", CoreType::String),
        ("div", "integer", "integer", CoreType::Int),
        ("&", "integer", "integer", CoreType::Int),
        ("<", "integer", "floating", CoreType::Bool),
        ("and", "boolean", "boolean", CoreType::Bool),
    ] {
        let expression = CoreExpr::BinaryOp {
            operator: operator.to_string(),
            left: Box::new(CoreExpr::Var(left.to_string())),
            right: Box::new(CoreExpr::Var(right.to_string())),
        };
        assert_eq!(
            infer_type(&expression, &variables, &Default::default(), "scope"),
            Some(expected),
        );
    }
    assert_eq!(
        super::super::super::expression::infer_core_binary_type(
            "unsupported",
            Some(CoreType::Int),
            Some(CoreType::Int),
        ),
        None,
    );
    assert_eq!(
        super::super::super::expression::infer_core_binary_type("+", None, Some(CoreType::Int),),
        None,
    );
}
