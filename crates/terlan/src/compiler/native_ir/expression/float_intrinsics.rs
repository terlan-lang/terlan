//! Direct-AOT lowering for the closed Float intrinsic family.

use std::collections::HashMap;

use crate::runtime::native_image::managed::{
    encode_float_acos_operation, encode_float_acosh_operation, encode_float_asin_operation,
    encode_float_asinh_operation, encode_float_atan2_operation, encode_float_atan_operation,
    encode_float_atanh_operation, encode_float_cbrt_operation, encode_float_cos_operation,
    encode_float_cosh_operation, encode_float_exp2_operation, encode_float_exp_operation,
    encode_float_expm1_operation, encode_float_from_string_operation, encode_float_hypot_operation,
    encode_float_log10_operation, encode_float_log1p_operation, encode_float_log2_operation,
    encode_float_log_operation, encode_float_pow_operation, encode_float_round_operation,
    encode_float_sin_operation, encode_float_sinh_operation, encode_float_sqrt_operation,
    encode_float_tan_operation, encode_float_tanh_operation, encode_float_to_int_exact_operation,
    encode_float_to_string_operation, encode_float_trunc_operation,
};
use crate::terlan_typeck::{CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic};

use super::{
    lower_expr_with_constructors, native_type, NativeConstructorLayouts, NativeExpr, NativeType,
};

pub(super) fn lower_float_intrinsic(
    call: &CoreIntrinsicCall,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<NativeExpr, String> {
    match &call.id {
        CoreIntrinsicId::Primitive(
            intrinsic @ (CorePrimitiveIntrinsic::FloatFloor | CorePrimitiveIntrinsic::FloatCeil),
        ) if call.args.len() == 1 => {
            let operand = lower_operand(
                call,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?;
            Ok(if *intrinsic == CorePrimitiveIntrinsic::FloatFloor {
                NativeExpr::FloatFloor(Box::new(operand))
            } else {
                NativeExpr::FloatCeil(Box::new(operand))
            })
        }
        CoreIntrinsicId::Primitive(
            intrinsic @ (CorePrimitiveIntrinsic::FloatToString
            | CorePrimitiveIntrinsic::FloatFromString
            | CorePrimitiveIntrinsic::FloatToIntExact),
        ) if call.args.len() == 1 => {
            let operand = lower_operand(
                call,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?;
            let encoded = match intrinsic {
                CorePrimitiveIntrinsic::FloatToString => encode_float_to_string_operation(),
                CorePrimitiveIntrinsic::FloatFromString => {
                    let NativeType::ManagedRef(semantic) = native_type(
                        Some(&call.return_type),
                        &call.return_type.contract_text(),
                    )
                    .ok_or_else(|| {
                        "error[native_ir.float_from_string]: unsupported Option[Float] result"
                            .to_string()
                    })?
                    else {
                        return Err(
                            "error[native_ir.float_from_string]: result is not managed".to_string()
                        );
                    };
                    encode_float_from_string_operation(semantic)
                }
                CorePrimitiveIntrinsic::FloatToIntExact => {
                    let NativeType::ManagedRef(semantic) = native_type(
                        Some(&call.return_type),
                        &call.return_type.contract_text(),
                    )
                    .ok_or_else(|| {
                        "error[native_ir.float_to_int_exact]: unsupported Option[Int] result"
                            .to_string()
                    })?
                    else {
                        return Err(
                            "error[native_ir.float_to_int_exact]: result is not managed".to_string()
                        );
                    };
                    encode_float_to_int_exact_operation(semantic)
                }
                _ => unreachable!(),
            };
            Ok(NativeExpr::ManagedOperation {
                encoded: encoded.into(),
                args: vec![operand],
            })
        }
        CoreIntrinsicId::Primitive(intrinsic)
            if call.args.len() == 1 && float_unary_operation(intrinsic).is_some() =>
        {
            let operand = lower_operand(
                call,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?;
            Ok(NativeExpr::ManagedOperation {
                encoded: float_unary_operation(intrinsic)
                    .expect("guard admitted unary Float operation")
                    .into(),
                args: vec![operand],
            })
        }
        CoreIntrinsicId::Primitive(intrinsic)
            if call.args.len() == 2 && float_binary_operation(intrinsic).is_some() =>
        {
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
                encoded: float_binary_operation(intrinsic)
                    .expect("guard admitted binary Float operation")
                    .into(),
                args,
            })
        }
        _ => Err(format!(
            "error[native_ir.intrinsic]: primitive intrinsic is not in the scalar native profile: {}",
            call.contract_text()
        )),
    }
}

pub(super) fn infer_float_intrinsic_type(call: &CoreIntrinsicCall) -> Option<NativeType> {
    match &call.id {
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::FloatFloor | CorePrimitiveIntrinsic::FloatCeil,
        ) => Some(NativeType::Float),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::FloatToString) => {
            Some(NativeType::StringRef)
        }
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::FloatFromString | CorePrimitiveIntrinsic::FloatToIntExact,
        ) => native_type(Some(&call.return_type), &call.return_type.contract_text()),
        CoreIntrinsicId::Primitive(intrinsic)
            if float_unary_operation(intrinsic).is_some()
                || float_binary_operation(intrinsic).is_some() =>
        {
            Some(NativeType::Float)
        }
        _ => None,
    }
}

