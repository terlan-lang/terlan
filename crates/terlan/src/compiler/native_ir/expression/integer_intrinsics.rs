//! Direct-AOT lowering for integer text conversion intrinsics.

use std::collections::HashMap;

use crate::runtime::native_image::managed::{
    encode_int_from_string_base_operation, encode_int_from_string_operation,
    encode_int_to_string_base_operation, encode_int_to_string_operation,
};
use crate::terlan_typeck::{CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic};

use super::{
    lower_expr_with_constructors, native_type, NativeConstructorLayouts, NativeExpr, NativeType,
};

pub(super) fn lower_integer_intrinsic(
    call: &CoreIntrinsicCall,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<NativeExpr, String> {
    let CoreIntrinsicId::Primitive(intrinsic) = &call.id else {
        return Err("error[native_ir.int_intrinsic]: expected primitive intrinsic".to_string());
    };
    let supported = matches!(
        intrinsic,
        CorePrimitiveIntrinsic::IntToString
            | CorePrimitiveIntrinsic::IntToStringBase
            | CorePrimitiveIntrinsic::IntFromString
            | CorePrimitiveIntrinsic::IntFromStringBase
    );
    if !supported {
        return Err("error[native_ir.int_intrinsic]: unsupported integer intrinsic".to_string());
    }
    let expected_arity = match intrinsic {
        CorePrimitiveIntrinsic::IntToString | CorePrimitiveIntrinsic::IntFromString => 1,
        _ => 2,
    };
    if call.args.len() != expected_arity {
        return Err("error[native_ir.int_intrinsic]: invalid intrinsic arity".to_string());
    }
    let args = call
        .args
        .iter()
        .map(|arg| {
            lower_expr_with_constructors(
                arg,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let encoded = match intrinsic {
        CorePrimitiveIntrinsic::IntToString => encode_int_to_string_operation(),
        CorePrimitiveIntrinsic::IntToStringBase => {
            encode_int_to_string_base_operation(option_semantic(call)?)
        }
        CorePrimitiveIntrinsic::IntFromString => {
            encode_int_from_string_operation(option_semantic(call)?)
        }
        CorePrimitiveIntrinsic::IntFromStringBase => {
            encode_int_from_string_base_operation(option_semantic(call)?)
        }
        _ => unreachable!(),
    };
    Ok(NativeExpr::ManagedOperation {
        encoded: encoded.into(),
        args,
    })
}

pub(super) fn infer_integer_intrinsic_type(call: &CoreIntrinsicCall) -> Option<NativeType> {
    match call.id {
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::IntToString) => {
            Some(NativeType::StringRef)
        }
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::IntToStringBase
            | CorePrimitiveIntrinsic::IntFromString
            | CorePrimitiveIntrinsic::IntFromStringBase,
        ) => native_type(Some(&call.return_type), &call.return_type.contract_text()),
        _ => None,
    }
}

fn option_semantic(
    call: &CoreIntrinsicCall,
) -> Result<crate::runtime::native_image::managed::SemanticTypeId, String> {
    let NativeType::ManagedRef(semantic) =
        native_type(Some(&call.return_type), &call.return_type.contract_text()).ok_or_else(
            || "error[native_ir.int_intrinsic]: unsupported Option result".to_string(),
        )?
    else {
        return Err("error[native_ir.int_intrinsic]: result is not managed".to_string());
    };
    Ok(semantic)
}
