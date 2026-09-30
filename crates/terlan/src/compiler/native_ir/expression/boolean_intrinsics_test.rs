use super::*;
use crate::terlan_syntax::span::Span;
use crate::terlan_typeck::{CoreEffectSet, CoreLetBinding, CorePattern, CoreType};

/// Creates an intrinsic boundary without evaluating its operand expressions.
fn call(intrinsic: CorePrimitiveIntrinsic, args: Vec<CoreExpr>) -> CoreIntrinsicCall {
    CoreIntrinsicCall {
        id: CoreIntrinsicId::Primitive(intrinsic),
        args,
        return_type: CoreType::Bool,
        effects: CoreEffectSet {
            effects: vec!["pure".to_string()],
        },
        span: Span::new(0, 1),
    }
}

/// A language conversion evaluates its operand once and keeps nested locals
/// disjoint from the result binding introduced by lowering.
#[test]
fn boolean_intrinsics_bind_operands_once_and_rebase_nested_locals() {
    let operand = |function: &str| CoreExpr::Let {
        bindings: vec![CoreLetBinding {
            pattern: CorePattern::Var("temporary".to_string()),
            value: CoreExpr::Call {
                type_args: Vec::new(),
                function: function.to_string(),
                args: vec![],
            },
        }],
        body: Box::new(CoreExpr::Var("temporary".to_string())),
    };
    let lowered = lower_boolean_intrinsic(
        &call(CorePrimitiveIntrinsic::BoolToString, vec![operand("left")]),
        &HashMap::from([("outer".to_string(), 0)]),
        &HashMap::from([("outer".to_string(), NativeType::Bool)]),
        &HashMap::from([(("left".to_string(), 0), 10)]),
        &HashMap::from([(("left".to_string(), 0), NativeType::Bool)]),
        &HashMap::new(),
    )
    .expect("lower conversion operand");
    let NativeExpr::Let { bindings, body } = lowered else {
        panic!("bound conversion operand")
    };
    assert_eq!(
        bindings,
        vec![NativeExpr::Let {
            bindings: vec![NativeExpr::Call {
                function: 10,
                args: vec![]
            }],
            body: Box::new(NativeExpr::Param(1))
        },]
    );
    let mut calls = 0;
    crate::compiler::native_ir::call_composition::walk_native_expr(&body, &mut |expr| {
        if matches!(expr, NativeExpr::Call { .. }) {
            calls += 1;
        }
    });
    assert_eq!(
        calls, 0,
        "conversion branches must only read the bound value"
    );
}

/// Invalid Boolean arities are rejected before touching malformed operands.
#[test]
fn boolean_intrinsics_reject_invalid_arity() {
    for (intrinsic, arity) in [
        (CorePrimitiveIntrinsic::BoolToString, 1),
        (CorePrimitiveIntrinsic::BoolFromString, 1),
    ] {
        for actual in [arity - 1, arity + 1] {
            let error = lower_boolean_intrinsic(
                &call(
                    intrinsic.clone(),
                    vec![CoreExpr::Var("unbound".to_string()); actual],
                ),
                &HashMap::new(),
                &HashMap::new(),
                &HashMap::new(),
                &HashMap::new(),
                &HashMap::new(),
            )
            .expect_err("invalid Boolean arity");
            assert_eq!(
                error.to_string(),
                "error[native_ir.bool_intrinsic]: invalid intrinsic arity"
            );
        }
    }
}