fn float_unary_operation(intrinsic: &CorePrimitiveIntrinsic) -> Option<Vec<u8>> {
    Some(match intrinsic {
        CorePrimitiveIntrinsic::FloatLog => encode_float_log_operation(),
        CorePrimitiveIntrinsic::FloatAtan => encode_float_atan_operation(),
        CorePrimitiveIntrinsic::FloatSqrt => encode_float_sqrt_operation(),
        CorePrimitiveIntrinsic::FloatRound => encode_float_round_operation(),
        CorePrimitiveIntrinsic::FloatTrunc => encode_float_trunc_operation(),
        CorePrimitiveIntrinsic::FloatExp => encode_float_exp_operation(),
        CorePrimitiveIntrinsic::FloatLog2 => encode_float_log2_operation(),
        CorePrimitiveIntrinsic::FloatLog10 => encode_float_log10_operation(),
        CorePrimitiveIntrinsic::FloatCbrt => encode_float_cbrt_operation(),
        CorePrimitiveIntrinsic::FloatSin => encode_float_sin_operation(),
        CorePrimitiveIntrinsic::FloatCos => encode_float_cos_operation(),
        CorePrimitiveIntrinsic::FloatTan => encode_float_tan_operation(),
        CorePrimitiveIntrinsic::FloatAsin => encode_float_asin_operation(),
        CorePrimitiveIntrinsic::FloatAcos => encode_float_acos_operation(),
        CorePrimitiveIntrinsic::FloatSinh => encode_float_sinh_operation(),
        CorePrimitiveIntrinsic::FloatCosh => encode_float_cosh_operation(),
        CorePrimitiveIntrinsic::FloatTanh => encode_float_tanh_operation(),
        CorePrimitiveIntrinsic::FloatAsinh => encode_float_asinh_operation(),
        CorePrimitiveIntrinsic::FloatAcosh => encode_float_acosh_operation(),
        CorePrimitiveIntrinsic::FloatAtanh => encode_float_atanh_operation(),
        CorePrimitiveIntrinsic::FloatExp2 => encode_float_exp2_operation(),
        CorePrimitiveIntrinsic::FloatExpm1 => encode_float_expm1_operation(),
        CorePrimitiveIntrinsic::FloatLog1p => encode_float_log1p_operation(),
        _ => return None,
    })
}

fn float_binary_operation(intrinsic: &CorePrimitiveIntrinsic) -> Option<Vec<u8>> {
    Some(match intrinsic {
        CorePrimitiveIntrinsic::FloatPow => encode_float_pow_operation(),
        CorePrimitiveIntrinsic::FloatAtan2 => encode_float_atan2_operation(),
        CorePrimitiveIntrinsic::FloatHypot => encode_float_hypot_operation(),
        _ => return None,
    })
}

fn lower_operand(
    call: &CoreIntrinsicCall,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<NativeExpr, String> {
    lower_expr_with_constructors(
        &call.args[0],
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )
}
