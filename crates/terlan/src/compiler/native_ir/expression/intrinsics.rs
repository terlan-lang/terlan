//! Closed dispatcher for direct-AOT primitive intrinsic families.

use std::collections::HashMap;

use crate::terlan_typeck::{CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic};

use super::{
    bitstring_intrinsics, boolean_intrinsics, bytes_intrinsics, float_intrinsics,
    integer_intrinsics, iterator_intrinsics, list_intrinsics, map_intrinsics, memory_intrinsics,
    native_type, set_intrinsics, string_intrinsics, value_intrinsics, NativeConstructorLayouts,
    NativeExpr, NativeType,
};

pub(super) fn infer_intrinsic_type(call: &CoreIntrinsicCall) -> Option<NativeType> {
    boolean_intrinsics::infer_boolean_intrinsic_type(call)
        .or_else(|| memory_intrinsics::infer_memory_intrinsic_type(call))
        .or_else(|| bitstring_intrinsics::infer_bitstring_intrinsic_type(call))
        .or_else(|| bytes_intrinsics::infer_bytes_intrinsic_type(call))
        .or_else(|| float_intrinsics::infer_float_intrinsic_type(call))
        .or_else(|| integer_intrinsics::infer_integer_intrinsic_type(call))
        .or_else(|| list_intrinsics::infer_list_intrinsic_type(call))
        .or_else(|| map_intrinsics::infer_map_intrinsic_type(call))
        .or_else(|| set_intrinsics::infer_set_intrinsic_type(call))
        .or_else(|| iterator_intrinsics::infer_iterator_intrinsic_type(call))
        .or_else(|| string_intrinsics::infer_string_intrinsic_type(call))
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
    if matches!(call.id, CoreIntrinsicId::VmProcessEntry(_)) {
        let [entry] = call.args.as_slice() else {
            return Err(
                "error[native_ir.process_entry]: Process.entry requires one image-local tag"
                    .to_string(),
            );
        };
        return super::lower_expr_with_constructors(
            entry,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        );
    }
    if let Some(lowered) = memory_intrinsics::lower_memory_intrinsic(
        call,
        params,
        param_types,
        functions,
        function_types,
        constructors,
    ) {
        return lowered;
    }
    match &call.id {
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::ValueToString) => {
            value_intrinsics::lower_value_to_string(
                call,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::BoolToString) => {
            boolean_intrinsics::lower_boolean_intrinsic(
                call,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )
            .map_err(String::from)
        }
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
            | CorePrimitiveIntrinsic::VmBytesStartsWith
            | CorePrimitiveIntrinsic::VmBytesContains
            | CorePrimitiveIntrinsic::VmBytesFirstNonAsciiWhitespace
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
            | CorePrimitiveIntrinsic::ListPush
            | CorePrimitiveIntrinsic::ListConcat
            | CorePrimitiveIntrinsic::ListSubtract
            | CorePrimitiveIntrinsic::ListClear,
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
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::SetNew
            | CorePrimitiveIntrinsic::SetFromList
            | CorePrimitiveIntrinsic::SetIsEmpty
            | CorePrimitiveIntrinsic::SetSize
            | CorePrimitiveIntrinsic::SetContains
            | CorePrimitiveIntrinsic::SetIterator
            | CorePrimitiveIntrinsic::SetAdd
            | CorePrimitiveIntrinsic::SetRemove
            | CorePrimitiveIntrinsic::SetClear,
        ) => set_intrinsics::lower_set_intrinsic(
            call,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        )
        .map_err(String::from),
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
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::StringContains
            | CorePrimitiveIntrinsic::StringCompare
            | CorePrimitiveIntrinsic::StringIsEmpty
            | CorePrimitiveIntrinsic::StringAppend
            | CorePrimitiveIntrinsic::StringConcat
            | CorePrimitiveIntrinsic::StringStartsWith
            | CorePrimitiveIntrinsic::StringEndsWith
            | CorePrimitiveIntrinsic::StringLength
            | CorePrimitiveIntrinsic::StringByteSize
            | CorePrimitiveIntrinsic::StringLowercase
            | CorePrimitiveIntrinsic::StringTrim
            | CorePrimitiveIntrinsic::StringTrimStart
            | CorePrimitiveIntrinsic::StringTrimEnd
            | CorePrimitiveIntrinsic::StringReplace
            | CorePrimitiveIntrinsic::CryptoSha256
            | CorePrimitiveIntrinsic::StringSplit
            | CorePrimitiveIntrinsic::StringSplitOnce
            | CorePrimitiveIntrinsic::StringCharacters
            | CorePrimitiveIntrinsic::StringCodepoints
            | CorePrimitiveIntrinsic::StringUtf8ByteAt
            | CorePrimitiveIntrinsic::StringUtf8FindAnyByte
            | CorePrimitiveIntrinsic::StringUtf8Slice,
        ) => string_intrinsics::lower_string_intrinsic(
            call,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        )
        .map_err(String::from),
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
