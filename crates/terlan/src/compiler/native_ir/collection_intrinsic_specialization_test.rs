use std::collections::HashMap;

use crate::terlan_typeck::{
    CoreEffectSet, CoreExpr, CoreIntrinsicId, CoreLetBinding, CorePattern, CorePrimitiveIntrinsic,
    CoreRecordExprField, CoreStructTypeField, CoreType,
};

#[test]
fn expected_option_retargets_an_inferred_variant_cast() {
    let variant = CoreType::Tuple(vec![
        crate::terlan_typeck::CoreTupleTypeElem::Type(CoreType::AtomLiteral("some".to_string())),
        crate::terlan_typeck::CoreTupleTypeElem::Type(CoreType::String),
    ]);
    let expected = CoreType::Apply {
        constructor: "std.core.Option.Option".to_string(),
        args: vec![CoreType::String],
    };
    let mut expression = CoreExpr::Cast {
        expr: Box::new(CoreExpr::ConstructorCall {
            constructor: "Some".to_string(),
            constructor_identity: Some("std.core.Option.Some".to_string()),
            args: vec![CoreExpr::Binary("value".to_string())],
        }),
        target_type: variant,
    };

    super::collection_intrinsic_specialization::annotate_expected_structural_constructors(
        &mut expression,
        &expected,
    );

    assert!(matches!(
        expression,
        CoreExpr::Cast { target_type, .. } if target_type == expected
    ));
}

#[test]
fn expected_result_retargets_an_inferred_variant_cast_inside_if() {
    let variant = CoreType::Tuple(vec![
        crate::terlan_typeck::CoreTupleTypeElem::Type(CoreType::AtomLiteral("ok".to_string())),
        crate::terlan_typeck::CoreTupleTypeElem::Type(CoreType::Int),
    ]);
    let expected = CoreType::Apply {
        constructor: "std.core.Result.Result".to_string(),
        args: vec![CoreType::Int, CoreType::String],
    };
    let mut expression = CoreExpr::If {
        clauses: vec![crate::terlan_typeck::CoreIfClause {
            condition: CoreExpr::Atom("true".to_string()),
            body: CoreExpr::Cast {
                expr: Box::new(CoreExpr::ConstructorCall {
                    constructor: "Ok".to_string(),
                    constructor_identity: Some("std.core.Result.Ok".to_string()),
                    args: vec![CoreExpr::Int(1)],
                }),
                target_type: variant,
            },
        }],
    };

    super::collection_intrinsic_specialization::annotate_expected_structural_constructors(
        &mut expression,
        &expected,
    );

    let CoreExpr::If { clauses } = expression else {
        panic!("expected if expression");
    };
    assert!(matches!(
        &clauses[0].body,
        CoreExpr::Cast { target_type, .. } if target_type == &expected
    ));
}

#[test]
fn checked_list_index_becomes_a_typed_list_get_intrinsic() {
    let mut expression = CoreExpr::Index {
        base: Box::new(CoreExpr::Var("values".to_string())),
        index: Box::new(CoreExpr::Int(2)),
    };
    let variables = HashMap::from([(
        "values".to_string(),
        CoreType::List(Box::new(CoreType::Float)),
    )]);
    let result = super::collection_intrinsic_specialization::specialize_expr(
        &mut expression,
        &variables,
        &HashMap::new(),
        "app.Test",
    );

    assert_eq!(result, Some(CoreType::Float));
    let CoreExpr::Intrinsic(call) = expression else {
        panic!("list index was not specialized");
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListGet)
    );
    assert_eq!(call.return_type, CoreType::Float);
    assert_eq!(
        super::collection_intrinsic_specialization::receiver_intrinsics::list_intrinsic_return_type(
            &CoreType::Float,
            &CorePrimitiveIntrinsic::ListLength
        ),
        CoreType::Int
    );
}

