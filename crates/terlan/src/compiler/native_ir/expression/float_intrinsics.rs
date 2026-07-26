//! Direct-AOT lowering for the closed Float intrinsic family.

use std::collections::HashMap;

use crate::runtime::native_image::managed::{
    encode_float_from_string_operation, encode_float_log_operation,
    encode_float_to_string_operation,
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
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::FloatPi) if call.args.is_empty() => {
            Ok(NativeExpr::Float(std::f64::consts::PI.to_bits()))
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::FloatTau) if call.args.is_empty() => {
            Ok(NativeExpr::Float(std::f64::consts::TAU.to_bits()))
        }
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
            | CorePrimitiveIntrinsic::FloatLog),
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
                CorePrimitiveIntrinsic::FloatLog => encode_float_log_operation(),
                _ => unreachable!(),
            };
            Ok(NativeExpr::ManagedOperation {
                encoded: encoded.into(),
                args: vec![operand],
            })
        }
        _ => Err(format!(
            "error[native_ir.intrinsic]: primitive intrinsic is not in the scalar native profile: {}",
            call.contract_text()
        )),
    }
}

pub(super) fn infer_float_intrinsic_type(call: &CoreIntrinsicCall) -> Option<NativeType> {
    match call.id {
        CoreIntrinsicId::Primitive(
            CorePrimitiveIntrinsic::FloatFloor
            | CorePrimitiveIntrinsic::FloatCeil
            | CorePrimitiveIntrinsic::FloatLog
            | CorePrimitiveIntrinsic::FloatPi
            | CorePrimitiveIntrinsic::FloatTau,
        ) => Some(NativeType::Float),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::FloatToString) => {
            Some(NativeType::StringRef)
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::FloatFromString) => {
            native_type(Some(&call.return_type), &call.return_type.contract_text())
        }
        _ => None,
    }
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
