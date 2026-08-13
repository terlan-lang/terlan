//! Direct-AOT lowering for actor-owned immutable byte sequences.

use std::collections::HashMap;

use crate::runtime::native_image::managed::{
    encode_bytes_concat_operation, encode_bytes_contains_operation,
    encode_bytes_first_non_ascii_whitespace_operation, encode_bytes_from_list_operation,
    encode_bytes_length_operation, encode_bytes_read_int_be_operation,
    encode_bytes_read_int_le_operation, encode_bytes_read_uint_be_operation,
    encode_bytes_read_uint_le_operation, encode_bytes_slice_operation,
    encode_bytes_starts_with_operation, encode_bytes_to_list_operation, SemanticTypeId,
};
use crate::terlan_typeck::{CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic, CoreType};

use super::{
    lower_expr_with_constructors, native_type, NativeConstructorLayouts, NativeExpr, NativeType,
};

pub(super) fn lower_bytes_intrinsic(
    call: &CoreIntrinsicCall,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<NativeExpr, String> {
    let CoreIntrinsicId::Primitive(intrinsic) = &call.id else {
        return Err("error[native_ir.bytes_intrinsic]: expected primitive intrinsic".to_string());
    };
    let list_type = CoreType::List(Box::new(CoreType::Int));
    let list_semantic = SemanticTypeId::from_canonical(&list_type.contract_text())
        .map_err(|error| format!("error[native_ir.bytes_intrinsic]: {error}"))?;
    let mut lowered = Vec::with_capacity(call.args.len());
    for (index, argument) in call.args.iter().enumerate() {
        let value = if index == 0 && *intrinsic == CorePrimitiveIntrinsic::VmBytesFromList {
            let literal = super::super::collection_values::lower_boundary_collection_value(
                argument,
                Some(&list_type),
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?;
            if let Some(literal) = literal {
                literal
            } else {
                lower_expr_with_constructors(
                    argument,
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                )?
            }
        } else {
            lower_expr_with_constructors(
                argument,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?
        };
        lowered.push(value);
    }
    let encoded = match intrinsic {
        CorePrimitiveIntrinsic::VmBytesFromList => encode_bytes_from_list_operation(list_semantic),
        CorePrimitiveIntrinsic::VmBytesToList => encode_bytes_to_list_operation(list_semantic),
        CorePrimitiveIntrinsic::VmBytesLength => encode_bytes_length_operation(list_semantic),
        CorePrimitiveIntrinsic::VmBytesStartsWith => {
            encode_bytes_starts_with_operation(list_semantic)
        }
        CorePrimitiveIntrinsic::VmBytesContains => encode_bytes_contains_operation(list_semantic),
        CorePrimitiveIntrinsic::VmBytesFirstNonAsciiWhitespace => {
            encode_bytes_first_non_ascii_whitespace_operation(list_semantic)
        }
        CorePrimitiveIntrinsic::VmBytesConcat => encode_bytes_concat_operation(list_semantic),
        CorePrimitiveIntrinsic::VmBytesSlice => encode_bytes_slice_operation(list_semantic),
        CorePrimitiveIntrinsic::VmBytesReadUintBe => {
            encode_bytes_read_uint_be_operation(list_semantic)
        }
        CorePrimitiveIntrinsic::VmBytesReadIntBe => {
            encode_bytes_read_int_be_operation(list_semantic)
        }
        CorePrimitiveIntrinsic::VmBytesReadUintLe => {
            encode_bytes_read_uint_le_operation(list_semantic)
        }
        CorePrimitiveIntrinsic::VmBytesReadIntLe => {
            encode_bytes_read_int_le_operation(list_semantic)
        }
        _ => {
            return Err("error[native_ir.bytes_intrinsic]: unsupported bytes intrinsic".to_string())
        }
    };
    Ok(NativeExpr::ManagedOperation {
        encoded: encoded.into(),
        args: lowered,
    })
}

pub(super) fn infer_bytes_intrinsic_type(call: &CoreIntrinsicCall) -> Option<NativeType> {
    match call.id {
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::VmBytesFromList
            | CorePrimitiveIntrinsic::VmBytesToList
            | CorePrimitiveIntrinsic::VmBytesLength
            | CorePrimitiveIntrinsic::VmBytesStartsWith
            | CorePrimitiveIntrinsic::VmBytesContains
            | CorePrimitiveIntrinsic::VmBytesFirstNonAsciiWhitespace
            | CorePrimitiveIntrinsic::VmBytesConcat
            | CorePrimitiveIntrinsic::VmBytesSlice
            | CorePrimitiveIntrinsic::VmBytesReadUintBe
            | CorePrimitiveIntrinsic::VmBytesReadIntBe
            | CorePrimitiveIntrinsic::VmBytesReadUintLe
            | CorePrimitiveIntrinsic::VmBytesReadIntLe,
        ) => native_type(Some(&call.return_type), &call.return_type.contract_text()),
        _ => None,
    }
}