#[test]
fn map_receiver_reached_through_struct_field_becomes_typed_intrinsic() {
    let map_type = CoreType::Apply {
        constructor: "std.collections.Map.Map".to_string(),
        args: vec![CoreType::String, CoreType::Int],
    };
    let index_type = CoreType::Struct {
        name: "app.GroupIndex".to_string(),
        fields: vec![CoreStructTypeField {
            name: "groups".to_string(),
            ty: map_type.clone(),
            is_private: false,
        }],
    };
    let mut expression = CoreExpr::RemoteCall {
        module: "app.__receiver__".to_string(),
        function: "get".to_string(),
        args: vec![
            CoreExpr::FieldAccess {
                base: Box::new(CoreExpr::Var("index".to_string())),
                field: "groups".to_string(),
            },
            CoreExpr::Binary("digest".to_string()),
        ],
    };
    let variables = HashMap::from([("index".to_string(), index_type)]);

    let result = super::collection_intrinsic_specialization::specialize_expr(
        &mut expression,
        &variables,
        &HashMap::new(),
        "app",
    );

    let expected = CoreType::Apply {
        constructor: "Option".to_string(),
        args: vec![CoreType::Int],
    };
    assert_eq!(result, Some(expected.clone()));
    let CoreExpr::Intrinsic(call) = expression else {
        panic!("map field receiver was not specialized");
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapGet)
    );
    assert_eq!(call.return_type, expected);
    assert_eq!(call.args.len(), 2);
}

#[test]
fn typed_string_variable_receiver_becomes_indexed_utf8_intrinsic() {
    let mut expression = CoreExpr::RemoteCall {
        module: "app.__receiver__".to_string(),
        function: "utf8_byte_at".to_string(),
        args: vec![CoreExpr::Var("value".to_string()), CoreExpr::Int(1)],
    };
    let variables = HashMap::from([("value".to_string(), CoreType::String)]);

    let result = super::collection_intrinsic_specialization::specialize_expr(
        &mut expression,
        &variables,
        &HashMap::new(),
        "app",
    );

    assert_eq!(result, Some(CoreType::Int));
    let CoreExpr::Intrinsic(call) = expression else {
        panic!("typed String receiver must lower to an intrinsic");
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringUtf8ByteAt)
    );
    assert_eq!(
        call.args,
        vec![CoreExpr::Var("value".to_string()), CoreExpr::Int(1)]
    );
}

#[test]
fn empty_collections_in_struct_binding_inherit_consumer_field_types() {
    let map_type = CoreType::Apply {
        constructor: "std.collections.Map.Map".to_string(),
        args: vec![CoreType::String, CoreType::Int],
    };
    let list_type = CoreType::List(Box::new(CoreType::String));
    let index_type = CoreType::Struct {
        name: "app.GroupIndex".to_string(),
        fields: vec![
            CoreStructTypeField {
                name: "groups".to_string(),
                ty: map_type.clone(),
                is_private: false,
            },
            CoreStructTypeField {
                name: "hashes".to_string(),
                ty: list_type.clone(),
                is_private: false,
            },
        ],
    };
    let empty = |id, return_type| {
        CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall {
            id: CoreIntrinsicId::Primitive(id),
            args: Vec::new(),
            return_type,
            effects: CoreEffectSet {
                effects: Vec::new(),
            },
            span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
        })
    };
    let mut expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("index".to_string()),
            value: CoreExpr::RecordConstruct {
                name: "app.GroupIndex".to_string(),
                fields: vec![
                    CoreRecordExprField {
                        key: "groups".to_string(),
                        required: true,
                        value: empty(
                            CorePrimitiveIntrinsic::MapNew,
                            CoreType::Named("Map".to_string()),
                        ),
                    },
                    CoreRecordExprField {
                        key: "hashes".to_string(),
                        required: true,
                        value: empty(
                            CorePrimitiveIntrinsic::ListNew,
                            CoreType::List(Box::new(CoreType::Dynamic)),
                        ),
                    },
                ],
            },
        }],
        body: Box::new(CoreExpr::Call {
            function: "consume".to_string(),
            args: vec![CoreExpr::Var("index".to_string())],
        }),
    };
    let functions = HashMap::from([(
        ("app".to_string(), "consume".to_string(), 1),
        super::collection_intrinsic_specialization::FunctionSignature {
            params: vec![index_type.clone()],
            result: index_type.clone(),
        },
    )]);

    let result = super::collection_intrinsic_specialization::specialize_expr(
        &mut expression,
        &HashMap::new(),
        &functions,
        "app",
    );

    assert_eq!(result, Some(index_type));
    let CoreExpr::Let { bindings, .. } = expression else {
        panic!("expected lexical binding");
    };
    let CoreExpr::RecordConstruct { fields, .. } = &bindings[0].value else {
        panic!("expected struct construction");
    };
    let CoreExpr::Intrinsic(groups) = &fields[0].value else {
        panic!("expected map intrinsic");
    };
    let CoreExpr::Intrinsic(hashes) = &fields[1].value else {
        panic!("expected list intrinsic");
    };
    assert_eq!(groups.return_type, map_type);
    assert_eq!(hashes.return_type, list_type);
}

