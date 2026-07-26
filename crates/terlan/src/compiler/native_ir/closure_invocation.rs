//! Type-directed lowering of owned closure calls at native function boundaries.

use std::collections::HashMap;
use std::sync::Arc;

use crate::runtime::native_image::managed::{encode_closure_allocation, ManagedClosureDescriptor};
use crate::terlan_typeck::{CoreExpr, CoreFunction, CoreType};

use super::transitions::is_process_transition;
use super::{
    closure_conversion::NativeCallableShape, infer_native_type_with_constructors,
    lower_expr_with_constructors, native_type, NativeConstructorLayouts, NativeExpr, NativeType,
};

#[derive(Clone, Debug, Eq, PartialEq)]
struct ClosureSignature {
    parameters: Vec<NativeType>,
    result: NativeType,
}

/// Lowers a tail-position call through one declared closure parameter.
#[allow(clippy::too_many_arguments)]
pub(super) fn lower_boundary_closure_invocation(
    body: &CoreExpr,
    function: &CoreFunction,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    callable_shapes: &HashMap<(String, usize), NativeCallableShape>,
    constructors: &NativeConstructorLayouts,
) -> Result<Option<NativeExpr>, String> {
    let mut signatures = HashMap::new();
    for parameter in &function.params {
        let Some(core_type) = parameter.core_ty.as_ref() else {
            continue;
        };
        if let Some(signature) = signature_from_type(core_type)? {
            signatures.insert(parameter.name.clone(), signature);
        }
    }
    lower_closure_invocation_at(
        body,
        function,
        params,
        param_types,
        &signatures,
        functions,
        function_types,
        callable_shapes,
        constructors,
    )
}

#[allow(clippy::too_many_arguments)]
fn lower_closure_invocation_at(
    body: &CoreExpr,
    function: &CoreFunction,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    signatures: &HashMap<String, ClosureSignature>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    callable_shapes: &HashMap<(String, usize), NativeCallableShape>,
    constructors: &NativeConstructorLayouts,
) -> Result<Option<NativeExpr>, String> {
    if let CoreExpr::Let { bindings, body } = body {
        let mut slots = params.clone();
        let mut types = param_types.clone();
        let mut closure_signatures = signatures.clone();
        let mut lowered = Vec::with_capacity(bindings.len());
        let mut next_slot = slots
            .values()
            .copied()
            .max()
            .map_or(0, |slot| slot.saturating_add(1));
        for binding in bindings {
            if is_process_transition(&binding.value) {
                return Ok(None);
            }
            let crate::terlan_typeck::CorePattern::Var(name) = &binding.pattern else {
                return Ok(None);
            };
            if let Some(signature) =
                closure_value_signature(&binding.value, &closure_signatures, callable_shapes)?
            {
                lowered.push(lower_closure_value(
                    &binding.value,
                    &signature,
                    &slots,
                    &types,
                    &closure_signatures,
                    functions,
                    function_types,
                    callable_shapes,
                    constructors,
                )?);
                types.insert(name.clone(), closure_native_type(&signature)?);
                closure_signatures.insert(name.clone(), signature);
            } else {
                let Some(ty) = infer_native_type_with_constructors(
                    &binding.value,
                    &types,
                    function_types,
                    constructors,
                ) else {
                    // This is a speculative boundary-closure recognizer. A
                    // normal AOT control expression that it cannot type is
                    // not itself a dynamic-closure error; let the primary
                    // lowering path diagnose or lower it.
                    return Ok(None);
                };
                lowered.push(lower_expr_with_constructors(
                    &binding.value,
                    &slots,
                    &types,
                    functions,
                    function_types,
                    constructors,
                )?);
                types.insert(name.clone(), ty);
                closure_signatures.remove(name);
            }
            slots.insert(name.clone(), next_slot);
            next_slot = next_slot.saturating_add(1);
        }
        let Some(body) = lower_closure_invocation_at(
            body,
            function,
            &slots,
            &types,
            &closure_signatures,
            functions,
            function_types,
            callable_shapes,
            constructors,
        )?
        else {
            return Ok(None);
        };
        return Ok(Some(NativeExpr::Let {
            bindings: lowered,
            body: Box::new(body),
        }));
    }
    if let CoreExpr::If { clauses } = body {
        let mut lowered = Vec::with_capacity(clauses.len());
        for clause in clauses {
            let Some(branch) = lower_closure_invocation_at(
                &clause.body,
                function,
                params,
                param_types,
                signatures,
                functions,
                function_types,
                callable_shapes,
                constructors,
            )?
            else {
                return Ok(None);
            };
            lowered.push((
                lower_expr_with_constructors(
                    &clause.condition,
                    params,
                    param_types,
                    functions,
                    function_types,
                    constructors,
                )?,
                branch,
            ));
        }
        return Ok(Some(NativeExpr::If { clauses: lowered }));
    }
    let CoreExpr::FunctionCall { callee, args } = body else {
        return Ok(None);
    };
    let CoreExpr::Var(callee_name) = callee.as_ref() else {
        return Err(
            "error[native_ir.dynamic_callee]: dynamic native call requires an owned closure value"
                .to_string(),
        );
    };
    let signature = signatures.get(callee_name).ok_or_else(|| {
        format!("error[native_ir.dynamic_callee]: `{callee_name}` is not an admitted closure value")
    })?;
    if signature.parameters.len() != args.len() {
        return Err(format!(
            "error[native_ir.dynamic_arity]: closure expects {} arguments but received {}",
            signature.parameters.len(),
            args.len()
        ));
    }
    let parameter_types = signature.parameters.clone();
    let result_type = signature.result;
    let declared_result = super::native_return_type(function).ok_or_else(|| {
        "error[native_ir.dynamic_result]: enclosing function has an unsupported result".to_string()
    })?;
    if declared_result != result_type {
        return Err(
            "error[native_ir.dynamic_result]: closure result does not match the enclosing function"
                .to_string(),
        );
    }
    let mut lowered_args = Vec::with_capacity(args.len());
    for (index, (argument, expected)) in args.iter().zip(&parameter_types).enumerate() {
        let actual = infer_native_type_with_constructors(
            argument,
            param_types,
            function_types,
            constructors,
        )
        .ok_or_else(|| {
            format!("error[native_ir.dynamic_argument]: cannot infer argument {index}")
        })?;
        if actual != *expected {
            return Err(format!(
                "error[native_ir.dynamic_argument]: argument {index} does not match its closure parameter"
            ));
        }
        lowered_args.push(lower_expr_with_constructors(
            argument,
            params,
            param_types,
            functions,
            function_types,
            constructors,
        )?);
    }
    let callee = lower_expr_with_constructors(
        callee,
        params,
        param_types,
        functions,
        function_types,
        constructors,
    )?;
    Ok(Some(NativeExpr::InvokeClosure {
        callee: Box::new(callee),
        args: lowered_args,
        parameter_types,
        result_type,
    }))
}

