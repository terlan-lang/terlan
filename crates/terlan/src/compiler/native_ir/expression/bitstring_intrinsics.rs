//! Direct-AOT lowering for actor-owned BitString primitives.

use std::collections::HashMap;

use crate::runtime::native_image::managed::{
    encode_bitstring_operation, ManagedBitStringOperation,
};
use crate::terlan_typeck::{CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic};

use super::{
    lower_expr_with_constructors, native_type, NativeConstructorLayouts, NativeExpr, NativeType,
};

pub(super) fn lower_bitstring_intrinsic(
    call: &CoreIntrinsicCall,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<NativeExpr, String> {
    let CoreIntrinsicId::Primitive(intrinsic) = &call.id else {
        return Err("error[native_ir.bitstring_intrinsic]: expected primitive intrinsic".into());
    };
    let operation = bitstring_operation(intrinsic).ok_or_else(|| {
        "error[native_ir.bitstring_intrinsic]: unsupported bitstring intrinsic".to_string()
    })?;
    if call.args.len() != bitstring_arity(operation) {
        return Err("error[native_ir.bitstring_intrinsic]: invalid intrinsic arity".into());
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
    Ok(NativeExpr::ManagedOperation {
        encoded: encode_bitstring_operation(operation).into(),
        args,
    })
}

pub(super) fn infer_bitstring_intrinsic_type(call: &CoreIntrinsicCall) -> Option<NativeType> {
    let CoreIntrinsicId::Primitive(intrinsic) = &call.id else {
        return None;
    };
    bitstring_operation(intrinsic)?;
    native_type(Some(&call.return_type), &call.return_type.contract_text())
}

pub(super) fn is_bitstring_intrinsic(intrinsic: &CorePrimitiveIntrinsic) -> bool {
    bitstring_operation(intrinsic).is_some()
}

fn bitstring_operation(intrinsic: &CorePrimitiveIntrinsic) -> Option<ManagedBitStringOperation> {
    use CorePrimitiveIntrinsic as P;
    use ManagedBitStringOperation as O;

    Some(match intrinsic {
        P::VmBitStringFromBytes => O::FromBytes,
        P::VmBitStringFromAllBytes => O::FromAllBytes,
        P::VmBitStringFromExactBytes => O::FromExactBytes,
        P::VmBitStringRequireExactBits => O::RequireExactBits,
        P::VmBitStringFromUintBe => O::FromUintBe,
        P::VmBitStringFromIntBe => O::FromIntBe,
        P::VmBitStringFromUintLe => O::FromUintLe,
        P::VmBitStringFromIntLe => O::FromIntLe,
        P::VmBitStringUtf8Scalar => O::Utf8Scalar,
        P::VmBitStringToUtf8Scalar => O::ToUtf8Scalar,
        P::VmBitStringUtf16BeScalar => O::Utf16BeScalar,
        P::VmBitStringUtf16LeScalar => O::Utf16LeScalar,
        P::VmBitStringToUtf16BeScalar => O::ToUtf16BeScalar,
        P::VmBitStringToUtf16LeScalar => O::ToUtf16LeScalar,
        P::VmBitStringUtf32BeScalar => O::Utf32BeScalar,
        P::VmBitStringUtf32LeScalar => O::Utf32LeScalar,
        P::VmBitStringToUtf32BeScalar => O::ToUtf32BeScalar,
        P::VmBitStringToUtf32LeScalar => O::ToUtf32LeScalar,
        P::VmBitStringBitLength => O::BitLength,
        P::VmBitStringByteLength => O::ByteLength,
        P::VmBitStringIsByteAligned => O::IsByteAligned,
        P::VmBitStringSlice => O::Slice,
        P::VmBitStringConcat => O::Concat,
        P::VmBitStringToBytes => O::ToBytes,
        P::VmBitStringToUintBe => O::ToUintBe,
        P::VmBitStringToIntBe => O::ToIntBe,
        P::VmBitStringToUintLe => O::ToUintLe,
        P::VmBitStringToIntLe => O::ToIntLe,
        _ => return None,
    })
}

fn bitstring_arity(operation: ManagedBitStringOperation) -> usize {
    use ManagedBitStringOperation as O;

    match operation {
        O::FromBytes
        | O::FromExactBytes
        | O::RequireExactBits
        | O::FromUintBe
        | O::FromIntBe
        | O::FromUintLe
        | O::FromIntLe
        | O::Concat => 2,
        O::Slice => 3,
        O::FromAllBytes
        | O::Utf8Scalar
        | O::ToUtf8Scalar
        | O::Utf16BeScalar
        | O::Utf16LeScalar
        | O::ToUtf16BeScalar
        | O::ToUtf16LeScalar
        | O::Utf32BeScalar
        | O::Utf32LeScalar
        | O::ToUtf32BeScalar
        | O::ToUtf32LeScalar
        | O::BitLength
        | O::ByteLength
        | O::IsByteAligned
        | O::ToBytes
        | O::ToUintBe
        | O::ToIntBe
        | O::ToUintLe
        | O::ToIntLe => 1,
    }
}
