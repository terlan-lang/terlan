//! Type predicates shared by scalar expression lowering.

use super::NativeType;

/// Reports whether an applied type uses a fixed aggregate or algebraic layout.
pub(super) fn managed_aggregate_constructor(constructor: &str) -> bool {
    matches!(
        constructor.rsplit('.').next(),
        Some("Option" | "Result" | "Array" | "FixedArray" | "Iterator" | "List" | "Map" | "Set")
    )
}

/// Reports whether native equality can compare the complete value in one word.
pub(super) fn native_word_equality(ty: NativeType) -> bool {
    matches!(
        ty,
        NativeType::Unit
            | NativeType::Int
            | NativeType::Float
            | NativeType::Bool
            | NativeType::Atom
    )
}
