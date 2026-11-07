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
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::FloatToIntExact) => {
            let [value] = call.args.as_slice() else {
                return None;
            };
            Some(format!(
                "((value) => Number.isFinite(value) && Number.isInteger(value) && value >= -9223372036854775808 && value < 9223372036854775808 ? [\"some\", value] : \"none\")({})",
                core_expr_to_js(value)?
            ))
        }
        CoreIntrinsicId::Primitive(intrinsic) if float_math_method(intrinsic).is_some() => {
            let method = float_math_method(intrinsic)?;
            let arguments = call
                .args
                .iter()
                .map(core_expr_to_js)
                .collect::<Option<Vec<_>>>()?;
            let expected = if matches!(
                intrinsic,
                CorePrimitiveIntrinsic::FloatPow
                    | CorePrimitiveIntrinsic::FloatAtan2
                    | CorePrimitiveIntrinsic::FloatHypot
            ) {
                2
            } else {
                1
            };
            if arguments.len() != expected {
                return None;
            }
            if matches!(intrinsic, CorePrimitiveIntrinsic::FloatRound) {
                let value = &arguments[0];
                return Some(format!(
                    "((value) => value < 0 ? -Math.round(-value) : Math.round(value))({value})"
                ));
            }
            if matches!(intrinsic, CorePrimitiveIntrinsic::FloatExp2) {
                return Some(format!("Math.pow(2, {})", arguments[0]));
            }
            Some(format!("Math.{method}({})", arguments.join(", ")))
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

fn float_math_method(intrinsic: &CorePrimitiveIntrinsic) -> Option<&'static str> {
    Some(match intrinsic {
        CorePrimitiveIntrinsic::FloatFloor => "floor",
        CorePrimitiveIntrinsic::FloatCeil => "ceil",
        CorePrimitiveIntrinsic::FloatLog => "log",
        CorePrimitiveIntrinsic::FloatAtan => "atan",
        CorePrimitiveIntrinsic::FloatSqrt => "sqrt",
        CorePrimitiveIntrinsic::FloatRound => "round",
        CorePrimitiveIntrinsic::FloatTrunc => "trunc",
        CorePrimitiveIntrinsic::FloatExp => "exp",
        CorePrimitiveIntrinsic::FloatLog2 => "log2",
        CorePrimitiveIntrinsic::FloatLog10 => "log10",
        CorePrimitiveIntrinsic::FloatCbrt => "cbrt",
        CorePrimitiveIntrinsic::FloatSin => "sin",
        CorePrimitiveIntrinsic::FloatCos => "cos",
        CorePrimitiveIntrinsic::FloatTan => "tan",
        CorePrimitiveIntrinsic::FloatAsin => "asin",
        CorePrimitiveIntrinsic::FloatAcos => "acos",
        CorePrimitiveIntrinsic::FloatSinh => "sinh",
        CorePrimitiveIntrinsic::FloatCosh => "cosh",
        CorePrimitiveIntrinsic::FloatTanh => "tanh",
        CorePrimitiveIntrinsic::FloatAsinh => "asinh",
        CorePrimitiveIntrinsic::FloatAcosh => "acosh",
        CorePrimitiveIntrinsic::FloatAtanh => "atanh",
        CorePrimitiveIntrinsic::FloatExp2 => "pow",
        CorePrimitiveIntrinsic::FloatExpm1 => "expm1",
        CorePrimitiveIntrinsic::FloatLog1p => "log1p",
        CorePrimitiveIntrinsic::FloatPow => "pow",
        CorePrimitiveIntrinsic::FloatAtan2 => "atan2",
        CorePrimitiveIntrinsic::FloatHypot => "hypot",
        _ => return None,
    })
}
