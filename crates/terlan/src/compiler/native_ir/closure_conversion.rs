//! Typed conversion of admitted named function values into owned closures.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::runtime::native_image::managed::encode_closure_allocation;
use crate::terlan_typeck::{
    CoreExpr, CoreFunction, CoreLetBinding, CoreParam, CorePattern, CoreType,
};

use super::{
    contains_process_yield, expr_calls_suspending, free_variables,
    infer_native_type_with_constructors, lower_expr_with_constructors, native_type,
    stable_export_id, NativeConstructorLayouts, NativeExpr, NativeFunction, NativeType,
};

/// Maximum number of immutable values owned by one generated closure.
const MAX_OWNED_CLOSURE_CAPTURES: usize = 64;

/// Maximum callable-producing clauses in one escaping closure branch.
const MAX_ESCAPING_CLOSURE_BRANCHES: usize = 64;

/// Maximum nested `Let`/`If` layers traversed while producing one closure.
const MAX_ESCAPING_CLOSURE_DEPTH: usize = 64;

/// Maximum lifted targets emitted by one closure-valued source function.
const MAX_ESCAPING_CLOSURE_TARGETS: usize = 64;

/// Lowers an escaping lambda after evaluating its scalar lexical prefix.
#[allow(clippy::too_many_arguments)]
pub(super) fn lower_escaping_closure(
    body: &CoreExpr,
    expected: Option<&CoreType>,
    available: &HashMap<String, usize>,
    available_types: &HashMap<String, NativeType>,
    identities: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
    suspending: &HashSet<(String, usize)>,
    callable_shapes: &HashMap<(String, usize), NativeCallableShape>,
    module: &str,
    owner_name: &str,
    owner_arity: usize,
) -> Result<Option<(NativeExpr, Vec<NativeFunction>)>, String> {
    let mut lifted_ordinal = 0;
    lower_escaping_closure_at(
        body,
        expected,
        available,
        available_types,
        identities,
        function_types,
        constructors,
        suspending,
        callable_shapes,
        module,
        owner_name,
        owner_arity,
        &mut lifted_ordinal,
        0,
    )
}

