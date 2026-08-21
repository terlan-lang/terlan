use std::collections::HashMap;
use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_list_prepend_operation, encode_managed_value_equal_operation,
    encode_string_append_operation, encode_string_literal,
};
use crate::terlan_typeck::{CoreExpr, CorePattern, CoreType};

use super::{
    constructors::{
        constructor_result_core_type, constructor_result_type, lower_constructor_call,
        lower_record_construct, lower_record_update, lower_structural_constructor_call,
        lower_structural_record_construct, managed_field_projection, record_construct_result_type,
        record_update_result_type, NativeConstructorLayouts,
    },
    escape::retained_managed_bindings,
    NativeBinaryOperator, NativeExpr, NativeType,
};

#[path = "expression/bitstring_intrinsics.rs"]
mod bitstring_intrinsics;
#[path = "expression/boolean_intrinsics.rs"]
mod boolean_intrinsics;
#[path = "expression/bytes_intrinsics.rs"]
mod bytes_intrinsics;
#[path = "expression/collection_literal_types.rs"]
mod collection_literal_types;
#[path = "expression/equality.rs"]
mod equality;
#[path = "expression/field_access.rs"]
mod field_access;
#[path = "expression/float_intrinsics.rs"]
mod float_intrinsics;
#[path = "expression/free_variables.rs"]
mod free_variable_analysis;
#[cfg(test)]
#[path = "expression/free_variables_test.rs"]
#[cfg(test)]
mod free_variable_analysis_test;
#[path = "expression/integer_intrinsics.rs"]
mod integer_intrinsics;
#[path = "expression/intrinsics.rs"]
mod intrinsics;
#[path = "expression/iterator_intrinsics.rs"]
mod iterator_intrinsics;
#[path = "expression/list_intrinsics.rs"]
mod list_intrinsics;
#[path = "expression/map_intrinsics.rs"]
mod map_intrinsics;
#[path = "expression/memory_intrinsics.rs"]
mod memory_intrinsics;
#[cfg(test)]
#[path = "expression/memory_intrinsics_test.rs"]
#[cfg(test)]
mod memory_intrinsics_test;
#[path = "expression/scalar_types.rs"]
mod scalar_types;
#[path = "expression/set_intrinsics.rs"]
mod set_intrinsics;
#[path = "expression/string_intrinsics.rs"]
mod string_intrinsics;
#[path = "expression/type_mapping.rs"]
mod type_mapping;
#[path = "expression/value_intrinsics.rs"]
mod value_intrinsics;

use equality::{lower_equality_operand, managed_equality_semantic};
use field_access::lower_managed_field_access;
pub(super) use free_variable_analysis::free_variables;
use type_mapping::is_empty_list;
pub(super) use type_mapping::{
    core_string_runtime_value, literal_collection_type, managed_semantic_contract, native_type,
    witnessed_collection_type,
};

mod inference;
mod scalar_detection;

pub(super) use inference::*;
pub(super) use scalar_detection::expr_is_scalar;

struct ExpectedFieldContext<'a> {
    params: &'a HashMap<String, usize>,
    param_types: &'a HashMap<String, NativeType>,
    functions: &'a HashMap<(String, usize), usize>,
    function_types: &'a HashMap<(String, usize), NativeType>,
    constructors: &'a NativeConstructorLayouts,
}

/// Lowers one field against its checked type, including scalar control flow
/// embedded inside a constructor or record value.
fn lower_expected_field(
    field: &CoreExpr,
    expected: &CoreType,
    type_error_code: &str,
    context: &ExpectedFieldContext<'_>,
) -> Result<(NativeExpr, NativeType), String> {
    let expected_native = native_type(Some(expected), &expected.contract_text())
        .ok_or_else(|| format!("error[{type_error_code}]: expected field type is not native"))?;
    let lowered = super::collection_values::try_lower_typed_value(
        field,
        expected,
        context.params,
        context.param_types,
        context.functions,
        context.function_types,
        context.constructors,
    )
    .map_err(|error| remap_field_type_error(error, type_error_code))?;
    if let Some(lowered) = lowered {
        return Ok((lowered, expected_native));
    }
    let actual = infer_native_type_for_lowering(
        field,
        context.param_types,
        context.function_types,
        context.constructors,
    )?
    .ok_or_else(|| {
        format!("error[native_ir.constructor_control_field]: cannot infer `{field:?}`")
    })?;
    if actual != expected_native {
        return Err(format!(
            "error[{type_error_code}]: expected {expected_native:?}, found {actual:?}"
        ));
    }
    let lowered = lower_expr_with_constructors(
        field,
        context.params,
        context.param_types,
        context.functions,
        context.function_types,
        context.constructors,
    )?;
    Ok((lowered, expected_native))
}

