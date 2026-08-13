//! Direct-AOT lowering for pure managed UTF-8 string operations.

use std::collections::HashMap;

use crate::runtime::native_image::managed::{
    encode_string_append_operation, encode_string_byte_size_operation,
    encode_string_characters_operation, encode_string_codepoints_operation,
    encode_string_compare_operation, encode_string_contains_operation,
    encode_string_ends_with_operation, encode_string_length_operation,
    encode_string_list_join_operation, encode_string_lowercase_operation,
    encode_string_replace_operation, encode_string_sha256_operation,
    encode_string_split_once_operation, encode_string_split_operation,
    encode_string_starts_with_operation, encode_string_trim_end_operation,
    encode_string_trim_operation, encode_string_trim_start_operation,
    encode_string_utf8_byte_at_operation, encode_string_utf8_find_any_byte_operation,
    encode_string_utf8_slice_operation, SemanticTypeId,
};
use crate::terlan_typeck::{
    CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic, CoreTupleTypeElem, CoreType,
};

use super::{
    lower_expr_with_constructors, native_type, NativeBinaryOperator, NativeConstructorLayouts,
    NativeExpr, NativeType,
};

pub(super) fn infer_string_intrinsic_type(call: &CoreIntrinsicCall) -> Option<NativeType> {
    match call.id {
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringContains
            | CorePrimitiveIntrinsic::StringStartsWith
            | CorePrimitiveIntrinsic::StringEndsWith
            | CorePrimitiveIntrinsic::StringIsEmpty,
        ) => Some(NativeType::Bool),
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringLength
            | CorePrimitiveIntrinsic::StringByteSize
            | CorePrimitiveIntrinsic::StringUtf8ByteAt
            | CorePrimitiveIntrinsic::StringUtf8FindAnyByte,
        ) => Some(NativeType::Int),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringCompare) => Some(NativeType::Atom),
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringSplit
            | CorePrimitiveIntrinsic::StringSplitOnce
            | CorePrimitiveIntrinsic::StringCharacters
            | CorePrimitiveIntrinsic::StringCodepoints,
        ) => native_type(Some(&call.return_type), &call.return_type.contract_text()),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringAppend) => {
            Some(NativeType::StringRef)
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringUtf8Slice) => {
            Some(NativeType::StringRef)
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringConcat) => {
            Some(NativeType::StringRef)
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringLowercase) => {
            Some(NativeType::StringRef)
        }
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringTrim
            | CorePrimitiveIntrinsic::StringTrimStart
            | CorePrimitiveIntrinsic::StringTrimEnd,
        ) => Some(NativeType::StringRef),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringReplace) => {
            Some(NativeType::StringRef)
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::CryptoSha256) => {
            Some(NativeType::StringRef)
        }
        _ => None,
    }
}