/// Recursively lowers one closure-valued result while assigning deterministic
/// identities to every branch-local lifted target.
#[allow(clippy::too_many_arguments)]
fn lower_escaping_closure_at(
    body: &CoreExpr,
    expected: Option<&CoreType>,
    available: &HashMap<String, usize>,
    available_types: &HashMap<String, NativeType>,
    identities: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
    suspending: &HashSet<(String, usize)>,
    callable_shapes: &HashMap<(String, usize), NativeCallableShape>,
    module: &str,
    owner_name: &str,
    owner_arity: usize,
    lifted_ordinal: &mut usize,
    depth: usize,
) -> Result<Option<(NativeExpr, Vec<NativeFunction>)>, String> {
    if depth > MAX_ESCAPING_CLOSURE_DEPTH {
        return Err(format!(
            "error[native_ir.closure_depth_limit]: escaping closure nesting exceeds {MAX_ESCAPING_CLOSURE_DEPTH} layers"
        ));
    }
    if !matches!(expected, Some(CoreType::Arrow { .. })) {
        return Ok(None);
    }
    if let Some(reference) =
        lower_escaping_function_reference(body, expected, available, callable_shapes)?
    {
        return Ok(Some((reference, Vec::new())));
    }
    if let Some(expected_native) =
        expected.and_then(|ty| native_type(Some(ty), &ty.contract_text()))
    {
        if !contains_process_yield(body)
            && !expr_calls_suspending(body, suspending)
            && infer_native_type_with_constructors(
                body,
                available_types,
                function_types,
                constructors,
            ) == Some(expected_native)
        {
            return lower_expr_with_constructors(
                body,
                available,
                available_types,
                identities,
                function_types,
                constructors,
            )
            .map(|body| Some((body, Vec::new())));
        }
    }
    let CoreExpr::Let {
        bindings,
        body: nested,
    } = body
    else {
        if let CoreExpr::If { clauses } = body {
            if clauses.is_empty() {
                return Err(
                    "error[native_ir.closure_branch]: escaping closure branch set is empty"
                        .to_string(),
                );
            }
            if clauses.len() > MAX_ESCAPING_CLOSURE_BRANCHES {
                return Err(format!(
                    "error[native_ir.closure_branch_limit]: escaping closure branch has {} clauses; limit is {MAX_ESCAPING_CLOSURE_BRANCHES}",
                    clauses.len()
                ));
            }
            let mut lowered_clauses = Vec::with_capacity(clauses.len());
            let mut lifted = Vec::new();
            for clause in clauses {
                if contains_process_yield(&clause.condition)
                    || expr_calls_suspending(&clause.condition, suspending)
                {
                    return Err(
                        "error[native_ir.closure_branch_suspension]: escaping closure branch condition cannot suspend"
                            .to_string(),
                    );
                }
                let condition = lower_expr_with_constructors(
                    &clause.condition,
                    available,
                    available_types,
                    identities,
                    function_types,
                    constructors,
                )?;
                let Some((branch, mut branch_lifted)) = lower_escaping_closure_at(
                    &clause.body,
                    expected,
                    available,
                    available_types,
                    identities,
                    function_types,
                    constructors,
                    suspending,
                    callable_shapes,
                    module,
                    owner_name,
                    owner_arity,
                    lifted_ordinal,
                    depth.saturating_add(1),
                )?
                else {
                    return Err(
                        "error[native_ir.closure_branch]: every escaping closure branch must produce a callable value"
                            .to_string(),
                    );
                };
                lowered_clauses.push((condition, branch));
                lifted.append(&mut branch_lifted);
            }
            return Ok(Some((
                NativeExpr::If {
                    clauses: lowered_clauses,
                },
                lifted,
            )));
        }
        return lower_escaping_lambda_at(
            body,
            expected,
            available,
            available_types,
            identities,
            function_types,
            constructors,
            suspending,
            module,
            owner_name,
            owner_arity,
            lifted_ordinal,
        )
        .map(|lowered| lowered.map(|(maker, lifted)| (maker, vec![lifted])));
    };
    let mut slots = available.clone();
    let mut types = available_types.clone();
    let mut next_slot = slots
        .values()
        .copied()
        .max()
        .map_or(0, |slot| slot.saturating_add(1));
    let mut lowered = Vec::with_capacity(bindings.len());
    for CoreLetBinding { pattern, value } in bindings {
        let CorePattern::Var(name) = pattern else {
            return Err(
                "error[native_ir.closure_prefix_pattern]: escaping closure prefix requires variable bindings"
                    .to_string(),
            );
        };
        if contains_process_yield(value) || expr_calls_suspending(value, suspending) {
            return Err(
                "error[native_ir.closure_prefix_suspension]: escaping closure prefix cannot suspend"
                    .to_string(),
            );
        }
        let ty = infer_native_type_with_constructors(value, &types, function_types, constructors)
            .ok_or_else(|| {
            format!("error[native_ir.closure_prefix_type]: cannot infer lexical binding `{name}`")
        })?;
        lowered.push(lower_expr_with_constructors(
            value,
            &slots,
            &types,
            identities,
            function_types,
            constructors,
        )?);
        slots.insert(name.clone(), next_slot);
        types.insert(name.clone(), ty);
        next_slot = next_slot.saturating_add(1);
    }
    let Some((maker, lifted)) = lower_escaping_closure_at(
        nested,
        expected,
        &slots,
        &types,
        identities,
        function_types,
        constructors,
        suspending,
        callable_shapes,
        module,
        owner_name,
        owner_arity,
        lifted_ordinal,
        depth.saturating_add(1),
    )?
    else {
        return Ok(None);
    };
    Ok(Some((
        if lowered.is_empty() {
            maker
        } else {
            NativeExpr::Let {
                bindings: lowered,
                body: Box::new(maker),
            }
        },
        lifted,
    )))
}

/// Closed-world native target shape used while lowering escaping references.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NativeCallableShape {
    pub(super) id: u64,
    pub(super) parameters: Vec<NativeType>,
    pub(super) result: NativeType,
}

/// Lowers one whole-result named function value into a zero-capture closure.
pub(super) fn lower_escaping_function_reference(
    body: &CoreExpr,
    expected: Option<&CoreType>,
    params: &HashMap<String, usize>,
    callable_shapes: &HashMap<(String, usize), NativeCallableShape>,
) -> Result<Option<NativeExpr>, String> {
    let Some(CoreType::Arrow {
        params: expected_params,
        return_type,
    }) = expected
    else {
        return Ok(None);
    };
    let expected_params = expected_params
        .iter()
        .map(|ty| native_type(Some(ty), &ty.contract_text()))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            "error[native_ir.closure_signature]: function value has unsupported parameters"
                .to_string()
        })?;
    let expected_result =
        native_type(Some(return_type), &return_type.contract_text()).ok_or_else(|| {
            "error[native_ir.closure_signature]: function value has unsupported result".to_string()
        })?;
    let arity = expected_params.len();
    let identity = match body {
        CoreExpr::RemoteFunRef {
            module,
            function,
            arity: declared_arity,
        } if *declared_arity == arity => (format!("{module}.{function}"), arity),
        CoreExpr::Var(name) if !params.contains_key(name) => (name.clone(), arity),
        _ => return Ok(None),
    };
    let target = callable_shapes.get(&identity).ok_or_else(|| {
        format!(
            "error[native_ir.function_value_target]: `{}/{}` is not an admitted native callable",
            identity.0, identity.1
        )
    })?;
    if target.parameters != expected_params || target.result != expected_result {
        return Err(format!(
            "error[native_ir.function_value_abi]: `{}/{}` does not match its declared closure signature",
            identity.0, identity.1
        ));
    }
    let encoded = encode_closure_allocation(target.id)
        .map_err(|error| format!("error[native_ir.closure_allocation]: {error}"))?;
    Ok(Some(NativeExpr::MakeClosure {
        encoded: Arc::from(encoded),
        captures: Vec::new(),
    }))
}

