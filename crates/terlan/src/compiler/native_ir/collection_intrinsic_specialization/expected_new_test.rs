//! Expected struct fields must reach collection intrinsics before suspension splitting.

use super::*;
use crate::terlan_typeck::CoreStructTypeField;

#[test]
fn explicit_tuple_context_types_empty_payloads_idempotently() {
    use crate::terlan_typeck::CoreTupleTypeElem;
    let list = CoreType::List(Box::new(CoreType::String));
    let mut expression = CoreExpr::Cast {
        expr: Box::new(CoreExpr::Tuple(vec![
            CoreExpr::Atom("wrapped".into()),
            CoreExpr::List(vec![]),
        ])),
        target_type: CoreType::Tuple(vec![
            CoreTupleTypeElem::Type(CoreType::AtomLiteral("wrapped".into())),
            CoreTupleTypeElem::Field {
                name: "values".into(),
                ty: list.clone(),
            },
        ]),
    };
    super::super::specialize_expr(&mut expression, &HashMap::new(), &HashMap::new(), "fixture");
    let CoreExpr::Cast { expr, .. } = &expression else {
        panic!("retain tuple type")
    };
    let CoreExpr::Tuple(items) = expr.as_ref() else {
        panic!("retain tuple value")
    };
    assert!(matches!(&items[1], CoreExpr::Cast { target_type, .. } if target_type == &list));
    let once = expression.clone();
    for _ in 0..4 {
        super::super::specialize_expr(&mut expression, &HashMap::new(), &HashMap::new(), "fixture");
        assert_eq!(expression, once);
    }
}

#[test]
fn checked_text_literal_context_replaces_inferred_cast_without_retyping_values() {
    let mut literal = CoreExpr::Cast {
        expr: Box::new(CoreExpr::Binary("\"a\"".to_string())),
        target_type: CoreType::String,
    };
    specialize_expected_collection_new(&mut literal, &CoreType::Binary, &HashMap::new(), "fixture");
    assert_eq!(
        literal,
        CoreExpr::Cast {
            expr: Box::new(CoreExpr::Binary("\"a\"".to_string())),
            target_type: CoreType::Binary,
        }
    );
    let mut value = CoreExpr::Cast {
        expr: Box::new(CoreExpr::Var("text".to_string())),
        target_type: CoreType::String,
    };
    let original = value.clone();
    specialize_expected_collection_new(&mut value, &CoreType::Binary, &HashMap::new(), "fixture");
    assert_eq!(
        value, original,
        "a checked existing value cannot be reinterpreted"
    );
}

#[test]
fn repeated_collection_context_is_idempotent() {
    let list_type = CoreType::List(Box::new(CoreType::Int));
    let mut expression = CoreExpr::List(vec![CoreExpr::Int(7)]);
    let functions = HashMap::new();
    specialize_expected_collection_new(&mut expression, &list_type, &functions, "fixture");
    let once = expression.clone();
    for _ in 0..4 {
        specialize_expected_collection_new(&mut expression, &list_type, &functions, "fixture");
        super::super::specialize_expr(&mut expression, &HashMap::new(), &functions, "fixture");
        assert_eq!(expression, once);
    }
}

/// A custom push method cannot overwrite a nominal constructor's checked type.
#[test]
fn push_inference_only_contextualizes_empty_list_initializers() {
    let custom = CoreExpr::Cast {
        expr: Box::new(CoreExpr::ConstructorCall {
            type_args: Vec::new(),
            constructor: "Buffer".to_string(),
            constructor_identity: Some("fixture.Buffer".to_string()),
            args: vec![],
        }),
        target_type: CoreType::Named("fixture.Buffer".to_string()),
    };
    let mut bindings = vec![CoreLetBinding {
        pattern: CorePattern::Var("state".to_string()),
        value: custom.clone(),
    }];
    let body = CoreExpr::MutableReceiverCall {
        receiver: Box::new(CoreExpr::Var("state".to_string())),
        method: "push".to_string(),
        args: vec![CoreExpr::Int(1)],
        effects: CoreEffectSet {
            effects: vec!["receiver_mutation".to_string()],
        },
    };
    specialize_collection_new_bindings(
        &mut bindings,
        &body,
        &HashMap::new(),
        &HashMap::new(),
        "fixture",
    );
    assert_eq!(bindings[0].value, custom);
    bindings[0].value = CoreExpr::List(vec![]);
    specialize_collection_new_bindings(
        &mut bindings,
        &body,
        &HashMap::new(),
        &HashMap::new(),
        "fixture",
    );
    assert!(
        matches!(&bindings[0].value, CoreExpr::Cast { target_type: CoreType::List(element), .. } if **element == CoreType::Int)
    );
}

/// Qualified and local nominal constructors contextualize their collection fields.
#[test]
fn struct_constructor_specializes_collection_arguments_without_widening_foreign_names() {
    let concrete = CoreType::Apply {
        constructor: "Map".to_owned(),
        args: vec![CoreType::String, CoreType::Int],
    };
    let generic = CoreType::Named("Map".to_owned());
    let expected = CoreType::Struct {
        name: "fixture.State".to_owned(),
        fields: vec![CoreStructTypeField {
            name: "values".to_owned(),
            ty: concrete.clone(),
            is_private: false,
        }],
    };
    for (constructor, identity, accepted) in [
        ("State", None, true),
        ("fixture.State", None, true),
        ("State", Some("fixture.State"), true),
        ("State", Some("State"), true),
        ("other.State", None, false),
        ("State", Some("other.State"), false),
    ] {
        let mut expression = CoreExpr::ConstructorCall {
            type_args: Vec::new(),
            constructor: constructor.to_owned(),
            constructor_identity: identity.map(str::to_owned),
            args: vec![CoreExpr::Intrinsic(CoreIntrinsicCall {
                id: CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::MapNew),
                args: Vec::new(),
                return_type: generic.clone(),
                effects: CoreEffectSet {
                    effects: Vec::new(),
                },
                span: crate::terlan_syntax::span::Span { start: 0, end: 0 },
            })],
        };
        specialize_expected_collection_new(&mut expression, &expected, &HashMap::new(), "fixture");
        let CoreExpr::ConstructorCall { args, .. } = expression else {
            panic!("expected unchanged constructor shape");
        };
        let CoreExpr::Intrinsic(call) = &args[0] else {
            panic!("expected map intrinsic");
        };
        assert_eq!(
            call.return_type,
            if accepted {
                concrete.clone()
            } else {
                generic.clone()
            }
        );
    }
}