pub(super) fn lower_string_intrinsic(
    call: &CoreIntrinsicCall,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> super::super::NativeIrResult<NativeExpr> {
    let CoreIntrinsicId::Primitive(intrinsic) = &call.id else {
        return Err("error[native_ir.string_intrinsic]: expected primitive intrinsic".into());
    };
    let expected_arity = match intrinsic {
        CorePrimitiveIntrinsic::StringLowercase
        | CorePrimitiveIntrinsic::StringTrim
        | CorePrimitiveIntrinsic::StringTrimStart
        | CorePrimitiveIntrinsic::StringTrimEnd
        | CorePrimitiveIntrinsic::StringLength
        | CorePrimitiveIntrinsic::StringByteSize
        | CorePrimitiveIntrinsic::StringCharacters
        | CorePrimitiveIntrinsic::StringCodepoints
        | CorePrimitiveIntrinsic::StringConcat
        | CorePrimitiveIntrinsic::StringIsEmpty => 1,
        CorePrimitiveIntrinsic::CryptoSha256 => 1,
        CorePrimitiveIntrinsic::StringReplace
        | CorePrimitiveIntrinsic::StringUtf8FindAnyByte
        | CorePrimitiveIntrinsic::StringUtf8Slice => 3,
        _ => 2,
    };
    if call.args.len() != expected_arity {
        return Err("error[native_ir.string_intrinsic]: invalid intrinsic arity".into());
    }
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
    if *intrinsic == CorePrimitiveIntrinsic::StringIsEmpty {
        let [value] = args.as_slice() else {
            unreachable!("String.is_empty arity was validated above");
        };
        return Ok(NativeExpr::Binary {
            operator: NativeBinaryOperator::Equal,
            operand_type: NativeType::Int,
            left: Box::new(NativeExpr::ManagedOperation {
                encoded: encode_string_byte_size_operation().into(),
                args: vec![value.clone()],
            }),
            right: Box::new(NativeExpr::Int(0)),
        });
    }
    let encoded = match intrinsic {
        CorePrimitiveIntrinsic::StringContains => encode_string_contains_operation(),
        CorePrimitiveIntrinsic::StringCompare => encode_string_compare_operation(),
        CorePrimitiveIntrinsic::StringStartsWith => encode_string_starts_with_operation(),
        CorePrimitiveIntrinsic::StringEndsWith => encode_string_ends_with_operation(),
        CorePrimitiveIntrinsic::StringAppend => encode_string_append_operation(),
        CorePrimitiveIntrinsic::StringConcat => encode_string_list_join_operation(),
        CorePrimitiveIntrinsic::StringLowercase => encode_string_lowercase_operation(),
        CorePrimitiveIntrinsic::StringTrim => encode_string_trim_operation(),
        CorePrimitiveIntrinsic::StringTrimStart => encode_string_trim_start_operation(),
        CorePrimitiveIntrinsic::StringTrimEnd => encode_string_trim_end_operation(),
        CorePrimitiveIntrinsic::StringLength => encode_string_length_operation(),
        CorePrimitiveIntrinsic::StringByteSize => encode_string_byte_size_operation(),
        CorePrimitiveIntrinsic::StringUtf8ByteAt => encode_string_utf8_byte_at_operation(),
        CorePrimitiveIntrinsic::StringUtf8FindAnyByte => {
            encode_string_utf8_find_any_byte_operation()
        }
        CorePrimitiveIntrinsic::StringUtf8Slice => encode_string_utf8_slice_operation(),
        CorePrimitiveIntrinsic::StringReplace => encode_string_replace_operation(),
        CorePrimitiveIntrinsic::CryptoSha256 => encode_string_sha256_operation(),
        CorePrimitiveIntrinsic::StringSplit => {
            encode_string_split_operation(managed_semantic(&call.return_type)?)
        }
        CorePrimitiveIntrinsic::StringCharacters => {
            encode_string_characters_operation(managed_semantic(&call.return_type)?)
        }
        CorePrimitiveIntrinsic::StringCodepoints => {
            encode_string_codepoints_operation(managed_semantic(&call.return_type)?)
        }
        CorePrimitiveIntrinsic::StringSplitOnce => {
            let pair = CoreType::Tuple(vec![
                CoreTupleTypeElem::Type(CoreType::String),
                CoreTupleTypeElem::Type(CoreType::String),
            ]);
            encode_string_split_once_operation(
                managed_semantic(&call.return_type)?,
                managed_semantic(&pair)?,
            )
        }
        _ => return Err("error[native_ir.string_intrinsic]: unsupported string intrinsic".into()),
    };
    Ok(NativeExpr::ManagedOperation {
        encoded: encoded.into(),
        args,
    })
}

fn managed_semantic(ty: &CoreType) -> super::super::NativeIrResult<SemanticTypeId> {
    let Some(NativeType::ManagedRef(semantic)) = native_type(Some(ty), &ty.contract_text()) else {
        return Err(format!(
            "error[native_ir.string_intrinsic]: `{}` is not a managed result",
            ty.contract_text()
        )
        .into());
    };
    Ok(semantic)
}
