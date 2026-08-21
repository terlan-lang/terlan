use std::collections::HashMap;

use crate::runtime::native_image::managed::SemanticTypeId;
use crate::terlan_syntax::span::Span;
use crate::terlan_typeck::{
    CoreCaseClause, CoreEffectSet, CoreExpr, CoreIntrinsicCall, CoreIntrinsicId, CorePattern,
    CorePrimitiveIntrinsic, CoreStructTypeField, CoreTupleTypeElem, CoreType,
};

use super::NativeType;

#[test]
fn hidden_continuation_slots_keep_new_source_locals_disjoint() {
    let sparse = HashMap::from([("capture".to_string(), 1), ("result".to_string(), 2)]);
    assert_eq!(super::control::next_local_index(&sparse), 3);
}

#[test]
fn control_prefix_recovers_dynamic_callback_result_type_from_core_signature() {
    let callback_type = CoreType::Arrow {
        params: vec![CoreType::Int],
        return_type: Box::new(CoreType::Bool),
    };
    let expression = CoreExpr::FunctionCall {
        callee: Box::new(CoreExpr::Var("assertion".to_string())),
        args: vec![CoreExpr::Int(1)],
    };
    assert_eq!(
        super::structured_case::core_expr_type(
            &expression,
            &HashMap::from([("assertion".to_string(), callback_type)]),
            &HashMap::new(),
        ),
        Some(CoreType::Bool)
    );
}

#[test]
fn list_concat_preserves_concrete_capture_type_across_polymorphic_intrinsic() {
    let payload = CoreType::Struct {
        name: "release.Payload".to_string(),
        fields: vec![
            CoreStructTypeField {
                name: "source".to_string(),
                ty: CoreType::String,
                is_private: false,
            },
            CoreStructTypeField {
                name: "archive".to_string(),
                ty: CoreType::String,
                is_private: false,
            },
        ],
    };
    let payloads = CoreType::List(Box::new(payload));
    let semantic = SemanticTypeId::from_canonical(&payloads.contract_text())
        .expect("typed payload list semantic identity");
    let expression = CoreExpr::Intrinsic(CoreIntrinsicCall {
        id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListConcat),
        args: vec![
            CoreExpr::Var("payloads".to_string()),
            CoreExpr::Var("extra".to_string()),
        ],
        return_type: CoreType::List(Box::new(CoreType::Named("Dynamic".to_string()))),
        effects: CoreEffectSet {
            effects: Vec::new(),
        },
        span: Span { start: 0, end: 0 },
    });
    let native_variables = HashMap::from([
        ("payloads".to_string(), NativeType::ManagedRef(semantic)),
        ("extra".to_string(), NativeType::ManagedRef(semantic)),
    ]);
    let core_variables = HashMap::from([
        ("payloads".to_string(), payloads.clone()),
        ("extra".to_string(), payloads.clone()),
    ]);

    assert_eq!(
        super::infer_native_type_with_constructors(
            &expression,
            &native_variables,
            &HashMap::new(),
            &HashMap::new(),
        ),
        Some(NativeType::ManagedRef(semantic)),
    );
    assert_eq!(
        super::structured_case::core_expr_type(&expression, &core_variables, &HashMap::new()),
        Some(payloads),
    );
}

#[test]
fn nested_result_case_recovers_tagged_union_result_type() {
    let result_type = CoreType::Union(vec![
        CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::AtomLiteral("ok".to_string())),
            CoreTupleTypeElem::Type(CoreType::String),
        ]),
        CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::AtomLiteral("error".to_string())),
            CoreTupleTypeElem::Type(CoreType::String),
        ]),
    ]);
    let expression = CoreExpr::Case {
        scrutinee: Box::new(CoreExpr::Var("result".to_string())),
        clauses: vec![
            CoreCaseClause {
                pattern: CorePattern::Tuple(vec![
                    CorePattern::Atom("ok".to_string()),
                    CorePattern::Var("value".to_string()),
                ]),
                guard: None,
                body: CoreExpr::Tuple(vec![
                    CoreExpr::Atom("ok".to_string()),
                    CoreExpr::Var("value".to_string()),
                ]),
            },
            CoreCaseClause {
                pattern: CorePattern::Tuple(vec![
                    CorePattern::Atom("error".to_string()),
                    CorePattern::Var("reason".to_string()),
                ]),
                guard: None,
                body: CoreExpr::Tuple(vec![
                    CoreExpr::Atom("error".to_string()),
                    CoreExpr::Var("reason".to_string()),
                ]),
            },
        ],
    };

    assert_eq!(
        super::structured_case::core_expr_type(
            &expression,
            &HashMap::from([("result".to_string(), result_type.clone())]),
            &HashMap::new(),
        ),
        Some(result_type),
    );
}

#[test]
fn native_list_inference_recovers_case_pattern_binding_types() {
    let option_type = CoreType::Apply {
        constructor: "Option".to_string(),
        args: vec![CoreType::String],
    };
    let mut layouts = super::NativeConstructorLayouts::new();
    super::constructors::install_structural_type_layouts([&option_type], &mut layouts)
        .expect("install Option[String] layouts");
    let option_native = super::native_type(Some(&option_type), &option_type.contract_text())
        .expect("Option[String] native type");
    let list_type = CoreType::List(Box::new(CoreType::String));
    let expected = super::native_type(Some(&list_type), &list_type.contract_text())
        .expect("List[String] native type");
    let expression = CoreExpr::List(vec![
        CoreExpr::Binary("\"prefix\"".to_string()),
        CoreExpr::Case {
            scrutinee: Box::new(CoreExpr::Var("mode".to_string())),
            clauses: vec![
                CoreCaseClause {
                    pattern: CorePattern::Tuple(vec![
                        CorePattern::Atom("some".to_string()),
                        CorePattern::Var("value".to_string()),
                    ]),
                    guard: None,
                    body: CoreExpr::Var("value".to_string()),
                },
                CoreCaseClause {
                    pattern: CorePattern::Atom("none".to_string()),
                    guard: None,
                    body: CoreExpr::Binary("\"\"".to_string()),
                },
            ],
        },
        CoreExpr::Binary("\"suffix\"".to_string()),
    ]);

    assert_eq!(
        super::infer_native_type_with_constructors(
            &expression,
            &HashMap::from([("mode".to_string(), option_native)]),
            &HashMap::new(),
            &layouts,
        ),
        Some(expected),
    );
    let lowered = super::collection_values::lower_typed_value(
        &expression,
        &list_type,
        &HashMap::from([("mode".to_string(), 0)]),
        &HashMap::from([("mode".to_string(), option_native)]),
        &HashMap::new(),
        &HashMap::new(),
        &layouts,
    )
    .expect("lower List[String] containing a pattern-bound case value");
    assert!(matches!(
        lowered,
        super::NativeExpr::ManagedOperation { .. }
    ));
}