#[test]
fn mutable_list_binding_rebinds_persistent_receiver_and_preserves_unit_result() {
    let mut expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("_pushed".to_string()),
            value: CoreExpr::MutableReceiverCall {
                receiver: Box::new(CoreExpr::Var("values".to_string())),
                method: "push".to_string(),
                args: vec![CoreExpr::Int(7)],
                effects: CoreEffectSet {
                    effects: vec!["receiver_mutation".to_string()],
                },
            },
        }],
        body: Box::new(CoreExpr::Var("values".to_string())),
    };
    let list_type = CoreType::List(Box::new(CoreType::Int));
    let variables = HashMap::from([("values".to_string(), list_type.clone())]);

    let result = super::collection_intrinsic_specialization::specialize_expr(
        &mut expression,
        &variables,
        &HashMap::new(),
        "app.Test",
    );

    assert_eq!(result, Some(list_type.clone()));
    let CoreExpr::Let { bindings, .. } = expression else {
        panic!("mutable receiver call escaped its lexical binding");
    };
    assert_eq!(bindings.len(), 2);
    assert_eq!(bindings[0].pattern, CorePattern::Var("values".to_string()));
    let CoreExpr::Intrinsic(call) = &bindings[0].value else {
        panic!("mutable list call was not specialized");
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListPush)
    );
    assert_eq!(call.return_type, list_type);
    assert_eq!(
        bindings[1],
        CoreLetBinding {
            pattern: CorePattern::Var("_pushed".to_string()),
            value: CoreExpr::Atom("Unit".to_string()),
        }
    );
}

#[test]
fn empty_list_binding_inherits_a_consumer_parameter_type() {
    let mut expression = CoreExpr::Let {
        bindings: vec![
            CoreLetBinding {
                pattern: CorePattern::Var("output".to_string()),
                value: CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall {
                    id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListNew),
                    args: Vec::new(),
                    return_type: CoreType::List(Box::new(CoreType::Dynamic)),
                    effects: CoreEffectSet {
                        effects: Vec::new(),
                    },
                    span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
                }),
            },
            CoreLetBinding {
                pattern: CorePattern::Var("copied".to_string()),
                value: CoreExpr::Call {
                    function: "copy".to_string(),
                    args: vec![CoreExpr::Var("output".to_string())],
                },
            },
        ],
        body: Box::new(CoreExpr::Var("copied".to_string())),
    };
    let field_type = CoreType::Named("app.ProtocolField".to_string());
    let list_type = CoreType::List(Box::new(field_type));
    let functions = HashMap::from([(
        ("app.Test".to_string(), "copy".to_string(), 1),
        super::collection_intrinsic_specialization::FunctionSignature {
            params: vec![list_type.clone()],
            result: list_type.clone(),
        },
    )]);

    let result = super::collection_intrinsic_specialization::specialize_expr(
        &mut expression,
        &HashMap::new(),
        &functions,
        "app.Test",
    );

    assert_eq!(result, Some(list_type.clone()));
    let CoreExpr::Let { bindings, .. } = expression else {
        panic!("expected typed empty-list lexical binding");
    };
    let CoreExpr::Intrinsic(call) = &bindings[0].value else {
        panic!("empty list was not an intrinsic");
    };
    assert_eq!(call.return_type, list_type);
}

