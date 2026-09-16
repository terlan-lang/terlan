//! Direct-AOT lowering for scalar Boolean intrinsics.

use std::collections::HashMap;
use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_managed_value_equal_operation, encode_string_literal, managed_string_semantic_id,
};
use crate::terlan_typeck::{CoreExpr, CoreIntrinsicCall, CoreIntrinsicId, CorePrimitiveIntrinsic};

use super::super::{call_composition::rebase_callee_locals, collection_values::lower_typed_value};
use super::{
    lower_expr_with_constructors, native_type, NativeBinaryOperator, NativeConstructorLayouts,
    NativeExpr, NativeType,
};

#[cfg(test)]
#[path = "boolean_intrinsics_test.rs"]
mod tests;

/// Lowers the closed Boolean domain using scalar comparisons, UTF-8 equality,
/// and the existing typed Option constructor machinery. Arguments execute once
/// in source order, including when comparison/parsing needs to inspect them twice.
pub(super) fn lower_boolean_intrinsic(
    call: &CoreIntrinsicCall,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> super::super::NativeIrResult<NativeExpr> {
    let CoreIntrinsicId::Primitive(intrinsic) = &call.id else {
        return Err("error[native_ir.bool_intrinsic]: expected Boolean intrinsic".into());
    };
    let arity = match intrinsic {
        CorePrimitiveIntrinsic::BoolEqual | CorePrimitiveIntrinsic::BoolCompare => 2,
        CorePrimitiveIntrinsic::BoolToString | CorePrimitiveIntrinsic::BoolFromString => 1,
        _ => return Err("error[native_ir.bool_intrinsic]: unsupported Boolean intrinsic".into()),
    };
    if call.args.len() != arity {
        return Err("error[native_ir.bool_intrinsic]: invalid intrinsic arity".into());
    }
    let base = params.values().copied().max().map_or(0, |index| index + 1);
    let bindings = call
        .args
        .iter()
        .enumerate()
        .map(|(index, argument)| {
            lower_expr_with_constructors(
                argument,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )
            .map(|value| rebase_callee_locals(&value, base, index))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let left = NativeExpr::Param(base);
    let text = |value: &str| {
        encode_string_literal(value)
            .map(|encoded| NativeExpr::ManagedLiteral {
                encoded: Arc::from(encoded),
            })
            .map_err(|error| format!("error[native_ir.bool_intrinsic]: {error}"))
    };
    let equality = || NativeExpr::Binary {
        operator: NativeBinaryOperator::Equal,
        operand_type: NativeType::Bool,
        left: Box::new(left.clone()),
        right: Box::new(NativeExpr::Param(base + 1)),
    };
    let body = match intrinsic {
        CorePrimitiveIntrinsic::BoolEqual => equality(),
        CorePrimitiveIntrinsic::BoolCompare => NativeExpr::If {
            clauses: vec![
                (equality(), NativeExpr::AtomLiteral("eq".into())),
                (left, NativeExpr::AtomLiteral("gt".into())),
                (NativeExpr::Bool(true), NativeExpr::AtomLiteral("lt".into())),
            ],
        },
        CorePrimitiveIntrinsic::BoolToString => NativeExpr::If {
            clauses: vec![
                (left, text("true")?),
                (NativeExpr::Bool(true), text("false")?),
            ],
        },
        CorePrimitiveIntrinsic::BoolFromString => {
            let option = |value: Option<bool>| {
                let constructor = if value.is_some() { "Some" } else { "None" };
                let value = CoreExpr::ConstructorCall {
                    constructor: constructor.to_string(),
                    constructor_identity: Some(format!("std.core.Option.{constructor}")),
                    args: value
                        .into_iter()
                        .map(|value| CoreExpr::Atom(value.to_string()))
                        .collect(),
                };
                lower_typed_value(
                    &value,
                    &call.return_type,
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                )
            };
            let matches = |literal| -> super::super::NativeIrResult<NativeExpr> {
                Ok(NativeExpr::ManagedOperation {
                    encoded: encode_managed_value_equal_operation(managed_string_semantic_id())
                        .into(),
                    args: vec![left.clone(), text(literal)?],
                })
            };
            NativeExpr::If {
                clauses: vec![
                    (matches("true")?, option(Some(true))?),
                    (matches("false")?, option(Some(false))?),
                    (NativeExpr::Bool(true), option(None)?),
                ],
            }
        }
        _ => unreachable!("Boolean intrinsic family was validated above"),
    };
    Ok(NativeExpr::Let {
        bindings,
        body: Box::new(body),
    })
}

/// Preserves the distinct Bool, Comparison, String and Option[Bool] result ABIs.
pub(super) fn infer_boolean_intrinsic_type(call: &CoreIntrinsicCall) -> Option<NativeType> {
    match call.id {
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::BoolEqual) => Some(NativeType::Bool),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::BoolCompare) => Some(NativeType::Atom),
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::BoolToString) => {
            Some(NativeType::StringRef)
        }
        CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::BoolFromString) => {
            native_type(Some(&call.return_type), &call.return_type.contract_text())
        }
        _ => None,
    }
}