fn remap_field_type_error(error: String, type_error_code: &str) -> String {
    if error.starts_with("error[native_ir.collection_value]:")
        || error.starts_with("error[native_ir.collection_control_type]:")
    {
        let detail = error
            .split_once(": ")
            .map_or(error.as_str(), |(_, detail)| detail);
        format!("error[{type_error_code}]: {detail}")
    } else {
        error
    }
}

/// Lowers CoreIR with the fixed managed constructors visible to the module.
pub(super) fn lower_expr_with_constructors(
    expr: &CoreExpr,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<NativeExpr, String> {
    if let Some(operation) =
        super::template_values::lower_managed_template_operation(expr, |argument| {
            lower_expr_with_constructors(
                argument,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )
        })?
    {
        return Ok(operation);
    }
    if let Some(operation) = super::http_values::lower_managed_http_operation(expr, |argument| {
        lower_expr_with_constructors(
            argument,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        )
    })? {
        return Ok(operation);
    }
    match expr {
        CoreExpr::Atom(value) if value == "Unit" => Ok(NativeExpr::Unit),
        CoreExpr::Var(value) if value == "Unit" => Ok(NativeExpr::Unit),
        CoreExpr::Int(value) => Ok(NativeExpr::Int(*value)),
        CoreExpr::Float(value) => {
            let parsed = value.parse::<f64>().map_err(|error| {
                format!("error[native_ir.float]: invalid Float `{value}`: {error}")
            })?;
            if !parsed.is_finite() {
                return Err(format!(
                    "error[native_ir.float]: invalid Float `{value}`: value must be finite"
                ));
            }
            Ok(NativeExpr::Float(parsed.to_bits()))
        }
        CoreExpr::Binary(value) => {
            let value = core_string_runtime_value(value)?;
            let encoded = encode_string_literal(&value)
                .map_err(|error| format!("error[native_ir.string_literal]: {error}"))?;
            Ok(NativeExpr::ManagedLiteral {
                encoded: encoded.into(),
            })
        }
        CoreExpr::Atom(value) if value == "true" => Ok(NativeExpr::Bool(true)),
        CoreExpr::Atom(value) if value == "false" => Ok(NativeExpr::Bool(false)),
        CoreExpr::Atom(value) => Ok(NativeExpr::AtomLiteral(Arc::from(value.as_str()))),
        CoreExpr::Var(value) if value == "true" => Ok(NativeExpr::Bool(true)),
        CoreExpr::Var(value) if value == "false" => Ok(NativeExpr::Bool(false)),
        CoreExpr::Var(name) => params
            .get(name.as_str())
            .copied()
            .map(NativeExpr::Param)
            .ok_or_else(|| format!("error[native_ir.variable]: unknown scalar variable `{name}`")),
        CoreExpr::List(_) => {
            let expected = literal_collection_type(expr)
                .or_else(|| {
                    collection_literal_types::inferred_collection_literal_type(expr, |item| {
                        infer_native_type_impl(
                            item,
                            param_types,
                            function_types,
                            Some(constructors),
                        )
                    }, |item| {
                        constructor_result_core_type(item, constructors).or_else(|| {
                            infer_native_type_impl(
                                item,
                                param_types,
                                function_types,
                                Some(constructors),
                            )
                            .and_then(|native| {
                                super::constructors::result_core_type_for_native(
                                    native,
                                    constructors,
                                )
                            })
                        })
                    })
                })
                .ok_or_else(|| {
                format!(
                    "error[native_ir.list_literal_type]: cannot infer a homogeneous native list literal from {expr:?}"
                )
            })?;
            super::collection_values::lower_boundary_collection_value(
                expr,
                Some(&expected),
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?
            .ok_or_else(|| {
                "error[native_ir.list_literal]: expected a concrete native list literal".to_string()
            })
        }
        CoreExpr::ListCons { head, tail } => {
            let Some(NativeType::ManagedRef(semantic)) = infer_native_type_impl(
                tail,
                param_types,
                function_types,
                Some(constructors),
            ) else {
                return Err(
                    "error[native_ir.list_cons_type]: list tail has no concrete managed schema"
                        .to_string(),
                );
            };
            Ok(NativeExpr::ManagedOperation {
                encoded: Arc::from(encode_list_prepend_operation(semantic)),
                args: vec![
                    lower_expr_with_constructors(
                        head,
                        params,
                        param_types,
                        functions,
                        function_types,
                        constructors,
                    )?,
                    lower_expr_with_constructors(
                        tail,
                        params,
                        param_types,
                        functions,
                        function_types,
                        constructors,
                    )?,
                ],
            })
        }
        expr @ CoreExpr::ConstructorCall { .. } => {
            lower_constructor_call(expr, constructors, |field, expected_core| {
                if let Some(expected_core) = expected_core {
                    return lower_expected_field(
                        field,
                        expected_core,
                        "native_ir.collection_value",
                        &ExpectedFieldContext {
                            params,
                            param_types,
                            functions,
                            function_types,
                            constructors,
                        },
                    );
                }
                let ty = infer_native_type_for_lowering(
                    field,
                    param_types,
                    function_types,
                    constructors,
                )?
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.constructor_field]: cannot infer constructor field type for `{field:?}`"
                    )
                })?;
                let lowered = lower_expr_with_constructors(
                    field,
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                )?;
                Ok((lowered, ty))
            })?
            .map(|(lowered, _)| lowered)
            .ok_or_else(|| "error[native_ir.constructor]: expected constructor call".to_string())
        }
        expr @ CoreExpr::RecordConstruct { .. } => {
            let local_base = params
                .values()
                .copied()
                .max()
                .map_or(0, |index| index.saturating_add(1));
            lower_record_construct(expr, constructors, local_base, |field, expected_core| {
                if let Some(expected_core) = expected_core {
                    return lower_expected_field(
                        field,
                        expected_core,
                        "native_ir.record_field_type",
                        &ExpectedFieldContext {
                            params,
                            param_types,
                            functions,
                            function_types,
                            constructors,
                        },
                    );
                }
                let ty = infer_native_type_for_lowering(
                    field,
                    param_types,
                    function_types,
                    constructors,
                )?
                .ok_or_else(|| {
                    "error[native_ir.record_field_type]: cannot infer record field type".to_string()
                })?;
                let lowered = lower_expr_with_constructors(
                    field,
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                )?;
                Ok((lowered, ty))
            })?
            .map(|(lowered, _)| lowered)
            .ok_or_else(|| {
                "error[native_ir.record_construct]: expected record construction".to_string()
            })
        }
        expr @ CoreExpr::RecordUpdate { .. } => {
            let local_base = params
                .values()
                .copied()
                .max()
                .map_or(0, |index| index.saturating_add(1));
            lower_record_update(expr, constructors, local_base, |value| {
                let ty = infer_native_type_for_lowering(
                    value,
                    param_types,
                    function_types,
                    constructors,
                )?
                .ok_or_else(|| {
                    "error[native_ir.record_update_type]: cannot infer record update value"
                        .to_string()
                })?;
                let lowered = lower_expr_with_constructors(
                    value,
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                )?;
                Ok((lowered, ty))
            })?
            .map(|(lowered, _)| lowered)
            .ok_or_else(|| "error[native_ir.record_update]: expected record update".to_string())
        }
        CoreExpr::FieldAccess { base, field } => lower_managed_field_access(
            field_access::ManagedFieldAccess {
                base,
                record_name: None,
                field,
            },
            params,
            param_types,
            functions,
            function_types,
            constructors,
        ),
        CoreExpr::RecordAccess { base, name, field } => lower_managed_field_access(
            field_access::ManagedFieldAccess {
                base,
                record_name: Some(name),
                field,
            },
            params,
            param_types,
            functions,
            function_types,
            constructors,
        ),
        CoreExpr::Call { function, args } => {
            let index = functions
                .get(&(function.clone(), args.len()))
                .copied()
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.call]: scalar call `{function}/{}` is not in the native module",
                        args.len()
                    )
                })?;
            Ok(NativeExpr::Call {
                function: index,
                args: args
                    .iter()
                    .map(|arg| {
                        lower_expr_with_constructors(
                            arg,
                            params,
                            param_types,
                            functions,
                            function_types,
                            constructors,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            })
        }
        CoreExpr::UnaryOp { operator, operand } if operator == "-" => {
            let operand_type =
                infer_native_type_for_lowering(operand, param_types, function_types, constructors)?
                    .ok_or_else(|| {
                        "error[native_ir.operator]: unsupported scalar negation".to_string()
                    })?;
            let operand = Box::new(lower_expr_with_constructors(
                operand,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?);
            match operand_type {
                NativeType::Int => Ok(NativeExpr::Neg(operand)),
                NativeType::Float => Ok(NativeExpr::FloatNeg(operand)),
                _ => Err("error[native_ir.operator]: unsupported scalar negation".to_string()),
            }
        }
        CoreExpr::UnaryOp { operator, operand } if matches!(operator.as_str(), "not" | "!") => {
            Ok(NativeExpr::Not(Box::new(lower_expr_with_constructors(
                operand,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?)))
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => {
            if operator == "and" {
                return Ok(NativeExpr::If {
                    clauses: vec![
                        (
                            lower_expr_with_constructors(
                                left,
                                params,
                                param_types,
                                functions,
                                function_types,
                                constructors,
                            )?,
                            lower_expr_with_constructors(
                                right,
                                params,
                                param_types,
                                functions,
                                function_types,
                                constructors,
                            )?,
                        ),
                        (NativeExpr::Bool(true), NativeExpr::Bool(false)),
                    ],
                });
            }
            if operator == "or" {
                return Ok(NativeExpr::If {
                    clauses: vec![
                        (
                            lower_expr_with_constructors(
                                left,
                                params,
                                param_types,
                                functions,
                                function_types,
                                constructors,
                            )?,
                            NativeExpr::Bool(true),
                        ),
                        (
                            NativeExpr::Bool(true),
                            lower_expr_with_constructors(
                                right,
                                params,
                                param_types,
                                functions,
                                function_types,
                                constructors,
                            )?,
                        ),
                    ],
                });
            }
            let mut left_type = infer_native_type_for_lowering(
                left,
                param_types,
                function_types,
                constructors,
            )?;
            let mut right_type = infer_native_type_for_lowering(
                right,
                param_types,
                function_types,
                constructors,
            )?;
            if matches!(operator.as_str(), "==" | "!=") {
                if left_type.is_none() && is_empty_list(left) {
                    left_type = right_type;
                }
                if right_type.is_none() && is_empty_list(right) {
                    right_type = left_type;
                }
                if left_type.is_none()
                    && matches!(right_type, Some(NativeType::ManagedRef(_)))
                    && matches!(left.as_ref(), CoreExpr::Tuple(_) | CoreExpr::Map(_))
                {
                    left_type = right_type;
                }
                if right_type.is_none()
                    && matches!(left_type, Some(NativeType::ManagedRef(_)))
                    && matches!(right.as_ref(), CoreExpr::Tuple(_) | CoreExpr::Map(_))
                {
                    right_type = left_type;
                }
            }
            let left_type = left_type.ok_or_else(|| {
                    format!(
                        "error[native_ir.operator]: scalar operator `{operator}` has an unsupported left operand: {left:?}"
                    )
                })?;
            let right_type = right_type.ok_or_else(|| {
                    format!(
                        "error[native_ir.operator]: scalar operator `{operator}` has an unsupported right operand: {right:?}"
                    )
                })?;
            let numeric = matches!(left_type, NativeType::Int | NativeType::Float)
                && matches!(right_type, NativeType::Int | NativeType::Float);
            let operand_type = if left_type == right_type {
                left_type
            } else if matches!(operator.as_str(), "==" | "!=")
                && matches!(
                    (left_type, right_type),
                    (NativeType::ManagedRef(_), NativeType::Atom)
                        | (NativeType::Atom, NativeType::ManagedRef(_))
                )
            {
                if matches!(left_type, NativeType::ManagedRef(_)) {
                    left_type
                } else {
                    right_type
                }
            } else if numeric
                && matches!(
                    operator.as_str(),
                    "+" | "-" | "*" | "/" | "<" | "<=" | ">" | ">="
                )
            {
                NativeType::Float
            } else {
                return Err(format!(
                    "error[native_ir.operator]: scalar operator `{operator}` has incompatible operands: left={left_type:?} `{}`, right={right_type:?} `{}`",
                    left.contract_text(),
                    right.contract_text()
                ));
            };
            if operator == "+" && operand_type == NativeType::StringRef {
                return Ok(NativeExpr::ManagedOperation {
                    encoded: Arc::from(encode_string_append_operation()),
                    args: vec![
                        lower_expr_with_constructors(
                            left,
                            params,
                            param_types,
                            functions,
                            function_types,
                            constructors,
                        )?,
                        lower_expr_with_constructors(
                            right,
                            params,
                            param_types,
                            functions,
                            function_types,
                            constructors,
                        )?,
                    ],
                });
            }
            if let Some(semantic) = managed_equality_semantic(operand_type) {
                if !matches!(operator.as_str(), "==" | "!=") {
                    return Err(format!(
                        "error[native_ir.operator]: managed operator `{operator}` is unsupported"
                    ));
                }
                let equality = NativeExpr::ManagedOperation {
                    encoded: encode_managed_value_equal_operation(semantic).into(),
                    args: vec![
                        lower_equality_operand(
                            left,
                            operand_type,
                            params,
                            param_types,
                            functions,
                            function_types,
                            constructors,
                        )?,
                        lower_equality_operand(
                            right,
                            operand_type,
                            params,
                            param_types,
                            functions,
                            function_types,
                            constructors,
                        )?,
                    ],
                };
                return if operator == "==" {
                    Ok(equality)
                } else {
                    Ok(NativeExpr::Not(Box::new(equality)))
                };
            }
            let operator = match operator.as_str() {
                "+" => NativeBinaryOperator::Add,
                "-" => NativeBinaryOperator::Subtract,
                "*" => NativeBinaryOperator::Multiply,
                "/" | "div" => NativeBinaryOperator::Divide,
                "rem" => NativeBinaryOperator::Remainder,
                "==" => NativeBinaryOperator::Equal,
                "!=" => NativeBinaryOperator::NotEqual,
                "<" => NativeBinaryOperator::LessThan,
                "<=" => NativeBinaryOperator::LessThanOrEqual,
                ">" => NativeBinaryOperator::GreaterThan,
                ">=" => NativeBinaryOperator::GreaterThanOrEqual,
                _ => {
                    return Err(format!(
                        "error[native_ir.operator]: unsupported scalar operator `{operator}`"
                    ));
                }
            };
            let mut left = lower_expr_with_constructors(
                left,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?;
            let mut right = lower_expr_with_constructors(
                right,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )?;
            if operand_type == NativeType::Float {
                if left_type == NativeType::Int {
                    left = NativeExpr::IntToFloat(Box::new(left));
                }
                if right_type == NativeType::Int {
                    right = NativeExpr::IntToFloat(Box::new(right));
                }
            }
            Ok(NativeExpr::Binary {
                operator,
                operand_type,
                left: Box::new(left),
                right: Box::new(right),
            })
        }
        CoreExpr::Let { bindings, body } => {
            let mut locals = params.clone();
            let mut local_types = param_types.clone();
            let mut next_local = locals
                .values()
                .copied()
                .max()
                .map_or(0, |index| index.saturating_add(1));
            let mut lowered = Vec::with_capacity(bindings.len());
            let retained = retained_managed_bindings(bindings, body);
            for (binding, retained) in bindings.iter().zip(retained) {
                let CorePattern::Var(name) = &binding.pattern else {
                    return Err(
                        "error[native_ir.let_pattern]: scalar let requires a variable pattern"
                            .to_string(),
                    );
                };
                if !retained {
                    continue;
                }
                let binding_type = infer_native_type_for_lowering(
                    &binding.value,
                    &local_types,
                    function_types,
                    constructors,
                )?
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.let_type]: cannot infer scalar binding `{name}` from {value:?}",
                        value = binding.value
                    )
                })?;
                lowered.push(lower_expr_with_constructors(
                    &binding.value,
                    &locals,
                    &local_types,
                    functions,
                    function_types,
                    constructors,
                )?);
                locals.insert(name.clone(), next_local);
                local_types.insert(name.clone(), binding_type);
                next_local = next_local.saturating_add(1);
            }
            let body = lower_expr_with_constructors(
                body,
                &locals,
                &local_types,
                functions,
                function_types,
                constructors,
            )?;
            Ok(if lowered.is_empty() {
                body
            } else {
                NativeExpr::Let {
                    bindings: lowered,
                    body: Box::new(body),
                }
            })
        }
        CoreExpr::Cast { expr, target_type } => {
            if matches!(expr.as_ref(), CoreExpr::Binary(_))
                && matches!(target_type, CoreType::Binary)
            {
                return super::collection_values::lower_typed_value(
                    expr,
                    target_type,
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                );
            }
            if super::collection_values::is_none_option_value(expr, target_type) {
                return super::collection_values::lower_typed_value(
                    expr,
                    target_type,
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                );
            }
            if matches!(
                expr.as_ref(),
                CoreExpr::List(_) | CoreExpr::Tuple(_) | CoreExpr::Map(_)
            ) {
                return super::collection_values::lower_boundary_collection_value(
                    expr,
                    Some(target_type),
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                )?
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.cast_collection]: cast target `{}` is not a concrete native collection",
                        target_type.contract_text()
                    )
                });
            }
            if matches!(
                expr.as_ref(),
                CoreExpr::ConstructorCall { constructor, .. }
                    if matches!(constructor.rsplit('.').next(), Some("List" | "Map"))
            ) {
                return super::collection_values::lower_typed_value(
                    expr,
                    target_type,
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                );
            }
            if let Some(lowered) = lower_structural_record_construct(
                expr,
                target_type,
                constructors,
                |field| {
                    let ty = infer_native_type_for_lowering(
                        field,
                        param_types,
                        function_types,
                        constructors,
                    )?
                    .ok_or_else(|| {
                        "error[native_ir.structural_record_field_type]: cannot infer field"
                            .to_string()
                    })?;
                    let lowered = lower_expr_with_constructors(
                        field,
                        params,
                        param_types,
                        functions,
                        function_types,
                        constructors,
                    )?;
                    Ok((lowered, ty))
                },
            )? {
                return Ok(lowered);
            }
            if let Some(lowered) =
                lower_structural_constructor_call(expr, target_type, |field, expected_core| {
                    if let Some(lowered) =
                        super::collection_values::lower_boundary_collection_value(
                            field,
                            Some(expected_core),
                            params,
                            param_types,
                            functions,
                            function_types,
                            constructors,
                        )?
                    {
                        let ty = native_type(
                            Some(expected_core),
                            &expected_core.contract_text(),
                        )
                        .ok_or_else(|| {
                            "error[native_ir.structural_constructor_field_type]: expected field is not native"
                                .to_string()
                        })?;
                        return Ok((lowered, ty));
                    }
                    let ty = infer_native_type_for_lowering(
                        field,
                        param_types,
                        function_types,
                        constructors,
                    )?
                    .ok_or_else(|| {
                        "error[native_ir.structural_constructor_field_type]: cannot infer field"
                            .to_string()
                    })?;
                    let lowered = lower_expr_with_constructors(
                        field,
                        params,
                        param_types,
                        functions,
                        function_types,
                        constructors,
                    )?;
                    Ok((lowered, ty))
                })?
            {
                return Ok(lowered);
            }
            let source = infer_native_type_for_lowering(
                expr,
                param_types,
                function_types,
                constructors,
            )?
            .ok_or_else(|| {
                format!("error[native_ir.cast_source]: cannot infer cast source for {expr:?}")
            })?;
            let target = native_type(Some(target_type), &target_type.contract_text())
                .ok_or_else(|| "error[native_ir.cast_target]: unsupported cast target".to_string())?;
            if source != target {
                return Err(format!(
                    "error[native_ir.cast_check]: cast changes native representation from {source:?} to {target:?} for {expr:?} -> {}",
                    target_type.contract_text()
                ));
            }
            lower_expr_with_constructors(
                expr,
                params,
                param_types,
                functions,
                function_types,
                constructors,
            )
        }
        CoreExpr::Intrinsic(call) => intrinsics::lower_intrinsic(
            call,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        ),
        CoreExpr::If { clauses } if !clauses.is_empty() => Ok(NativeExpr::If {
            clauses: clauses
                .iter()
                .map(|clause| {
                    Ok((
                        lower_expr_with_constructors(
                            &clause.condition,
                            params,
                            param_types,
                            functions,
                            function_types,
                            constructors,
                        )?,
                        lower_expr_with_constructors(
                            &clause.body,
                            params,
                            param_types,
                            functions,
                            function_types,
                            constructors,
                        )?,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?,
        }),
        unsupported => Err(format!(
            "error[native_ir.expression]: expression is not in the scalar native profile: {unsupported:?}"
        )),
    }
}
