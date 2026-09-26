//! Primitive calls reconstructed only after receiver types become concrete.

use crate::terlan_typeck::{
    core_intrinsic_lowering::core_pure_effect_set, core_primitive_intrinsic_return_type, CoreExpr,
    CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic, CoreType,
};

/// Builds a checked primitive call using the shared result/effect contract.
pub(super) fn intrinsic(intrinsic: CorePrimitiveIntrinsic, args: Vec<CoreExpr>) -> CoreExpr {
    CoreExpr::Intrinsic(CoreIntrinsicCall {
        return_type: core_primitive_intrinsic_return_type(&intrinsic),
        id: CoreIntrinsicId::Primitive(intrinsic),
        args,
        effects: core_pure_effect_set(),
        span: crate::terlan_syntax::span::Span::new(0, 0),
    })
}

/// Reuses UTF-8 comparison for relational operators. The concrete atom union
/// preserves every possible runtime result in the closed image's atom inventory,
/// even when the source never imports Ordering. Each operand is evaluated once.
pub(super) fn string_ordering(operator: &str, left: CoreExpr, right: CoreExpr) -> CoreExpr {
    let (operator, atom) = match operator {
        "<" => ("==", "lt"),
        "<=" => ("!=", "gt"),
        ">" => ("==", "gt"),
        ">=" => ("!=", "lt"),
        _ => unreachable!("caller selects relational operators"),
    };
    let mut comparison = intrinsic(CorePrimitiveIntrinsic::StringCompare, vec![left, right]);
    if let CoreExpr::Intrinsic(call) = &mut comparison {
        call.return_type = CoreType::Union(
            ["lt", "eq", "gt"]
                .map(|name| CoreType::AtomLiteral(name.into()))
                .into(),
        );
    }
    CoreExpr::BinaryOp {
        operator: operator.into(),
        left: Box::new(comparison),
        right: Box::new(CoreExpr::Atom(atom.into())),
    }
}
