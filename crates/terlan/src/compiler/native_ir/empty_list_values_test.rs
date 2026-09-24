use super::*;

#[test]
fn boundary_adaptation_uses_only_the_exact_bottom_list_witness() {
    let bottom = CoreType::List(Box::new(CoreType::Never));
    let expected = CoreType::List(Box::new(CoreType::Int));
    let source = CoreExpr::Var("values".into());
    for ty in [
        bottom.clone(),
        expected.clone(),
        CoreType::List(Box::new(bottom.clone())),
        CoreType::Apply {
            constructor: "app.List".into(),
            args: vec![CoreType::Never],
        },
    ] {
        let variables = HashMap::from([(
            "values".into(),
            crate::compiler::native_ir::native_type(Some(&ty), &ty.contract_text()).unwrap(),
        )]);
        let adapted = at_boundary(
            &source,
            &expected,
            &variables,
            &HashMap::new(),
            &HashMap::new(),
        );
        assert_eq!(adapted.is_some(), ty == bottom);
        assert!(at_boundary(
            &source,
            &bottom,
            &variables,
            &HashMap::new(),
            &HashMap::new()
        )
        .is_none());
    }
    assert!(at_boundary(
        &CoreExpr::List(vec![]),
        &expected,
        &HashMap::new(),
        &HashMap::new(),
        &HashMap::new()
    )
    .is_none());
}

#[test]
fn boundary_adaptation_retains_a_callable_producer_once() {
    let bottom = CoreType::List(Box::new(CoreType::Never));
    let source = CoreExpr::Call {
        function: "produce".into(),
        type_args: vec![],
        args: vec![],
    };
    let functions = HashMap::from([(
        ("produce".into(), 0),
        crate::compiler::native_ir::native_type(Some(&bottom), &bottom.contract_text()).unwrap(),
    )]);
    let adapted = at_boundary(
        &source,
        &CoreType::List(Box::new(CoreType::Int)),
        &HashMap::new(),
        &functions,
        &HashMap::new(),
    )
    .unwrap();
    let CoreExpr::Let { bindings, .. } = adapted else {
        panic!("producer sequence")
    };
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].value, source);
}

#[test]
fn bottom_adaptation_retains_one_evaluation_of_the_producer() {
    let source = CoreExpr::Call {
        function: "produce_with_effects".to_string(),
        type_args: vec![],
        args: vec![CoreExpr::Int(42)],
    };
    let expected = CoreType::List(Box::new(CoreType::String));
    let mut expression = source.clone();
    coerce(
        &mut expression,
        &CoreType::List(Box::new(CoreType::Never)),
        &expected,
    );
    let CoreExpr::Let { bindings, body } = &expression else {
        panic!("producer must execute");
    };
    assert_eq!(bindings.len(), 1);
    assert_eq!(bindings[0].value, source);
    assert_eq!(
        **body,
        CoreExpr::Cast {
            expr: Box::new(CoreExpr::List(vec![])),
            target_type: expected.clone()
        }
    );
    let once = expression.clone();
    coerce(&mut expression, &expected, &expected);
    assert_eq!(expression, once);
}

#[test]
fn bottom_adaptation_does_not_retype_concrete_or_user_defined_collections() {
    for actual in [
        CoreType::List(Box::new(CoreType::Int)),
        CoreType::Apply {
            constructor: "app.List".to_string(),
            args: vec![CoreType::Never],
        },
        CoreType::List(Box::new(CoreType::List(Box::new(CoreType::Never)))),
    ] {
        let source = CoreExpr::Var("values".to_string());
        let mut expression = source.clone();
        coerce(
            &mut expression,
            &actual,
            &CoreType::List(Box::new(CoreType::String)),
        );
        assert_eq!(expression, source);
    }
}
