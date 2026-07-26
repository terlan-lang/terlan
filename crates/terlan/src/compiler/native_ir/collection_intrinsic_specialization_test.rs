use std::collections::HashMap;

use crate::terlan_typeck::{
    CoreEffectSet, CoreExpr, CoreIntrinsicId, CoreLetBinding, CorePattern, CorePrimitiveIntrinsic,
    CoreType,
};

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
        super::collection_intrinsic_specialization::list_intrinsic_return_type(
            &CoreType::Float,
            &CorePrimitiveIntrinsic::ListLength
        ),
        CoreType::Int
    );
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
