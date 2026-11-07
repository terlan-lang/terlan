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

#[test]
fn linked_list_casts_recover_let_and_list_pattern_binding_types() {
    let coordinate = CoreType::Named("fixture.Coordinate".into());
    let coordinates = CoreType::List(Box::new(coordinate.clone()));
    let returns = HashMap::from([(("coordinate".into(), 0), coordinate.clone())]);
    let mut expression = CoreExpr::Let {
        bindings: vec![crate::terlan_typeck::CoreLetBinding {
            pattern: CorePattern::Var("point".into()),
            value: CoreExpr::Call {
                function: "coordinate".into(),
                type_args: Vec::new(),
                args: Vec::new(),
            },
        }],
        body: Box::new(CoreExpr::Cast {
            expr: Box::new(CoreExpr::List(vec![CoreExpr::Var("point".into())])),
            target_type: coordinates.clone(),
        }),
    };
    assert_eq!(
        erase_expr_identity_casts(&mut expression, &returns, &HashMap::new()),
        Some(coordinates.clone())
    );
    let CoreExpr::Let { body, .. } = expression else {
        panic!("expected let expression");
    };
    assert!(matches!(*body, CoreExpr::List(_)));

    let mut environment = HashMap::new();
    bind_pattern_type(
        &CorePattern::ListCons {
            head: Box::new(CorePattern::Var("first".into())),
            tail: Box::new(CorePattern::Var("rest".into())),
        },
        &coordinates,
        &mut environment,
    );
    assert_eq!(environment.get("first"), Some(&coordinate));
    assert_eq!(environment.get("rest"), Some(&coordinates));
}

#[test]
fn linked_variable_casts_retain_non_identity_conversions() {
    let mut environment = HashMap::new();
    environment.insert("value".into(), CoreType::Int);
    let mut expression = CoreExpr::Cast {
        expr: Box::new(CoreExpr::Var("value".into())),
        target_type: CoreType::String,
    };
    assert_eq!(
        erase_expr_identity_casts(&mut expression, &HashMap::new(), &environment),
        Some(CoreType::String)
    );
    assert!(matches!(expression, CoreExpr::Cast { .. }));
}

#[test]
fn linked_case_lambda_and_comprehension_scopes_erase_only_identity_casts() {
    let coordinate = CoreType::Named("fixture.Coordinate".into());
    let coordinates = CoreType::List(Box::new(coordinate.clone()));
    let mut environment = HashMap::new();
    environment.insert("pending".into(), coordinates.clone());
    let list_cast = |name: &str| CoreExpr::Cast {
        expr: Box::new(CoreExpr::List(vec![CoreExpr::Var(name.into())])),
        target_type: coordinates.clone(),
    };
    let mut case = CoreExpr::Case {
        scrutinee: Box::new(CoreExpr::Var("pending".into())),
        clauses: vec![crate::terlan_typeck::CoreCaseClause {
            pattern: CorePattern::ListCons {
                head: Box::new(CorePattern::Var("point".into())),
                tail: Box::new(CorePattern::Var("rest".into())),
            },
            guard: Some(CoreExpr::Cast {
                expr: Box::new(CoreExpr::Var("point".into())),
                target_type: coordinate.clone(),
            }),
            body: list_cast("point"),
        }],
    };
    assert_eq!(
        erase_expr_identity_casts(&mut case, &HashMap::new(), &environment),
        Some(coordinates.clone())
    );
    let CoreExpr::Case { clauses, .. } = case else {
        panic!("expected case expression");
    };
    assert!(matches!(clauses[0].guard, Some(CoreExpr::Var(_))));
    assert!(matches!(clauses[0].body, CoreExpr::List(_)));

    let mut lambda = CoreExpr::Lam {
        params: vec![CorePattern::Var("point".into())],
        parameter_types: vec![Some(coordinate.clone())],
        body: Box::new(list_cast("point")),
    };
    assert_eq!(
        erase_expr_identity_casts(&mut lambda, &HashMap::new(), &HashMap::new()),
        Some(CoreType::Arrow {
            params: vec![coordinate.clone()],
            return_type: Box::new(coordinates.clone()),
        })
    );

    let mut comprehension = CoreExpr::ListComprehension {
        expr: Box::new(CoreExpr::Cast {
            expr: Box::new(CoreExpr::Var("point".into())),
            target_type: coordinate.clone(),
        }),
        generators: vec![crate::terlan_typeck::CoreListComprehensionGenerator {
            pattern: CorePattern::Var("point".into()),
            source: CoreExpr::Var("pending".into()),
        }],
        guards: vec![CoreExpr::Cast {
            expr: Box::new(CoreExpr::Var("point".into())),
            target_type: coordinate.clone(),
        }],
        lift: None,
    };
    assert_eq!(
        erase_expr_identity_casts(&mut comprehension, &HashMap::new(), &environment,),
        Some(coordinates)
    );
}

