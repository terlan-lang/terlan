//! Expected struct fields must reach collection intrinsics before suspension splitting.

use super::*;
use crate::terlan_typeck::CoreStructTypeField;

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
