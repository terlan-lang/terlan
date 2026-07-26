//! Direct-AOT lowering for the typed list inspection surface.

use std::collections::HashMap;

use crate::runtime::native_image::managed::{
    encode_list_append_operation, encode_list_empty_operation, encode_list_first_option_operation,
    encode_list_get_operation, encode_list_is_empty_operation, encode_list_length_operation,
    encode_list_rest_option_operation, SemanticTypeId,
};
use crate::terlan_typeck::{CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic};

use super::{
    infer_native_type_with_constructors, lower_expr_with_constructors, native_type,
    NativeConstructorLayouts, NativeExpr, NativeType,
};

pub(super) fn lower_list_intrinsic(
    call: &CoreIntrinsicCall,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<NativeExpr, String> {
    let CoreIntrinsicId::Primitive(intrinsic) = &call.id else {
        return Err("error[native_ir.list_intrinsic]: expected primitive intrinsic".to_string());
    };
    if *intrinsic == CorePrimitiveIntrinsic::ListNew && call.args.is_empty() {
        let semantic = managed_semantic_from_type(&call.return_type)?;
        return Ok(NativeExpr::ManagedOperation {
            encoded: encode_list_empty_operation(semantic).into(),
            args: Vec::new(),
        });
    }
    if *intrinsic == CorePrimitiveIntrinsic::ListGet && call.args.len() == 2 {
        let list_semantic = match infer_native_type_with_constructors(
            &call.args[0],
            param_types,
            function_types,
            constructors,
        ) {
            Some(NativeType::ManagedRef(semantic)) => semantic,
            _ => {
                return Err(
                    "error[native_ir.list_intrinsic]: list operand type is not managed".to_string(),
                )
            }
        };
        let lowered = call
            .args
            .iter()
            .map(|argument| {
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
        let result_is_reference = matches!(
            native_type(Some(&call.return_type), &call.return_type.contract_text()),
            Some(NativeType::ManagedRef(_))
        );
        return Ok(NativeExpr::ManagedOperation {
            encoded: encode_list_get_operation(list_semantic, result_is_reference).into(),
            args: lowered,
        });
    }
    if *intrinsic == CorePrimitiveIntrinsic::ListPush && call.args.len() == 2 {
        let list_semantic = match infer_native_type_with_constructors(
            &call.args[0],
            param_types,
            function_types,
            constructors,
        ) {
            Some(NativeType::ManagedRef(semantic)) => semantic,
            _ => {
                return Err(
                    "error[native_ir.list_intrinsic]: push receiver is not managed".to_string(),
                )
            }
        };
        let args = call
            .args
            .iter()
            .map(|argument| {
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
        return Ok(NativeExpr::ManagedOperation {
            encoded: encode_list_append_operation(list_semantic).into(),
            args,
        });
    }
    if call.args.len() != 1 {
        return Err("error[native_ir.list_intrinsic]: invalid intrinsic arity".to_string());
    }
    let list_semantic = match infer_native_type_with_constructors(
        &call.args[0],
        param_types,
        function_types,
        constructors,
    ) {
        Some(NativeType::ManagedRef(semantic)) => semantic,
        _ => {
            return Err(
                "error[native_ir.list_intrinsic]: list operand type is not managed".to_string(),
            )
        }
    };
    let operand = lower_expr_with_constructors(
        &call.args[0],
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )?;
    if *intrinsic == CorePrimitiveIntrinsic::ListIterator {
        return Ok(operand);
    }
    let encoded = match intrinsic {
        CorePrimitiveIntrinsic::ListIsEmpty => encode_list_is_empty_operation(list_semantic),
        CorePrimitiveIntrinsic::ListLength => encode_list_length_operation(list_semantic),
        CorePrimitiveIntrinsic::ListFirst => encode_list_first_option_operation(
            list_semantic,
            managed_semantic_from_type(&call.return_type)?,
        ),
        CorePrimitiveIntrinsic::ListRest => encode_list_rest_option_operation(
            list_semantic,
            managed_semantic_from_type(&call.return_type)?,
        ),
        _ => {
            return Err(
                "error[native_ir.list_intrinsic]: unsupported typed list intrinsic".to_string(),
            )
        }
    };
    Ok(NativeExpr::ManagedOperation {
        encoded: encoded.into(),
        args: vec![operand],
    })
}

pub(super) fn infer_list_intrinsic_type(call: &CoreIntrinsicCall) -> Option<NativeType> {
    match call.id {
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListIsEmpty) => Some(NativeType::Bool),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListLength) => Some(NativeType::Int),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ListGet) => {
            native_type(Some(&call.return_type), &call.return_type.contract_text())
        }
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::ListNew
            | CorePrimitiveIntrinsic::ListFirst
            | CorePrimitiveIntrinsic::ListRest
            | CorePrimitiveIntrinsic::ListIterator
            | CorePrimitiveIntrinsic::ListPush,
        ) => native_type(Some(&call.return_type), &call.return_type.contract_text()),
        _ => None,
    }
}

fn managed_semantic_from_type(
    ty: &crate::terlan_typeck::CoreType,
) -> Result<SemanticTypeId, String> {
    let NativeType::ManagedRef(semantic) = native_type(Some(ty), &ty.contract_text())
        .ok_or_else(|| "error[native_ir.list_intrinsic]: unsupported managed result".to_string())?
    else {
        return Err("error[native_ir.list_intrinsic]: result is not managed".to_string());
    };
    Ok(semantic)
}