#[test]
fn set_from_list_inherits_the_concrete_list_element_type() {
    let mut expression = CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall {
        id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetFromList),
        args: vec![CoreExpr::Var("paths".to_string())],
        return_type: CoreType::Apply {
            constructor: "Set".to_string(),
            args: vec![CoreType::Dynamic],
        },
        effects: CoreEffectSet {
            effects: Vec::new(),
        },
        span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
    });
    let variables = HashMap::from([(
        "paths".to_string(),
        CoreType::List(Box::new(CoreType::String)),
    )]);
    let expected = CoreType::Apply {
        constructor: "Set".to_string(),
        args: vec![CoreType::String],
    };

    let result = super::collection_intrinsic_specialization::specialize_expr(
        &mut expression,
        &variables,
        &HashMap::new(),
        "app.Test",
    );

    assert_eq!(result, Some(expected.clone()));
    let CoreExpr::Intrinsic(call) = expression else {
        panic!("set construction was not retained as an intrinsic");
    };
    assert_eq!(call.return_type, expected);
}

#[test]
fn set_receiver_call_becomes_a_typed_intrinsic() {
    let set_type = CoreType::Apply {
        constructor: "Set".to_string(),
        args: vec![CoreType::String],
    };
    let mut expression = CoreExpr::Call {
        function: "size".to_string(),
        args: vec![CoreExpr::Var("values".to_string())],
    };
    let variables = HashMap::from([("values".to_string(), set_type)]);

    let result = super::collection_intrinsic_specialization::specialize_expr(
        &mut expression,
        &variables,
        &HashMap::new(),
        "app.Test",
    );

    assert_eq!(result, Some(CoreType::Int));
    let CoreExpr::Intrinsic(call) = expression else {
        panic!("set receiver call was not specialized");
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetSize)
    );
    assert_eq!(call.return_type, CoreType::Int);
}

#[test]
fn mutable_set_binding_rebinds_persistent_receiver_and_preserves_unit_result() {
    let mut expression = CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("_added".to_string()),
            value: CoreExpr::MutableReceiverCall {
                receiver: Box::new(CoreExpr::Var("values".to_string())),
                method: "add".to_string(),
                args: vec![CoreExpr::Int(7)],
                effects: CoreEffectSet {
                    effects: vec!["receiver_mutation".to_string()],
                },
            },
        }],
        body: Box::new(CoreExpr::Var("values".to_string())),
    };
    let set_type = CoreType::Apply {
        constructor: "Set".to_string(),
        args: vec![CoreType::Int],
    };
    let variables = HashMap::from([("values".to_string(), set_type.clone())]);

    let result = super::collection_intrinsic_specialization::specialize_expr(
        &mut expression,
        &variables,
        &HashMap::new(),
        "app.Test",
    );

    assert_eq!(result, Some(set_type.clone()));
    let CoreExpr::Let { bindings, .. } = expression else {
        panic!("mutable set call escaped its lexical binding");
    };
    assert_eq!(bindings.len(), 2);
    assert_eq!(bindings[0].pattern, CorePattern::Var("values".to_string()));
    let CoreExpr::Intrinsic(call) = &bindings[0].value else {
        panic!("mutable set call was not specialized");
    };
    assert_eq!(
        call.id,
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::SetAdd)
    );
    assert_eq!(call.return_type, set_type);
    assert_eq!(
        bindings[1],
        CoreLetBinding {
            pattern: CorePattern::Var("_added".to_string()),
            value: CoreExpr::Atom("Unit".to_string()),
        }
    );
}
