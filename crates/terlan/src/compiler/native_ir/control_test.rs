use std::collections::HashMap;

use crate::runtime::native_image::managed::SemanticTypeId;
use crate::terlan_syntax::span::Span;
use crate::terlan_typeck::{
    CoreEffectSet, CoreExpr, CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic,
    CoreStructTypeField, CoreType,
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