#[test]
fn structural_patterns_retain_each_checked_binding_type() {
    let mut environment = HashMap::new();
    bind_pattern_type(
        &CorePattern::Alias {
            alias: "pair".into(),
            pattern: Box::new(CorePattern::Tuple(vec![
                CorePattern::Var("count".into()),
                CorePattern::Var("label".into()),
            ])),
        },
        &CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::Int),
            CoreTupleTypeElem::Field {
                name: "label".into(),
                ty: CoreType::String,
            },
        ]),
        &mut environment,
    );
    assert!(matches!(environment.get("pair"), Some(CoreType::Tuple(_))));
    assert_eq!(environment.get("count"), Some(&CoreType::Int));
    assert_eq!(environment.get("label"), Some(&CoreType::String));

    bind_pattern_type(
        &CorePattern::List(vec![CorePattern::Var("item".into())]),
        &CoreType::List(Box::new(CoreType::Bool)),
        &mut environment,
    );
    bind_pattern_type(
        &CorePattern::Constructor {
            name: "Some".into(),
            constructor_identity: None,
            args: vec![CorePattern::Var("optional".into())],
        },
        &CoreType::Apply {
            constructor: "Option".into(),
            args: vec![CoreType::Float],
        },
        &mut environment,
    );
    assert_eq!(environment.get("item"), Some(&CoreType::Bool));
    assert_eq!(environment.get("optional"), Some(&CoreType::Float));
    assert_eq!(
        constructor_pattern_payload(
            &CoreType::Apply {
                constructor: "Result".into(),
                args: vec![CoreType::Int, CoreType::String],
            },
            "Err",
        ),
        Some(&CoreType::String)
    );
    assert_eq!(
        common_type(&[Some(CoreType::Int), Some(CoreType::Int)]),
        Some(CoreType::Int)
    );
    assert_eq!(
        common_type(&[Some(CoreType::Int), Some(CoreType::String)]),
        None
    );
    assert_eq!(common_type(&[]), None);
}

#[test]
fn formal_linking_emits_lists_from_let_and_case_pattern_variables() {
    let source = r#"
module js_cast_scope.
make(): Int -> 1.
pub check(pending: List[Int]): List[Int] ->
    case pending {
        [] -> let point = make(); [point];
        [first | _rest] -> [first]
    }.
"#;
    let compiled = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        "js_cast_scope.terl",
        source,
        crate::DiagnosticFormat::default(),
        None,
        crate::validation::native_policy::NativePolicy::default(),
        crate::validation::target_profile::TargetProfile::JsShared,
    )
    .expect("compile scoped JavaScript cast fixture");
    let linked = super::super::source_linking::link_libraries(
        &compiled.core,
        std::path::Path::new("js_cast_scope.terl"),
    )
    .expect("link scoped JavaScript cast fixture");
    let js = super::super::direct_ast::emit_core_module_with_direct_oxc_ast(&linked)
        .expect("emit scoped JavaScript cast fixture");
    assert!(js.contains("function check"), "{js}");
}
