//! Representation-aware operands for direct-AOT equality.

use std::collections::HashMap;

use crate::runtime::native_image::managed::{
    encode_list_empty_operation, managed_binary_semantic_id, managed_bytes_semantic_id,
    managed_string_semantic_id, SemanticTypeId,
};
use crate::terlan_typeck::CoreExpr;

use super::{
    is_empty_list, lower_expr_with_constructors, NativeConstructorLayouts, NativeExpr, NativeType,
};

pub(super) fn managed_equality_semantic(ty: NativeType) -> Option<SemanticTypeId> {
    match ty {
        NativeType::ManagedRef(semantic) => Some(semantic),
        NativeType::StringRef => Some(managed_string_semantic_id()),
        NativeType::BytesRef => Some(managed_bytes_semantic_id()),
        NativeType::BinaryRef => Some(managed_binary_semantic_id()),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn lower_equality_operand(
    expr: &CoreExpr,
    expected: NativeType,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<NativeExpr, String> {
    if is_empty_list(expr) {
        let NativeType::ManagedRef(semantic) = expected else {
            return Err(
                "error[native_ir.list_literal_type]: empty list requires a managed list context"
                    .to_string(),
            );
        };
        return Ok(NativeExpr::ManagedOperation {
            encoded: encode_list_empty_operation(semantic).into(),
            args: Vec::new(),
        });
    }
    lower_expr_with_constructors(
        expr,
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )
}
