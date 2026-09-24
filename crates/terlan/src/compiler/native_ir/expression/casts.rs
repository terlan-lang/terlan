//! Representation-safe checked casts and compiler-owned existential boundaries.

use std::collections::HashMap;

use crate::runtime::native_image::managed::{
    encode_erased_value_box_operation, encode_erased_value_is_type_operation,
    encode_erased_value_unbox_operation, managed_erased_value_semantic_id,
};
use crate::terlan_typeck::{CoreExpr, CoreIntrinsicCall, CoreIntrinsicId, CoreType};

use super::{
    infer_native_type_for_lowering, lower_expr_with_constructors,
    lower_structural_constructor_call, native_type, NativeConstructorLayouts, NativeExpr,
    NativeType,
};

/// Queries a checked existential without treating a bad envelope as a type miss.
pub(super) fn lower_type_query(
    call: &CoreIntrinsicCall,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> super::super::NativeIrResult<NativeExpr> {
    let (CoreIntrinsicId::ErasedValueIs(expected), [value]) = (&call.id, call.args.as_slice())
    else {
        return Err("error[native_ir.erased_query]: expected one boxed argument".into());
    };
    let erased = NativeType::ManagedRef(
        managed_erased_value_semantic_id().map_err(|error| error.to_string())?,
    );
    if call.return_type != CoreType::Bool
        || super::infer_native_type_with_constructors(
            value,
            param_types,
            function_types,
            constructors,
        ) != Some(erased)
    {
        return Err("error[native_ir.erased_query]: expected a boxed value and Bool result".into());
    }
    let expected = native_type(Some(expected), &expected.contract_text()).ok_or_else(|| {
        "error[native_ir.erased_query]: expected type is not concrete".to_string()
    })?;
    let encoded = encode_erased_value_is_type_operation(&expected.boundary_type())
        .map_err(|error| error.to_string())?;
    Ok(NativeExpr::ManagedOperation {
        encoded: encoded.into(),
        args: vec![lower_expr_with_constructors(
            value,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        )?],
    })
}

/// Lowers a cast only when its checked storage representation is supported.
pub(super) fn lower_cast(
    expr: &CoreExpr,
    target_type: &CoreType,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> super::super::NativeIrResult<NativeExpr> {
    let erased = NativeType::ManagedRef(
        managed_erased_value_semantic_id().map_err(|error| error.to_string())?,
    );
    let target = native_type(Some(target_type), &target_type.contract_text());
    let source =
        super::infer_native_type_with_constructors(expr, param_types, function_types, constructors);
    if target == Some(erased) || source == Some(erased) {
        let source = source.ok_or_else(|| {
            "error[native_ir.erased_source]: boxed input must have a concrete native type"
                .to_string()
        })?;
        let target = target.ok_or_else(|| {
            "error[native_ir.erased_target]: unboxed output must have a concrete native type"
                .to_string()
        })?;
        let value = lower_expr_with_constructors(
            expr,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        )?;
        if source == target {
            return Ok(value);
        }
        let encoded = if target == erased {
            encode_erased_value_box_operation(&source.boundary_type())
        } else {
            encode_erased_value_unbox_operation(&target.boundary_type())
        }
        .map_err(|error| format!("error[native_ir.erased_type]: {error}"))?;
        return Ok(NativeExpr::ManagedOperation {
            encoded: encoded.into(),
            args: vec![value],
        });
    }
    if matches!(expr, CoreExpr::Binary(_)) && matches!(target_type, CoreType::Binary) {
        return super::super::collection_values::lower_typed_value(
            expr,
            target_type,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        )
        .map_err(Into::into);
    }
    if super::super::collection_values::is_none_option_value(expr, target_type) {
        return super::super::collection_values::lower_typed_value(
            expr,
            target_type,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        )
        .map_err(Into::into);
    }
    if matches!(
        expr,
        CoreExpr::List(_) | CoreExpr::Tuple(_) | CoreExpr::Map(_)
    ) {
        return super::super::collection_values::lower_boundary_collection_value(
            expr,
            Some(target_type),
            params,
            param_types,
            functions,
            function_types,
            constructors,
        )?
        .ok_or_else(|| {
            format!(
                "error[native_ir.cast_collection]: cast target `{}` is not a concrete native collection",
                target_type.contract_text()
            )
        }).map_err(Into::into);
    }
    if matches!(
        expr,
        CoreExpr::ConstructorCall { constructor, .. }
            if matches!(constructor.rsplit('.').next(), Some("List" | "Map"))
    ) {
        return super::super::collection_values::lower_typed_value(
            expr,
            target_type,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        )
        .map_err(Into::into);
    }
    if let Some(lowered) = super::super::collection_values::lower_boundary_collection_value(
        expr,
        Some(target_type),
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )? {
        return Ok(lowered);
    }
    if let Some(lowered) = lower_structural_constructor_call(
        expr,
        target_type,
        |field, expected_core| {
            if let Some(lowered) = super::super::collection_values::lower_boundary_collection_value(
                field,
                Some(expected_core),
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )? {
                let ty = native_type(
                    Some(expected_core),
                    &expected_core.contract_text(),
                )
                .ok_or_else(|| {
                    "error[native_ir.structural_constructor_field_type]: expected field is not native"
                        .to_string()
                })?;
                return Ok((lowered, ty));
            }
            let ty =
                infer_native_type_for_lowering(field, param_types, function_types, constructors)?
                    .ok_or_else(|| {
                    "error[native_ir.structural_constructor_field_type]: cannot infer field"
                        .to_string()
                })?;
            let lowered = lower_expr_with_constructors(
                field,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?;
            Ok((lowered, ty))
        },
    )? {
        return Ok(lowered);
    }
    let source = infer_native_type_for_lowering(expr, param_types, function_types, constructors)?
        .ok_or_else(|| {
        format!("error[native_ir.cast_source]: cannot infer cast source for {expr:?}")
    })?;
    let target = native_type(Some(target_type), &target_type.contract_text())
        .ok_or_else(|| "error[native_ir.cast_target]: unsupported cast target".to_string())?;
    if source != target {
        return Err(format!(
            "error[native_ir.cast_check]: cast changes native representation from {source:?} to {target:?} for {expr:?} -> {}",
            target_type.contract_text()
        ).into());
    }
    lower_expr_with_constructors(
        expr,
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )
    .map_err(Into::into)
}

/// Exposes contextual collection casts without discarding executable type checks.
pub(in crate::compiler::native_ir) fn collection_cast_source<'a>(
    value: &'a CoreExpr,
    expected: &CoreType,
    variables: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Option<&'a CoreExpr> {
    let CoreExpr::Cast { expr, target_type } = value else {
        return None;
    };
    let target = super::super::native_type_with_constructors(
        Some(target_type),
        &target_type.contract_text(),
        constructors,
    );
    let source =
        super::infer_native_type_with_constructors(expr, variables, functions, constructors);
    let erased = managed_erased_value_semantic_id()
        .ok()
        .map(NativeType::ManagedRef);
    if target == erased || source == erased {
        return None;
    }
    let expected_native = super::super::native_type_with_constructors(
        Some(expected),
        &expected.contract_text(),
        constructors,
    );
    (target_type == expected || expected_native == target).then_some(expr)
}
