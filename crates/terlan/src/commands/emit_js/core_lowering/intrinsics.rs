//! JavaScript emission for checked CoreIR intrinsics.

use super::*;

/// Lowers a supported CoreIR intrinsic call into JavaScript expression text.
///
/// Inputs:
/// - `call`: CoreIR intrinsic call with a closed backend-neutral intrinsic id.
///
/// Output:
/// - JavaScript expression text for the supported intrinsic subset.
/// - `None` for intrinsic operations that are not yet selected for JS emission.
///
/// Transformation:
/// - Maps compiler-owned primitive intrinsic ids to JavaScript standard
///   operations while keeping JavaScript names out of CoreIR and source syntax.
pub(super) fn core_intrinsic_call_expr_to_js(call: &CoreIntrinsicCall) -> Option<String> {
    match &call.id {
        CoreIntrinsicId::Primitive(
            intrinsic @ (CorePrimitiveIntrinsic::FloatFloor | CorePrimitiveIntrinsic::FloatCeil),
        ) => {
            let [value] = call.args.as_slice() else {
                return None;
            };
            let method = if matches!(intrinsic, CorePrimitiveIntrinsic::FloatFloor) {
                "floor"
            } else {
                "ceil"
            };
            Some(format!("Math.{method}({})", core_expr_to_js(value)?))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringContains) => {
            let [value, pattern] = call.args.as_slice() else {
                return None;
            };
            Some(format!(
                "{}.includes({})",
                core_expr_to_js(value)?,
                core_expr_to_js(pattern)?
            ))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringStartsWith) => {
            let [value, prefix] = call.args.as_slice() else {
                return None;
            };
            Some(format!(
                "{}.startsWith({})",
                core_expr_to_js(value)?,
                core_expr_to_js(prefix)?
            ))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringLength) => {
            let [value] = call.args.as_slice() else {
                return None;
            };
            Some(format!("Array.from({}).length", core_expr_to_js(value)?))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringCharacters) => {
            let [value] = call.args.as_slice() else {
                return None;
            };
            Some(format!("Array.from({})", core_expr_to_js(value)?))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringCodepoints) => {
            let [value] = call.args.as_slice() else {
                return None;
            };
            Some(format!(
                "Array.from({}, (value) => value.codePointAt(0))",
                core_expr_to_js(value)?
            ))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringUtf8ByteAt) => {
            let [value, index] = call.args.as_slice() else {
                return None;
            };
            Some(format!(
                "new TextEncoder().encode({})[{}]",
                core_expr_to_js(value)?,
                core_expr_to_js(index)?
            ))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringUtf8FindAnyByte) => {
            let [value, start, candidates] = call.args.as_slice() else {
                return None;
            };
            Some(format!(
                "((value, start, candidates) => {{ const bytes = new TextEncoder().encode(value); const candidateBytes = new TextEncoder().encode(candidates); if (start < 0 || start > bytes.length || candidateBytes.some((byte) => byte >= 128)) throw new RangeError(\"invalid UTF-8 byte search\"); return bytes.findIndex((byte, index) => index >= start && candidateBytes.includes(byte)); }})({}, {}, {})",
                core_expr_to_js(value)?,
                core_expr_to_js(start)?,
                core_expr_to_js(candidates)?
            ))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::StringUtf8Slice) => {
            let [value, start, length] = call.args.as_slice() else {
                return None;
            };
            let value = core_expr_to_js(value)?;
            let start = core_expr_to_js(start)?;
            let length = core_expr_to_js(length)?;
            Some(format!(
                "new TextDecoder().decode(new TextEncoder().encode({value}).slice({start}, ({start}) + ({length})))"
            ))
        }
        _ => None,
    }
}
