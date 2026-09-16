//! Mutation witnesses must use the correct lexical scope and collection owner.

use super::*;

fn initializer(kind: CorePrimitiveIntrinsic) -> CoreLetBinding {
    CoreLetBinding {
        pattern: CorePattern::Var("state".to_string()),
        value: CoreExpr::Intrinsic(CoreIntrinsicCall {
            id: CoreIntrinsicId::Primitive(kind),
            args: vec![],
            return_type: CoreType::Dynamic,
            effects: CoreEffectSet { effects: vec![] },
            span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
        }),
    }
}

fn mutation(method: &str, args: Vec<CoreExpr>) -> CoreExpr {
    CoreExpr::MutableReceiverCall {
        receiver: Box::new(CoreExpr::Var("state".to_string())),
        method: method.to_string(),
        args,
        effects: CoreEffectSet {
            effects: vec!["receiver_mutation".to_string()],
        },
    }
}

#[test]
fn mutation_types_use_callback_parameters_for_all_collection_kinds() {
    let variables = HashMap::from([
        ("key".to_string(), CoreType::String),
        ("value".to_string(), CoreType::Int),
    ]);
    for (kind, method, args, expected) in [
        (
            CorePrimitiveIntrinsic::ListNew,
            "push",
            vec![CoreExpr::Var("value".to_string())],
            CoreType::List(Box::new(CoreType::Int)),
        ),
        (
            CorePrimitiveIntrinsic::MapNew,
            "put",
            vec![
                CoreExpr::Var("key".to_string()),
                CoreExpr::Var("value".to_string()),
            ],
            CoreType::Apply {
                constructor: "Map".to_string(),
                args: vec![CoreType::String, CoreType::Int],
            },
        ),
        (
            CorePrimitiveIntrinsic::SetNew,
            "add",
            vec![CoreExpr::Var("value".to_string())],
            CoreType::Apply {
                constructor: "Set".to_string(),
                args: vec![CoreType::Int],
            },
        ),
    ] {
        assert_eq!(
            infer_binding_use(
                &initializer(kind),
                &[],
                &mutation(method, args),
                &variables,
                &HashMap::new(),
                "fixture"
            ),
            Some(expected)
        );
    }
}

#[test]
fn mutation_witnesses_follow_operand_shadowing_but_stop_at_receiver_shadowing() {
    let binding = initializer(CorePrimitiveIntrinsic::MapNew);
    let later = vec![CoreLetBinding {
        pattern: CorePattern::Var("key".to_string()),
        value: CoreExpr::Int(3),
    }];
    let body = mutation(
        "put",
        vec![CoreExpr::Var("key".to_string()), CoreExpr::Int(7)],
    );
    let variables = HashMap::from([("key".to_string(), CoreType::String)]);
    let expected = CoreType::Apply {
        constructor: "Map".to_string(),
        args: vec![CoreType::Int, CoreType::Int],
    };
    assert_eq!(
        infer_binding_use(
            &binding,
            &later,
            &body,
            &variables,
            &HashMap::new(),
            "fixture"
        ),
        Some(expected.clone())
    );
    let nested = CoreExpr::Let {
        bindings: later,
        body: Box::new(body.clone()),
    };
    assert_eq!(
        infer_binding_use(
            &binding,
            &[],
            &nested,
            &variables,
            &HashMap::new(),
            "fixture"
        ),
        Some(expected)
    );
    let shadow = vec![CoreLetBinding {
        pattern: CorePattern::Var("state".to_string()),
        value: CoreExpr::Int(0),
    }];
    assert_eq!(
        infer_binding_use(
            &binding,
            &shadow,
            &body,
            &variables,
            &HashMap::new(),
            "fixture"
        ),
        None
    );
}

#[test]
fn similarly_named_custom_mutators_cannot_retype_an_initializer() {
    let binding = CoreLetBinding {
        pattern: CorePattern::Var("state".to_string()),
        value: CoreExpr::Cast {
            expr: Box::new(CoreExpr::ConstructorCall {
                constructor: "Store".to_string(),
                constructor_identity: Some("fixture.Store".to_string()),
                args: vec![],
            }),
            target_type: CoreType::Named("fixture.Store".to_string()),
        },
    };
    for (method, args) in [
        ("put", vec![CoreExpr::Int(1), CoreExpr::Int(2)]),
        ("add", vec![CoreExpr::Int(1)]),
        ("push", vec![CoreExpr::Int(1)]),
    ] {
        assert_eq!(
            infer_binding_use(
                &binding,
                &[],
                &mutation(method, args),
                &HashMap::new(),
                &HashMap::new(),
                "fixture"
            ),
            None
        );
    }
}