/// Closure-converts one lambda by lifting its body and snapshotting the typed
/// lexical values that it references.
#[allow(clippy::too_many_arguments)]
#[cfg(test)]
pub(super) fn lower_escaping_lambda(
    body: &CoreExpr,
    expected: Option<&CoreType>,
    available: &HashMap<String, usize>,
    available_types: &HashMap<String, NativeType>,
    identities: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
    suspending: &HashSet<(String, usize)>,
    module: &str,
    owner_name: &str,
    owner_arity: usize,
) -> Result<Option<(NativeExpr, NativeFunction)>, String> {
    let mut lifted_ordinal = 0;
    lower_escaping_lambda_at(
        body,
        expected,
        available,
        available_types,
        identities,
        function_types,
        constructors,
        suspending,
        module,
        owner_name,
        owner_arity,
        &mut lifted_ordinal,
    )
}

/// Lifts one lambda at the next deterministic identity within its owner.
#[allow(clippy::too_many_arguments)]
fn lower_escaping_lambda_at(
    body: &CoreExpr,
    expected: Option<&CoreType>,
    available: &HashMap<String, usize>,
    available_types: &HashMap<String, NativeType>,
    identities: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
    suspending: &HashSet<(String, usize)>,
    module: &str,
    owner_name: &str,
    owner_arity: usize,
    lifted_ordinal: &mut usize,
) -> Result<Option<(NativeExpr, NativeFunction)>, String> {
    let (
        CoreExpr::Lam {
            params: lambda_patterns,
            body: lambda_body,
        },
        Some(CoreType::Arrow {
            params: expected_params,
            return_type,
        }),
    ) = (body, expected)
    else {
        return Ok(None);
    };
    if *lifted_ordinal >= MAX_ESCAPING_CLOSURE_TARGETS {
        return Err(format!(
            "error[native_ir.closure_target_limit]: escaping closure emits more than {MAX_ESCAPING_CLOSURE_TARGETS} lifted targets"
        ));
    }
    if lambda_patterns.len() != expected_params.len() {
        return Err(format!(
            "error[native_ir.closure_arity]: escaping lambda declares {} parameters but its type requires {}",
            lambda_patterns.len(),
            expected_params.len()
        ));
    }
    let lambda_names = lambda_patterns
        .iter()
        .map(|pattern| match pattern {
            CorePattern::Var(name) => Ok(name.clone()),
            _ => Err(
                "error[native_ir.closure_parameter]: escaping lambda parameters must be variables"
                    .to_string(),
            ),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let lambda_types = expected_params
        .iter()
        .map(|ty| native_type(Some(ty), &ty.contract_text()))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| {
            "error[native_ir.closure_signature]: escaping lambda has unsupported parameters"
                .to_string()
        })?;
    let result_type =
        native_type(Some(return_type), &return_type.contract_text()).ok_or_else(|| {
            "error[native_ir.closure_signature]: escaping lambda has an unsupported result"
                .to_string()
        })?;
    let suspending_tail = matches!(
        lambda_body.as_ref(),
        CoreExpr::Call { function, args }
            if suspending.contains(&(function.clone(), args.len()))
    );
    if contains_process_yield(lambda_body)
        || (expr_calls_suspending(lambda_body, suspending) && !suspending_tail)
    {
        return Err(
            "error[native_ir.closure_suspension]: escaping lambda requires suspending indirect-call lowering"
                .to_string(),
        );
    }

    let lambda_bound = lambda_names.iter().cloned().collect::<HashSet<_>>();
    let mut capture_names = free_variables(lambda_body)
        .into_iter()
        .filter(|name| !lambda_bound.contains(name))
        .collect::<Vec<_>>();
    capture_names.sort();
    if capture_names.len() > MAX_OWNED_CLOSURE_CAPTURES {
        return Err(format!(
            "error[native_ir.closure_capture_limit]: escaping lambda captures {} values; limit is {MAX_OWNED_CLOSURE_CAPTURES}",
            capture_names.len()
        ));
    }
    let capture_types = capture_names
        .iter()
        .map(|name| {
            available_types.get(name).copied().ok_or_else(|| {
                format!("error[native_ir.closure_capture]: `{name}` is not a typed lexical value")
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let capture_values = capture_names
        .iter()
        .map(|name| {
            available
                .get(name)
                .copied()
                .map(NativeExpr::Param)
                .ok_or_else(|| {
                    format!("error[native_ir.closure_capture]: `{name}` has no lexical value slot")
                })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut lifted_names = capture_names;
    lifted_names.extend(lambda_names.iter().cloned());
    let mut lifted_types = capture_types.clone();
    lifted_types.extend(lambda_types);
    let lifted_params = lifted_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), index))
        .collect::<HashMap<_, _>>();
    let lifted_param_types = lifted_names
        .iter()
        .cloned()
        .zip(lifted_types.iter().copied())
        .collect::<HashMap<_, _>>();
    let inferred_result = infer_native_type_with_constructors(
        lambda_body,
        &lifted_param_types,
        function_types,
        constructors,
    );
    if inferred_result.is_some_and(|inferred| inferred != result_type) {
        return Err(
            "error[native_ir.closure_result]: escaping lambda body does not match its declared result"
                .to_string(),
        );
    }
    if inferred_result.is_none() && !super::expr_is_native_control(lambda_body) {
        return Err(
            "error[native_ir.closure_result]: cannot infer escaping lambda result".to_string(),
        );
    }
    let closure_contract = CoreFunction {
        name: format!("$closure_contract_{owner_name}_{owner_arity}"),
        arity: lambda_names.len(),
        public: false,
        generic_params: Vec::new(),
        native_operation: None,
        params: lambda_names
            .iter()
            .cloned()
            .zip(expected_params.iter().cloned())
            .map(|(name, ty)| CoreParam {
                name,
                ty: ty.contract_text(),
                core_ty: Some(ty),
            })
            .collect(),
        return_type: return_type.contract_text(),
        core_return_type: Some(return_type.as_ref().clone()),
        clauses: Vec::new(),
    };
    let structured = super::structured_case::lower_structured_case(
        lambda_body,
        &closure_contract,
        &lifted_params,
        &lifted_param_types,
        identities,
        function_types,
        &HashMap::new(),
        constructors,
    )?;
    let lifted_body = if let Some(body) = structured {
        body
    } else if let CoreExpr::Call { function, args } = lambda_body.as_ref() {
        if suspending_tail {
            let target = identities
                .get(&(function.clone(), args.len()))
                .copied()
                .ok_or_else(|| {
                    format!(
                        "error[native_ir.closure_tail_target]: `{function}/{}` is absent",
                        args.len()
                    )
                })?;
            NativeExpr::TailCall {
                function: target,
                args: args
                    .iter()
                    .map(|argument| {
                        lower_expr_with_constructors(
                            argument,
                            &lifted_params,
                            &lifted_param_types,
                            identities,
                            function_types,
                            constructors,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            }
        } else {
            lower_expr_with_constructors(
                lambda_body,
                &lifted_params,
                &lifted_param_types,
                identities,
                function_types,
                constructors,
            )?
        }
    } else {
        lower_expr_with_constructors(
            lambda_body,
            &lifted_params,
            &lifted_param_types,
            identities,
            function_types,
            constructors,
        )?
    };
    let ordinal = *lifted_ordinal;
    *lifted_ordinal = ordinal.saturating_add(1);
    let lifted_name = format!("$closure_{owner_name}_{owner_arity}_{ordinal}");
    let lifted_id = stable_export_id(module, &lifted_name, lifted_types.len());
    let encoded = encode_closure_allocation(lifted_id)
        .map_err(|error| format!("error[native_ir.closure_allocation]: {error}"))?;
    Ok(Some((
        NativeExpr::MakeClosure {
            encoded: Arc::from(encoded),
            captures: capture_values,
        },
        NativeFunction {
            export_id: lifted_id,
            name: lifted_name,
            public: false,
            arity: lifted_types.len(),
            source_module: module.to_string(),
            source_function: owner_name.to_string(),
            source_arity: owner_arity,
            callable_captures: capture_types,
            params: lifted_types,
            return_type: result_type,
            body: lifted_body,
        },
    )))
}
