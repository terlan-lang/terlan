use std::collections::HashMap;

use crate::runtime::native_image::managed::{
    encode_managed_value_equal_operation, encode_string_literal, ManagedClosureDescriptor,
    SemanticTypeId,
};
use crate::terlan_typeck::{CoreExpr, CorePattern, CoreType};

use super::{
    constructors::{
        constructor_result_type, lower_constructor_call, lower_record_construct,
        lower_record_update, managed_field_projection, record_construct_result_type,
        record_update_result_type, NativeConstructorLayouts,
    },
    escape::retained_managed_bindings,
    NativeBinaryOperator, NativeExpr, NativeType,
};

#[path = "expression/free_variables.rs"]
mod free_variable_analysis;
#[cfg(test)]
#[path = "expression/free_variables_test.rs"]
mod free_variable_analysis_test;

pub(super) use free_variable_analysis::free_variables;

pub(super) fn infer_native_type(
    expr: &CoreExpr,
    variables: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), NativeType>,
) -> Option<NativeType> {
    infer_native_type_impl(expr, variables, functions, None)
}

/// Infers one expression type with fixed managed layouts available.
pub(super) fn infer_native_type_with_constructors(
    expr: &CoreExpr,
    variables: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Option<NativeType> {
    infer_native_type_impl(expr, variables, functions, Some(constructors))
}

/// Infers a lowering type while preserving managed-projection diagnostics.
fn infer_native_type_for_lowering(
    expr: &CoreExpr,
    variables: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<Option<NativeType>, String> {
    match expr {
        CoreExpr::FieldAccess { base, field } => {
            let Some(base) =
                infer_native_type_for_lowering(base, variables, functions, constructors)?
            else {
                return Ok(None);
            };
            managed_field_projection(base, None, field, constructors).map(|(_, ty)| Some(ty))
        }
        CoreExpr::RecordAccess { base, name, field } => {
            let Some(base) =
                infer_native_type_for_lowering(base, variables, functions, constructors)?
            else {
                return Ok(None);
            };
            managed_field_projection(base, Some(name), field, constructors).map(|(_, ty)| Some(ty))
        }
        CoreExpr::RecordUpdate { base, name, .. } => {
            let Some(base) =
                infer_native_type_for_lowering(base, variables, functions, constructors)?
            else {
                return Ok(None);
            };
            record_update_result_type(name, base, constructors).map(Some)
        }
        _ => Ok(infer_native_type_with_constructors(
            expr,
            variables,
            functions,
            constructors,
        )),
    }
}

/// Shared recursive native-type inference implementation.
fn infer_native_type_impl(
    expr: &CoreExpr,
    variables: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), NativeType>,
    constructors: Option<&NativeConstructorLayouts>,
) -> Option<NativeType> {
    if let Some(ty) = super::template_values::managed_template_operation_type(expr) {
        return Some(ty);
    }
    if let Some(ty) = super::http_values::managed_http_operation_type(expr) {
        return Some(ty);
    }
    if let Some(ty) = super::list_comprehension::managed_comprehension_operation_type(expr) {
        return Some(ty);
    }
    match expr {
        CoreExpr::Atom(value) | CoreExpr::Var(value) if value == "Unit" => Some(NativeType::Unit),
        CoreExpr::Int(_) => Some(NativeType::Int),
        CoreExpr::Float(_) => Some(NativeType::Float),
        CoreExpr::Binary(_) => Some(NativeType::StringRef),
        CoreExpr::Atom(value) | CoreExpr::Var(value)
            if matches!(value.as_str(), "true" | "false") =>
        {
            Some(NativeType::Bool)
        }
        CoreExpr::Var(name) => variables.get(name).copied(),
        CoreExpr::Call { function, args } => {
            functions.get(&(function.clone(), args.len())).copied()
        }
        CoreExpr::ConstructorCall { .. } => {
            constructors.and_then(|layouts| constructor_result_type(expr, layouts))
        }
        CoreExpr::RecordConstruct { .. } => {
            constructors.and_then(|layouts| record_construct_result_type(expr, layouts))
        }
        CoreExpr::RecordUpdate { base, name, .. } => {
            let base = infer_native_type_impl(base, variables, functions, constructors)?;
            record_update_result_type(name, base, constructors?).ok()
        }
        CoreExpr::FieldAccess { base, field } => {
            let base = infer_native_type_impl(base, variables, functions, constructors)?;
            managed_field_projection(base, None, field, constructors?)
                .ok()
                .map(|(_, ty)| ty)
        }
        CoreExpr::RecordAccess { base, name, field } => {
            let base = infer_native_type_impl(base, variables, functions, constructors)?;
            managed_field_projection(base, Some(name), field, constructors?)
                .ok()
                .map(|(_, ty)| ty)
        }
        CoreExpr::UnaryOp { operator, operand } => match operator.as_str() {
            "-" => match infer_native_type_impl(operand, variables, functions, constructors) {
                Some(ty @ (NativeType::Int | NativeType::Float)) => Some(ty),
                _ => None,
            },
            "not" | "!" => (infer_native_type_impl(operand, variables, functions, constructors)
                == Some(NativeType::Bool))
            .then_some(NativeType::Bool),
            _ => None,
        },
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => {
            let left = infer_native_type_impl(left, variables, functions, constructors)?;
            let right = infer_native_type_impl(right, variables, functions, constructors)?;
            match operator.as_str() {
                "+" | "-" | "*" | "/"
                    if matches!(left, NativeType::Int | NativeType::Float)
                        && matches!(right, NativeType::Int | NativeType::Float) =>
                {
                    Some(if left == NativeType::Float || right == NativeType::Float {
                        NativeType::Float
                    } else {
                        NativeType::Int
                    })
                }
                "div" | "rem" if left == NativeType::Int && right == NativeType::Int => {
                    Some(NativeType::Int)
                }
                "==" | "!="
                    if left == right
                        && (native_word_equality(left)
                            || matches!(left, NativeType::ManagedRef(_))) =>
                {
                    Some(NativeType::Bool)
                }
                "<" | "<=" | ">" | ">="
                    if matches!(left, NativeType::Int | NativeType::Float)
                        && matches!(right, NativeType::Int | NativeType::Float) =>
                {
                    Some(NativeType::Bool)
                }
                "and" | "&&" | "or" | "||"
                    if left == NativeType::Bool && right == NativeType::Bool =>
                {
                    Some(NativeType::Bool)
                }
                _ => None,
            }
        }
        CoreExpr::Let { bindings, body } => {
            let mut locals = variables.clone();
            for binding in bindings {
                let CorePattern::Var(name) = &binding.pattern else {
                    return None;
                };
                let ty = infer_native_type_impl(&binding.value, &locals, functions, constructors)?;
                locals.insert(name.clone(), ty);
            }
            infer_native_type_impl(body, &locals, functions, constructors)
        }
        CoreExpr::If { clauses } => {
            let mut types = clauses.iter().map(|clause| {
                infer_native_type_impl(&clause.body, variables, functions, constructors)
            });
            let first = types.next()??;
            types.all(|ty| ty == Some(first)).then_some(first)
        }
        CoreExpr::Cast { expr, target_type } => {
            let source = infer_native_type_impl(expr, variables, functions, constructors)?;
            let target = native_type(Some(target_type), &target_type.contract_text())?;
            (source == target).then_some(target)
        }
        _ => None,
    }
}

pub(super) fn expr_is_scalar(expr: &CoreExpr) -> bool {
    if super::template_values::managed_template_operation_type(expr).is_some() {
        let CoreExpr::RemoteCall { args, .. } = expr else {
            unreachable!("managed template operations are remote calls");
        };
        return args.iter().all(expr_is_scalar);
    }
    if super::http_values::managed_http_operation_type(expr).is_some() {
        let CoreExpr::RemoteCall { args, .. } = expr else {
            unreachable!("managed HTTP operations are remote calls");
        };
        return args.iter().all(expr_is_scalar);
    }
    if super::list_comprehension::managed_comprehension_operation_type(expr).is_some() {
        let CoreExpr::RemoteCall { args, .. } = expr else {
            unreachable!("managed comprehension operations are remote calls");
        };
        return args.iter().all(expr_is_scalar);
    }
    match expr {
        CoreExpr::Int(_) | CoreExpr::Float(_) | CoreExpr::Binary(_) | CoreExpr::Var(_) => true,
        CoreExpr::Atom(value) => matches!(value.as_str(), "Unit" | "true" | "false"),
        CoreExpr::Call { args, .. } | CoreExpr::ConstructorCall { args, .. } => {
            args.iter().all(expr_is_scalar)
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) => items.iter().all(expr_is_scalar),
        CoreExpr::ListCons { head, tail } => expr_is_scalar(head) && expr_is_scalar(tail),
        CoreExpr::Map(fields) => fields.iter().all(|field| expr_is_scalar(&field.value)),
        CoreExpr::FunctionCall { callee, args } => {
            expr_is_scalar(callee) && args.iter().all(expr_is_scalar)
        }
        CoreExpr::Cast { expr, .. } => expr_is_scalar(expr),
        CoreExpr::RecordConstruct { fields, .. } => {
            fields.iter().all(|field| expr_is_scalar(&field.value))
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            expr_is_scalar(base) && fields.iter().all(|field| expr_is_scalar(&field.value))
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            expr_is_scalar(base)
        }
        CoreExpr::UnaryOp { operator, operand } => {
            matches!(operator.as_str(), "-" | "not" | "!") && expr_is_scalar(operand)
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => {
            matches!(
                operator.as_str(),
                "+" | "-"
                    | "*"
                    | "/"
                    | "div"
                    | "rem"
                    | "=="
                    | "!="
                    | "<"
                    | "<="
                    | ">"
                    | ">="
                    | "and"
                    | "&&"
                    | "or"
                    | "||"
            ) && expr_is_scalar(left)
                && expr_is_scalar(right)
        }
        CoreExpr::Let { bindings, body } => {
            bindings.iter().all(|binding| {
                matches!(binding.pattern, CorePattern::Var(_)) && expr_is_scalar(&binding.value)
            }) && expr_is_scalar(body)
        }
        CoreExpr::If { clauses } => {
            !clauses.is_empty()
                && clauses
                    .iter()
                    .all(|clause| expr_is_scalar(&clause.condition) && expr_is_scalar(&clause.body))
        }
        _ => false,
    }
}

pub(super) fn native_type(core: Option<&CoreType>, text: &str) -> Option<NativeType> {
    match core {
        Some(CoreType::Named(name)) if name == "Unit" => Some(NativeType::Unit),
        Some(CoreType::Int) => Some(NativeType::Int),
        Some(CoreType::Float) => Some(NativeType::Float),
        Some(CoreType::Bool) => Some(NativeType::Bool),
        Some(CoreType::Atom | CoreType::AtomLiteral(_)) => Some(NativeType::Atom),
        Some(CoreType::String) => Some(NativeType::StringRef),
        Some(CoreType::Named(name)) if super::template_values::is_template_html_type(name) => {
            Some(NativeType::StringRef)
        }
        Some(CoreType::Binary) => Some(NativeType::BinaryRef),
        Some(CoreType::Arrow {
            params,
            return_type,
        }) => {
            let parameters = params
                .iter()
                .map(|ty| native_type(Some(ty), &ty.contract_text()).map(NativeType::boundary_type))
                .collect::<Option<Vec<_>>>()?;
            let result =
                native_type(Some(return_type), &return_type.contract_text())?.boundary_type();
            ManagedClosureDescriptor::semantic_id_for_signature(&parameters, &[result])
                .ok()
                .map(NativeType::ManagedRef)
        }
        Some(CoreType::Named(name)) if matches!(name.as_str(), "Bytes" | "std.binary.Bytes") => {
            Some(NativeType::BytesRef)
        }
        Some(CoreType::Named(name))
            if matches!(name.as_str(), "Binary" | "BitString" | "std.binary.Binary") =>
        {
            Some(NativeType::BinaryRef)
        }
        Some(CoreType::Apply { constructor, args })
            if constructor.rsplit('.').next() == Some("Process") && args.len() == 1 =>
        {
            Some(NativeType::Int)
        }
        Some(CoreType::Apply { constructor, args })
            if matches!(
                constructor.rsplit('.').next(),
                Some("Entry" | "Monitor" | "ResourceKind" | "Resource")
            ) && args.len() == 1 =>
        {
            Some(NativeType::Int)
        }
        Some(CoreType::Apply { constructor, args })
            if constructor.rsplit('.').next() == Some("Message") && args.len() == 1 =>
        {
            native_type(Some(&args[0]), &args[0].contract_text())
        }
        Some(CoreType::Named(name))
            if matches!(
                name.rsplit('.').next(),
                Some("Timer" | "ExitReason" | "SchedulingClass")
            ) =>
        {
            Some(NativeType::Int)
        }
        Some(core @ CoreType::Named(_)) => managed_reference_type(core),
        Some(
            core @ (CoreType::Tuple(_)
            | CoreType::Struct { .. }
            | CoreType::Union(_)
            | CoreType::List(_)),
        ) => managed_reference_type(core),
        Some(core @ CoreType::Apply { constructor, .. })
            if managed_aggregate_constructor(constructor) =>
        {
            managed_reference_type(core)
        }
        None if text == "Unit" => Some(NativeType::Unit),
        None if text == "Int" => Some(NativeType::Int),
        None if text == "Float" => Some(NativeType::Float),
        None if text == "Bool" => Some(NativeType::Bool),
        None if text == "Atom" => Some(NativeType::Atom),
        None if text == "String" => Some(NativeType::StringRef),
        None if matches!(text, "Bytes" | "std.binary.Bytes") => Some(NativeType::BytesRef),
        None if matches!(text, "Binary" | "BitString" | "std.binary.Binary") => {
            Some(NativeType::BinaryRef)
        }
        _ => None,
    }
}

/// Decodes one checked CoreIR string payload into its runtime UTF-8 value.
fn core_string_runtime_value(value: &str) -> Result<String, String> {
    if value.starts_with('"') && value.ends_with('"') {
        serde_json::from_str(value)
            .map_err(|error| format!("error[native_ir.string_literal]: {error}"))
    } else {
        Ok(value.to_string())
    }
}

/// Creates an exact managed-reference kind from canonical checked CoreIR type text.
fn managed_reference_type(core: &CoreType) -> Option<NativeType> {
    SemanticTypeId::from_canonical(&core.contract_text())
        .ok()
        .map(NativeType::ManagedRef)
}

/// Reports whether an applied type uses a fixed aggregate or algebraic layout.
fn managed_aggregate_constructor(constructor: &str) -> bool {
    matches!(
        constructor.rsplit('.').next(),
        Some("Option" | "Result" | "Array" | "FixedArray" | "List" | "Map" | "Set")
    )
}

/// Reports whether native equality can compare the complete value in one word.
fn native_word_equality(ty: NativeType) -> bool {
    matches!(
        ty,
        NativeType::Unit | NativeType::Int | NativeType::Bool | NativeType::Atom
    )
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
    if let Some(operation) =
        super::list_comprehension::lower_managed_comprehension_operation(expr, |argument| {
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
            Ok(NativeExpr::StringLiteral {
                encoded: encoded.into(),
            })
        }
        CoreExpr::Atom(value) if value == "true" => Ok(NativeExpr::Bool(true)),
        CoreExpr::Atom(value) if value == "false" => Ok(NativeExpr::Bool(false)),
        CoreExpr::Var(value) if value == "true" => Ok(NativeExpr::Bool(true)),
        CoreExpr::Var(value) if value == "false" => Ok(NativeExpr::Bool(false)),
        CoreExpr::Var(name) => params
            .get(name.as_str())
            .copied()
            .map(NativeExpr::Param)
            .ok_or_else(|| format!("error[native_ir.variable]: unknown scalar variable `{name}`")),
        expr @ CoreExpr::ConstructorCall { .. } => {
            lower_constructor_call(expr, constructors, |field| {
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
            lower_record_construct(expr, constructors, local_base, |field| {
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
            base,
            None,
            field,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        ),
        CoreExpr::RecordAccess { base, name, field } => lower_managed_field_access(
            base,
            Some(name),
            field,
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
            if matches!(operator.as_str(), "and" | "&&") {
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
            if matches!(operator.as_str(), "or" | "||") {
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
            let left_type = infer_native_type_for_lowering(
                left,
                param_types,
                function_types,
                constructors,
            )?
            .ok_or_else(|| {
                    format!(
                        "error[native_ir.operator]: scalar operator `{operator}` has an unsupported left operand"
                    )
                })?;
            let right_type = infer_native_type_for_lowering(
                right,
                param_types,
                function_types,
                constructors,
            )?
            .ok_or_else(|| {
                    format!(
                        "error[native_ir.operator]: scalar operator `{operator}` has an unsupported right operand"
                    )
                })?;
            let numeric = matches!(left_type, NativeType::Int | NativeType::Float)
                && matches!(right_type, NativeType::Int | NativeType::Float);
            let operand_type = if left_type == right_type {
                left_type
            } else if numeric
                && matches!(
                    operator.as_str(),
                    "+" | "-" | "*" | "/" | "<" | "<=" | ">" | ">="
                )
            {
                NativeType::Float
            } else {
                return Err(format!(
                    "error[native_ir.operator]: scalar operator `{operator}` has incompatible operands"
                ));
            };
            if let NativeType::ManagedRef(semantic) = operand_type {
                if !matches!(operator.as_str(), "==" | "!=") {
                    return Err(format!(
                        "error[native_ir.operator]: managed operator `{operator}` is unsupported"
                    ));
                }
                let equality = NativeExpr::ManagedOperation {
                    encoded: encode_managed_value_equal_operation(semantic).into(),
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
                    format!("error[native_ir.let_type]: cannot infer scalar binding `{name}`")
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
            let source = infer_native_type_for_lowering(
                expr,
                param_types,
                function_types,
                constructors,
            )?
            .ok_or_else(|| "error[native_ir.cast_source]: cannot infer cast source".to_string())?;
            let target = native_type(Some(target_type), &target_type.contract_text())
                .ok_or_else(|| "error[native_ir.cast_target]: unsupported cast target".to_string())?;
            if source != target {
                return Err(format!(
                    "error[native_ir.cast_check]: cast changes native representation from {source:?} to {target:?}"
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
        _ => Err(
            "error[native_ir.expression]: expression is not in the scalar native profile"
                .to_string(),
        ),
    }
}

/// Lowers one checked named field read through the bounded managed-operation ABI.
#[allow(clippy::too_many_arguments)]
fn lower_managed_field_access(
    base: &CoreExpr,
    record_name: Option<&str>,
    field: &str,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
) -> Result<NativeExpr, String> {
    let base_type =
        infer_native_type_for_lowering(base, param_types, function_types, constructors)?
            .ok_or_else(|| {
                format!(
                    "error[native_ir.field_base]: cannot infer receiver `{}` for field `{field}`",
                    base.contract_text()
                )
            })?;
    let (encoded, _) = managed_field_projection(base_type, record_name, field, constructors)?;
    let base = lower_expr_with_constructors(
        base,
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )?;
    Ok(NativeExpr::ManagedOperation {
        encoded,
        args: vec![base],
    })
}