fn signature_from_type(ty: &CoreType) -> Result<Option<ClosureSignature>, String> {
    let CoreType::Arrow {
        params,
        return_type,
    } = ty
    else {
        return Ok(None);
    };
    let parameters = params
        .iter()
        .map(|ty| native_type(Some(ty), &ty.contract_text()))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| "error[native_ir.dynamic_signature]: unsupported parameters".to_string())?;
    let result = native_type(Some(return_type), &return_type.contract_text())
        .ok_or_else(|| "error[native_ir.dynamic_signature]: unsupported result".to_string())?;
    Ok(Some(ClosureSignature { parameters, result }))
}

fn closure_value_signature(
    value: &CoreExpr,
    signatures: &HashMap<String, ClosureSignature>,
    callable_shapes: &HashMap<(String, usize), NativeCallableShape>,
) -> Result<Option<ClosureSignature>, String> {
    match value {
        CoreExpr::Var(name) if signatures.contains_key(name) => Ok(signatures.get(name).cloned()),
        CoreExpr::RemoteFunRef {
            module,
            function,
            arity,
        } => callable_shapes
            .get(&(format!("{module}.{function}"), *arity))
            .map(callable_signature)
            .transpose(),
        CoreExpr::If { clauses } if !clauses.is_empty() => {
            let mut shapes = clauses
                .iter()
                .map(|clause| closure_value_signature(&clause.body, signatures, callable_shapes));
            let Some(first) = shapes.next().transpose()? else {
                return Ok(None);
            };
            let Some(first) = first else { return Ok(None) };
            for shape in shapes {
                if shape? != Some(first.clone()) {
                    return Err("error[native_ir.dynamic_branch_abi]: closure branches have incompatible signatures".to_string());
                }
            }
            Ok(Some(first))
        }
        _ => Ok(None),
    }
}

fn callable_signature(shape: &NativeCallableShape) -> Result<ClosureSignature, String> {
    Ok(ClosureSignature {
        parameters: shape.parameters.clone(),
        result: shape.result,
    })
}

fn closure_native_type(signature: &ClosureSignature) -> Result<NativeType, String> {
    let parameters = signature
        .parameters
        .iter()
        .copied()
        .map(NativeType::boundary_type)
        .collect::<Vec<_>>();
    let result = signature.result.boundary_type();
    ManagedClosureDescriptor::semantic_id_for_signature(&parameters, &[result])
        .map(NativeType::ManagedRef)
        .map_err(|error| format!("error[native_ir.dynamic_signature]: {error}"))
}

#[allow(clippy::too_many_arguments)]
fn lower_closure_value(
    value: &CoreExpr,
    signature: &ClosureSignature,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    signatures: &HashMap<String, ClosureSignature>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    callable_shapes: &HashMap<(String, usize), NativeCallableShape>,
    constructors: &NativeConstructorLayouts,
) -> Result<NativeExpr, String> {
    match value {
        CoreExpr::Var(name) if signatures.contains_key(name) => params
            .get(name)
            .copied()
            .map(NativeExpr::Param)
            .ok_or_else(|| format!("error[native_ir.dynamic_local]: `{name}` has no value slot")),
        CoreExpr::RemoteFunRef {
            module,
            function,
            arity,
        } => {
            let target = callable_shapes
                .get(&(format!("{module}.{function}"), *arity))
                .ok_or_else(|| {
                    "error[native_ir.dynamic_target]: callable target is absent".to_string()
                })?;
            if callable_signature(target)? != *signature {
                return Err(
                    "error[native_ir.dynamic_target_abi]: callable target signature differs"
                        .to_string(),
                );
            }
            Ok(NativeExpr::MakeClosure {
                encoded: Arc::from(
                    encode_closure_allocation(target.id)
                        .map_err(|error| format!("error[native_ir.closure_allocation]: {error}"))?,
                ),
                captures: vec![],
            })
        }
        CoreExpr::If { clauses } => Ok(NativeExpr::If {
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
                        lower_closure_value(
                            &clause.body,
                            signature,
                            params,
                            param_types,
                            signatures,
                            functions,
                            function_types,
                            callable_shapes,
                            constructors,
                        )?,
                    ))
                })
                .collect::<Result<Vec<_>, String>>()?,
        }),
        _ => Err("error[native_ir.dynamic_value]: unsupported closure-valued local".to_string()),
    }
}
