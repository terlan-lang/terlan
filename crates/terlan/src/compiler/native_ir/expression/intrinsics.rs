//! Closed dispatcher for direct-AOT primitive intrinsic families.

use std::collections::HashMap;

use crate::terlan_typeck::{CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic};

use super::{
    bitstring_intrinsics, bytes_intrinsics, float_intrinsics, integer_intrinsics,
    iterator_intrinsics, list_intrinsics, map_intrinsics, native_type, NativeConstructorLayouts,
    NativeExpr, NativeType,
};

pub(super) fn infer_intrinsic_type(call: &CoreIntrinsicCall) -> Option<NativeType> {
    bitstring_intrinsics::infer_bitstring_intrinsic_type(call)
        .or_else(|| bytes_intrinsics::infer_bytes_intrinsic_type(call))
        .or_else(|| float_intrinsics::infer_float_intrinsic_type(call))
        .or_else(|| integer_intrinsics::infer_integer_intrinsic_type(call))
        .or_else(|| list_intrinsics::infer_list_intrinsic_type(call))
        .or_else(|| map_intrinsics::infer_map_intrinsic_type(call))
        .or_else(|| iterator_intrinsics::infer_iterator_intrinsic_type(call))
        .or_else(|| native_type(Some(&call.return_type), &call.return_type.contract_text()))
}

pub(super) fn lower_intrinsic(
    call: &CoreIntrinsicCall,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<NativeExpr, String> {
    match &call.id {
        CoreIntrinsicId::Primitive(intrinsic)
            if bitstring_intrinsics::is_bitstring_intrinsic(intrinsic) =>
        {
            bitstring_intrinsics::lower_bitstring_intrinsic(
                call,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )
        }
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::VmBytesFromList
            | CorePrimitiveIntrinsic::VmBytesToList
            | CorePrimitiveIntrinsic::VmBytesLength
            | CorePrimitiveIntrinsic::VmBytesConcat
            | CorePrimitiveIntrinsic::VmBytesSlice
            | CorePrimitiveIntrinsic::VmBytesReadUintBe
            | CorePrimitiveIntrinsic::VmBytesReadIntBe
            | CorePrimitiveIntrinsic::VmBytesReadUintLe
            | CorePrimitiveIntrinsic::VmBytesReadIntLe,
        ) => bytes_intrinsics::lower_bytes_intrinsic(
            call,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        ),
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::IntToString
            | CorePrimitiveIntrinsic::IntToStringBase
            | CorePrimitiveIntrinsic::IntFromString
            | CorePrimitiveIntrinsic::IntFromStringBase,
        ) => integer_intrinsics::lower_integer_intrinsic(
            call,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        ),
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::ListNew
            | CorePrimitiveIntrinsic::ListIsEmpty
            | CorePrimitiveIntrinsic::ListLength
            | CorePrimitiveIntrinsic::ListGet
            | CorePrimitiveIntrinsic::ListFirst
            | CorePrimitiveIntrinsic::ListRest
            | CorePrimitiveIntrinsic::ListIterator
            | CorePrimitiveIntrinsic::ListPush,
        ) => list_intrinsics::lower_list_intrinsic(
            call,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        ),
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::MapNew
            | CorePrimitiveIntrinsic::MapFromEntries
            | CorePrimitiveIntrinsic::MapIsEmpty
            | CorePrimitiveIntrinsic::MapSize
            | CorePrimitiveIntrinsic::MapGet
            | CorePrimitiveIntrinsic::MapTake
            | CorePrimitiveIntrinsic::MapContainsKey
            | CorePrimitiveIntrinsic::MapIterator
            | CorePrimitiveIntrinsic::MapPut
            | CorePrimitiveIntrinsic::MapRemove
            | CorePrimitiveIntrinsic::MapClear,
        ) => map_intrinsics::lower_map_intrinsic(
            call,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        ),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::IteratorNext) => {
            iterator_intrinsics::lower_iterator_intrinsic(
                call,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )
        }
        _ => float_intrinsics::lower_float_intrinsic(
            call,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        ),
    }
}
