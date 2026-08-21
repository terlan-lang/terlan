//! Representation-aware operands for direct-AOT equality.

use std::collections::HashMap;

use crate::runtime::native_image::managed::{
    encode_list_empty_operation, managed_binary_semantic_id, managed_bytes_semantic_id,
    managed_string_semantic_id, SemanticTypeId,
};
use crate::terlan_typeck::CoreExpr;

use super::super::constructors::result_core_type_for_native;
use super::{
    infer_native_type_with_constructors, is_empty_list, lower_expr_with_constructors,
    NativeConstructorLayouts, NativeExpr, NativeType,
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
    if let Some(lowered) =
        super::super::constructors::lower_zero_field_managed_variant(expr, expected, constructors)?
    {
        return Ok(lowered);
    }
    if let Some(lowered) = lower_tagged_tuple_operand(
        expr,
        expected,
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )? {
        return Ok(lowered);
    }
    if matches!(
        expr,
        CoreExpr::Tuple(_) | CoreExpr::List(_) | CoreExpr::Map(_)
    ) {
        if let Some(expected_core) = result_core_type_for_native(expected, constructors) {
            return super::super::collection_values::lower_typed_value(
                expr,
                &expected_core,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            );
        }
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

/// Lowers a transparent tagged tuple through its canonical constructor layout.
fn lower_tagged_tuple_operand(
    expr: &CoreExpr,
    expected: NativeType,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<Option<NativeExpr>, String> {
    let CoreExpr::Tuple(items) = expr else {
        return Ok(None);
    };
    let Some(CoreExpr::Atom(tag)) = items.first() else {
        return Ok(None);
    };
    let arguments = &items[1..];
    let mut candidates = constructors.values().filter(|layout| {
        layout.result == expected
            && layout.parameters.len() == arguments.len()
            && layout
                .descriptor
                .variant_name()
                .is_some_and(|name| name.eq_ignore_ascii_case(tag))
    });
    let Some(layout) = candidates.next() else {
        return Ok(None);
    };
    if candidates.any(|candidate| candidate.encoded_layout != layout.encoded_layout) {
        return Err(format!(
            "error[native_ir.equality_variant]: tagged tuple `{tag}` has ambiguous managed layouts"
        ));
    }
    let fields = arguments
        .iter()
        .zip(layout.descriptor.fields())
        .map(|(argument, field)| {
            let actual = infer_native_type_with_constructors(
                argument,
                param_types,
                function_types,
                constructors,
            )
            .ok_or_else(|| {
                format!("error[native_ir.equality_variant]: cannot infer `{tag}` field type")
            })?;
            let physical = super::super::constructors::managed_field_type(actual)?;
            if physical != field.field_type() {
                return Err(format!(
                    "error[native_ir.equality_variant]: `{tag}` field has incompatible managed type"
                ));
            }
            lower_expr_with_constructors(
                argument,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(NativeExpr::Construct {
        descriptor: layout.descriptor.clone(),
        encoded_layout: layout.encoded_layout.clone(),
        fields,
    }))
}
