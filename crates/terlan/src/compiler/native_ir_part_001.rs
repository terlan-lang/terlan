use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::terlan_typeck::{CoreExpr, CoreFunction, CoreLetBinding, CorePattern, CoreType};

pub(crate) use codegen_policy::NativeCodegenPolicy;
pub(crate) use model::{NativeBinaryOperator, NativeExpr, NativeTransitionOperation, NativeType};
pub(crate) use request_projection::{
    install_native_request_projection_exports, native_request_projections, NativeRequestProjection,
};
pub(crate) use crate::runtime::native_image::{
    TVM_DISPATCH_SYMBOL_V2 as DISPATCH_SYMBOL, TVM_IMAGE_ENTRY_SYMBOL_V1 as IMAGE_ENTRY_SYMBOL,
};
use application_calls::{eager_argument_yield, expr_calls_are_local, expr_calls_suspending};
use call_composition::{
    composed_call_region, is_composable_suspending_body, rebase_callee_locals,
    suspending_call_count, CallRegion, ComposedCallProfile,
};
use closure_conversion::{
    lower_escaping_closure, lower_escaping_function_reference, NativeCallableShape,
};
use constructors::NativeConstructorLayouts;
#[cfg(test)]
pub(crate) use cranelift::emit_native_application_object;
pub(crate) use cranelift::{
    emit_native_application_dispatch_object_with_policy,
    emit_native_application_object_with_policy, emit_native_module_object_with_policy,
    native_application_abi_fingerprint,
};
use expression::{
    expr_is_scalar, free_variables, infer_native_type, infer_native_type_with_constructors,
    lower_expr_with_constructors, native_type,
};
use identity::{stable_continuation_id, stable_export_id};
use transitions::{is_process_transition, process_transition};

/// ABI implemented by the first direct-AOT scalar slice.
pub(crate) const NATIVE_ABI_VERSION: &str = "terlan-native-v2";

/// Bounds source-to-native expansion while non-linear conditions are still
/// represented as nested control regions rather than a shared control graph.
const MAX_NATIVE_CONDITION_COMPOSITION_DEPTH: usize = 1_024;
const MAX_NATIVE_CALL_COMPOSITION_DEPTH: usize = 1_024;

/// Stable native status returned across the direct-AOT boundary.
pub(crate) mod status {
    pub(crate) const OK: i32 = 0;
    pub(crate) const UNKNOWN_EXPORT: i32 = 1;
    pub(crate) const ARITY: i32 = 2;
    pub(crate) const OVERFLOW: i32 = 3;
    pub(crate) const DIVISION_BY_ZERO: i32 = 4;
    pub(crate) const NO_MATCHING_BRANCH: i32 = 5;
    pub(crate) const YIELD: i32 = 6;
    pub(crate) const TRANSITION_CAPACITY: i32 = 7;
    pub(crate) const SEND: i32 = 8;
    pub(crate) const RECEIVE: i32 = 9;
    pub(crate) const SPAWN: i32 = 10;
    pub(crate) const TIMER: i32 = 11;
    pub(crate) const LINK: i32 = 12;
    pub(crate) const MONITOR: i32 = 13;
    pub(crate) const RESOURCE: i32 = 14;
    pub(crate) const CANCELLATION: i32 = 15;
    pub(crate) const FAILURE: i32 = 16;
    pub(crate) const SCHEDULING: i32 = 17;
    pub(crate) const FLOAT_OVERFLOW: i32 = 18;
    pub(crate) const FLOAT_DIVISION_BY_ZERO: i32 = 19;
    pub(crate) const MANAGED_RUNTIME_UNAVAILABLE: i32 = 20;
    pub(crate) const INVALID_MANAGED_REFERENCE: i32 = 21;
    pub(crate) const SEND_TYPED: i32 = 22;
    pub(crate) const RECEIVE_TYPED: i32 = 23;
    pub(crate) const CAPABILITY: i32 = 24;
}

/// One compiler-owned native module before backend lowering.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NativeModule {
    /// Checked module identity used by symbol and descriptor generation.
    pub(crate) name: String,
    /// Public and private functions admitted to native lowering.
    pub(crate) functions: Vec<NativeFunction>,
    /// Generated resume entries owned by this module.
    pub(crate) continuations: Vec<NativeContinuation>,
    /// Canonical fixed aggregate layouts visible to this checked module.
    pub(crate) managed_layouts: Vec<Arc<[u8]>>,
    /// Canonical List, Map, and Set schemas visible to this checked module.
    pub(crate) managed_collections: Vec<Arc<[u8]>>,
    /// Canonically ordered finite atom identities visible to this checked module.
    pub(crate) atoms: Vec<String>,
}

/// One compiler-generated resume entry with pointer-free stable identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NativeContinuation {
    pub(crate) id: u64,
    /// Checked source declaration that caused this generated resume entry.
    pub(crate) source_module: String,
    pub(crate) source_function: String,
    pub(crate) source_arity: usize,
    pub(crate) params: Vec<NativeType>,
    pub(crate) return_type: NativeType,
    pub(crate) body: NativeExpr,
}

/// One native function and its stable dispatch id.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NativeFunction {
    pub(crate) export_id: u64,
    pub(crate) name: String,
    pub(crate) public: bool,
    pub(crate) arity: usize,
    /// Checked source declaration represented by this native function.
    ///
    /// Generated closures, projections, and continuation bodies retain their
    /// generated runtime identity while pointing debug spans at their owner.
    pub(crate) source_module: String,
    pub(crate) source_function: String,
    pub(crate) source_arity: usize,
    /// Leading lifted parameters populated from a closure's owned captures.
    pub(crate) callable_captures: Vec<NativeType>,
    pub(crate) params: Vec<NativeType>,
    pub(crate) return_type: NativeType,
    pub(crate) body: NativeExpr,
}

#[allow(clippy::too_many_arguments)]
fn lower_native_function_with_callables(
    module: &str,
    function: &CoreFunction,
    identities: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    function_core_types: &HashMap<(String, usize), CoreType>,
    callable_shapes: &HashMap<(String, usize), NativeCallableShape>,
    lifted_functions: &mut Vec<NativeFunction>,
    constructors: &NativeConstructorLayouts,
    suspending_functions: &HashSet<(String, usize)>,
    call_profiles: &HashMap<usize, ComposedCallProfile>,
    stable_ids: &mut HashSet<u64>,
) -> Result<(NativeFunction, Vec<NativeContinuation>), String> {
    let clause = function
        .clauses
        .first()
        .ok_or_else(|| native_error(function, "has no direct-binding clause"))?;
    let params = function
        .params
        .iter()
        .enumerate()
        .map(|(index, param)| (param.name.clone(), index))
        .collect::<HashMap<_, _>>();
    let core_body = clause
        .body
        .core_expr
        .as_ref()
        .ok_or_else(|| native_error(function, "has no typed CoreIR body"))?;
    let core_body = scalar_replacement::scalar_replace_fixed_aggregates(core_body, constructors);
    let native_params = function
        .params
        .iter()
        .map(|param| {
            native_type_with_constructors(param.core_ty.as_ref(), &param.ty, constructors)
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| native_error(function, "has an unsupported parameter type"))?;
    let param_names = function
        .params
        .iter()
        .map(|param| param.name.clone())
        .collect::<Vec<_>>();
    let param_types = param_names
        .iter()
        .cloned()
        .zip(native_params.iter().copied())
        .collect::<HashMap<_, _>>();
    let return_type = native_return_type_with_constructors(function, constructors)
        .ok_or_else(|| native_error(function, "has an unsupported return type"))?;
    let inferred_dynamic_return = dynamic_return::inferred_dynamic_return_type(function);
    let boundary_return_type = function
        .core_return_type
        .as_ref()
        .filter(|ty| !matches!(ty, crate::terlan_typeck::CoreType::Dynamic))
        .or(inferred_dynamic_return.as_ref());
    let collection_value = collection_values::lower_boundary_collection_value(
        &core_body,
        boundary_return_type,
        &params,
        &param_types,
        identities,
        function_types,
        constructors,
    )?;
    let structured_case = structured_case::lower_structured_case(
        &core_body,
        function,
        &params,
        &param_types,
        identities,
        function_types,
        function_core_types,
        constructors,
    )?;
    let try_expr = try_lowering::lower_try(
        &core_body,
        function,
        &params,
        &param_types,
        identities,
        function_types,
        constructors,
    )?;
    let indirect_invocation = closure_invocation::lower_boundary_closure_invocation(
        &core_body,
        function,
        &params,
        &param_types,
        identities,
        function_types,
        callable_shapes,
        constructors,
    )?;
    let escaping_reference = lower_escaping_function_reference(
        &core_body,
        function.core_return_type.as_ref(),
        &params,
        callable_shapes,
    )?;
    let escaping_lambda = lower_escaping_closure(
        &core_body,
        function.core_return_type.as_ref(),
        &params,
        &param_types,
        identities,
        function_types,
        constructors,
        suspending_functions,
        callable_shapes,
        module,
        &function.name,
        function.arity,
    )?;
    let (body, continuations) = if let Some(body) = collection_value {
        (body, Vec::new())
    } else if let Some(body) = structured_case {
        (body, Vec::new())
    } else if let Some(body) = try_expr {
        (body, Vec::new())
    } else if let Some(body) = indirect_invocation {
        (body, Vec::new())
    } else if let Some(body) = escaping_reference {
        (body, Vec::new())
    } else if let Some((body, lifted)) = escaping_lambda {
        for function in &lifted {
            if !stable_ids.insert(function.export_id) {
                return Err(format!(
                    "error[native_ir.export_id_collision]: lifted callable identity {} collides in module `{module}`",
                    function.export_id
                ));
            }
        }
        lifted_functions.extend(lifted);
        (body, Vec::new())
    } else {
        let mut continuation_ordinal = 0;
        control::lower_expr_with_yields(
            &core_body,
            &param_names,
            &params,
            &param_types,
            identities,
            function_types,
            constructors,
            suspending_functions,
            call_profiles,
            module,
            &function.name,
            function.arity,
            return_type,
            &mut continuation_ordinal,
            stable_ids,
        )
        .map_err(|error| {
            format!(
                "{error}; while lowering `{module}.{}/{}`",
                function.name, function.arity
            )
        })?
    };
    let export_id = stable_export_id(module, &function.name, function.arity);
    if !stable_ids.insert(export_id) {
        return Err(format!(
            "error[native_ir.export_id_collision]: stable export id {export_id} collides in module `{module}`"
        ));
    }
    Ok((
        NativeFunction {
            export_id,
            name: function.name.clone(),
            public: function.public,
            arity: function.arity,
            source_module: module.to_string(),
            source_function: function.name.clone(),
            source_arity: function.arity,
            callable_captures: Vec::new(),
            params: native_params,
            return_type,
            body,
        },
        continuations,
    ))
}

fn contains_process_yield(expr: &CoreExpr) -> bool {
    if is_process_transition(expr) {
        return true;
    }
    match expr {
        CoreExpr::Call { args, .. }
        | CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            args.iter().any(contains_process_yield)
        }
        CoreExpr::RecordConstruct { fields, .. } => fields
            .iter()
            .any(|field| contains_process_yield(&field.value)),
        CoreExpr::RecordUpdate { base, fields, .. } => {
            contains_process_yield(base)
                || fields
                    .iter()
                    .any(|field| contains_process_yield(&field.value))
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            contains_process_yield(base)
        }
        CoreExpr::UnaryOp { operand, .. } => contains_process_yield(operand),
        CoreExpr::BinaryOp { left, right, .. } => {
            contains_process_yield(left) || contains_process_yield(right)
        }
        CoreExpr::Let { bindings, body } => {
            bindings
                .iter()
                .any(|binding| contains_process_yield(&binding.value))
                || contains_process_yield(body)
        }
        CoreExpr::If { clauses } => clauses.iter().any(|clause| {
            contains_process_yield(&clause.condition) || contains_process_yield(&clause.body)
        }),
        _ => false,
    }
}

fn is_scalar_candidate(function: &CoreFunction, constructors: &NativeConstructorLayouts) -> bool {
    function.native_operation.is_none()
        && function
            .params
            .iter()
            .all(|param| native_type(param.core_ty.as_ref(), &param.ty).is_some())
        && native_return_type(function).is_some()
        && matches!(function.clauses.as_slice(), [clause]
        if clause.guard.is_none()
            && clause.core_patterns.len() == function.params.len()
            && clause.core_patterns.iter().zip(&function.params).all(|(pattern, param)| {
                matches!(pattern, Some(CorePattern::Var(name)) if name == &param.name)
            })
            && clause
                .body
                .core_expr
                .as_ref()
                .map(|body| scalar_replacement::scalar_replace_fixed_aggregates(body, constructors))
                .is_some_and(|body| expr_is_native_control(&body)))
}

/// Resolves the scalar ABI result for a native candidate.
///
/// Ordinary functions keep their checked declared type. A `Dynamic` wrapper,
/// such as the synthetic REPL entry, may narrow only when its typed CoreIR body
/// is independently scalar and does not need another function's result type.
/// This preserves the source-facing dynamic contract while allowing the AOT
/// boundary to carry the concrete `Unit`, `Int`, `Float`, or `Bool` value it
/// proved.
fn native_return_type(function: &CoreFunction) -> Option<NativeType> {
    native_type(function.core_return_type.as_ref(), &function.return_type)
        .or_else(|| {
            let inferred = dynamic_return::inferred_dynamic_return_type(function)?;
            native_type(Some(&inferred), &inferred.contract_text())
        })
        .or_else(|| {
            let variables = function
                .params
                .iter()
                .map(|param| {
                    native_type(param.core_ty.as_ref(), &param.ty)
                        .map(|ty| (param.name.clone(), ty))
                })
                .collect::<Option<HashMap<_, _>>>()?;
            let body = function.clauses.first()?.body.core_expr.as_ref()?;
            infer_native_type(body, &variables, &HashMap::new())
        })
}

fn native_return_type_with_constructors(
    function: &CoreFunction,
    constructors: &NativeConstructorLayouts,
) -> Option<NativeType> {
    native_type_with_constructors(
        function.core_return_type.as_ref(),
        &function.return_type,
        constructors,
    )
    .or_else(|| native_return_type(function))
}

fn native_type_with_constructors(
    core: Option<&crate::terlan_typeck::CoreType>,
    text: &str,
    constructors: &NativeConstructorLayouts,
) -> Option<NativeType> {
    if let Some(crate::terlan_typeck::CoreType::Named(name)) = core {
        let mut matching = constructors
            .iter()
            .filter(|((identity, _), _)| identity == name)
            .map(|(_, layout)| layout.result);
        if let Some(result) = matching.next() {
            if matching.all(|candidate| candidate == result) {
                return Some(result);
            }
        }
    }
    native_type(core, text)
}

fn expr_is_native_control(expr: &CoreExpr) -> bool {
    if expr_is_scalar(expr) {
        return true;
    }
    if condition_yield_region(expr).is_some_and(|region| {
        region.prefix.iter().all(|binding| {
            matches!(binding.pattern, CorePattern::Var(_)) && expr_is_scalar(&binding.value)
        }) && expr_is_native_control(&region.resume)
    }) {
        return true;
    }
    match expr {
        CoreExpr::RemoteFunRef { .. } | CoreExpr::Lam { .. } => true,
        CoreExpr::Let { bindings, body } => {
            !bindings.is_empty()
                && bindings.iter().all(|binding| {
                    matches!(binding.pattern, CorePattern::Var(_))
                        && (expr_is_scalar(&binding.value)
                            || (structured_case::contains_case(&binding.value)
                                && expr_is_native_control(&binding.value)))
                })
                && expr_is_native_control(body)
        }
        CoreExpr::If { clauses } => {
            !clauses.is_empty()
                && clauses.iter().all(|clause| {
                    expr_is_native_clause_condition(&clause.condition, &clause.body)
                        && expr_is_native_control(&clause.body)
                })
        }
        CoreExpr::Case { scrutinee, clauses } => {
            !clauses.is_empty()
                && expr_is_scalar(scrutinee)
                && clauses.iter().all(|clause| {
                    clause.guard.as_ref().is_none_or(expr_is_native_condition)
                        && expr_is_native_control(&clause.body)
                })
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            !contains_process_yield(body)
                && of_clauses.iter().chain(catch_clauses).all(|clause| {
                    clause.guard.as_ref().is_none_or(expr_is_scalar) && expr_is_scalar(&clause.body)
                })
                && after_clause.as_ref().is_none_or(|after| {
                    expr_is_scalar(&after.trigger) && expr_is_scalar(&after.body)
                })
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } if matches!(operator.as_str(), "and" | "&&" | "or" | "||") => {
            expr_is_native_condition(left) && expr_is_native_control(right)
        }
        _ => false,
    }
}

/// Admits scalar condition suspension only when lowering can preserve source
/// evaluation order and bound the composed native control expansion.
fn expr_is_native_condition(expr: &CoreExpr) -> bool {
    expr_is_native_condition_at_depth(expr, 0)
}

fn expr_is_native_condition_at_depth(expr: &CoreExpr, depth: usize) -> bool {
    if expr_is_scalar(expr) {
        return true;
    }
    if condition_yield_region(expr).is_some_and(|region| {
        region.prefix.iter().all(|binding| {
            matches!(binding.pattern, CorePattern::Var(_)) && expr_is_scalar(&binding.value)
        }) && expr_is_native_condition_at_depth(&region.resume, depth)
    }) {
        return true;
    }
    matches!(
        expr,
        CoreExpr::BinaryOp { operator, left, right }
            if matches!(operator.as_str(), "and" | "&&" | "or" | "||")
                && depth < MAX_NATIVE_CONDITION_COMPOSITION_DEPTH
                && expr_is_scalar(left)
                && expr_is_native_condition_at_depth(right, depth + 1)
    )
}

fn expr_is_native_clause_condition(condition: &CoreExpr, body: &CoreExpr) -> bool {
    if expr_is_scalar(condition) {
        return true;
    }
    if let CoreExpr::BinaryOp {
        operator,
        left,
        right,
    } = condition
    {
        if matches!(operator.as_str(), "or" | "||")
            && expr_is_scalar(left)
            && expr_is_native_condition(right)
        {
            return expr_is_native_control(body);
        }
    }
    expr_is_native_condition(condition)
}

struct YieldRegion {
    prefix: Vec<CoreLetBinding>,
    operation: NativeTransitionOperation,
    arguments: Vec<CoreExpr>,
    result: Option<(String, NativeType)>,
    resume: CoreExpr,
}

struct LoweredYield {
    entry: NativeExpr,
    continuation_params: Vec<NativeType>,
    resume: CoreExpr,
    resume_names: Vec<String>,
    resume_vars: HashMap<String, usize>,
    resume_types: HashMap<String, NativeType>,
}

fn yield_region(expr: &CoreExpr) -> Option<YieldRegion> {
    if let Some((operation, arguments, result_type)) = process_transition(expr) {
        let result = result_type.map(|ty| ("$native_transition_result".to_string(), ty));
        let resume = result.as_ref().map_or_else(
            || CoreExpr::Atom("Unit".to_string()),
            |(name, _)| CoreExpr::Var(name.clone()),
        );
        return Some(YieldRegion {
            prefix: Vec::new(),
            operation,
            arguments,
            result,
            resume,
        });
    }
    let mut prefix = Vec::new();
    let mut current = expr;
    loop {
        let CoreExpr::Let { bindings, body } = current else {
            return None;
        };
        for (index, binding) in bindings.iter().enumerate() {
            if let Some((operation, arguments, result_type)) = process_transition(&binding.value) {
                let result = result_type.and_then(|ty| match &binding.pattern {
                    CorePattern::Var(name) => Some((name.clone(), ty)),
                    _ => None,
                });
                if result_type.is_some() && result.is_none() {
                    return None;
                }
                let remaining = bindings[index + 1..].to_vec();
                let resume = if remaining.is_empty() {
                    body.as_ref().clone()
                } else {
                    CoreExpr::Let {
                        bindings: remaining,
                        body: body.clone(),
                    }
                };
                return Some(YieldRegion {
                    prefix,
                    operation,
                    arguments,
                    result,
                    resume,
                });
            }
            prefix.push(binding.clone());
        }
        current = body;
    }
}

/// Extracts a yield only when moving it to the condition boundary cannot move
/// an earlier scalar computation across the suspension point.
fn condition_yield_region(expr: &CoreExpr) -> Option<YieldRegion> {
    condition_yield_region_at_depth(expr, 0)
}

fn condition_yield_region_at_depth(expr: &CoreExpr, depth: usize) -> Option<YieldRegion> {
    if let Some(region) = yield_region(expr) {
        return Some(region);
    }
    match expr {
        CoreExpr::Call { function, args } if !args.is_empty() => {
            let (region, args) = eager_argument_yield(args, depth)?;
            Some(YieldRegion {
                resume: CoreExpr::Call {
                    function: function.clone(),
                    args,
                },
                ..region
            })
        }
        CoreExpr::ConstructorCall {
            constructor,
            constructor_identity,
            args,
        } if !args.is_empty() => {
            let (region, args) = eager_argument_yield(args, depth)?;
            Some(YieldRegion {
                resume: CoreExpr::ConstructorCall {
                    constructor: constructor.clone(),
                    constructor_identity: constructor_identity.clone(),
                    args,
                },
                ..region
            })
        }
        CoreExpr::UnaryOp { operator, operand } => {
            condition_yield_region_at_depth(operand, depth.saturating_add(1)).map(|region| {
                YieldRegion {
                    prefix: region.prefix,
                    operation: region.operation,
                    arguments: region.arguments,
                    result: region.result,
                    resume: CoreExpr::UnaryOp {
                        operator: operator.clone(),
                        operand: Box::new(region.resume),
                    },
                }
            })
        }
        CoreExpr::BinaryOp {
            operator,
            left,
            right,
        } => {
            if let Some(region) = condition_yield_region_at_depth(left, depth.saturating_add(1)) {
                return Some(YieldRegion {
                    prefix: region.prefix,
                    operation: region.operation,
                    arguments: region.arguments,
                    result: region.result,
                    resume: CoreExpr::BinaryOp {
                        operator: operator.clone(),
                        left: Box::new(region.resume),
                        right: right.clone(),
                    },
                });
            }
            if matches!(operator.as_str(), "and" | "&&" | "or" | "||") || !expr_is_scalar(left) {
                return None;
            }
            let mut region = condition_yield_region_at_depth(right, depth.saturating_add(1))?;
            let left_name = format!("$native_eager_left_{depth}");
            region.prefix.insert(
                0,
                CoreLetBinding {
                    pattern: CorePattern::Var(left_name.clone()),
                    value: left.as_ref().clone(),
                },
            );
            region.resume = CoreExpr::BinaryOp {
                operator: operator.clone(),
                left: Box::new(CoreExpr::Var(left_name)),
                right: Box::new(region.resume),
            };
            Some(region)
        }
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn lower_yield_region(
    region: &YieldRegion,
    param_names: &[String],
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    functions: &HashMap<(String, usize), usize>,
    function_types: &HashMap<(String, usize), NativeType>,
    constructors: &NativeConstructorLayouts,
    continuation_id: u64,
) -> Result<LoweredYield, String> {
    let mut available_names = param_names.iter().cloned().collect::<HashSet<_>>();
    let mut prefix_names = Vec::with_capacity(region.prefix.len());
    for binding in &region.prefix {
        let CorePattern::Var(name) = &binding.pattern else {
            return Err(
                "error[native_ir.yield_pattern]: yield prefix requires scalar variable bindings"
                    .to_string(),
            );
        };
        if !available_names.insert(name.clone()) {
            return Err(format!(
                "error[native_ir.yield_shadow]: captured variable `{name}` shadows an existing scalar"
            ));
        }
        prefix_names.push(name.clone());
    }

    let mut capture_set = free_variables(&region.resume);
    if let Some((name, _)) = &region.result {
        capture_set.remove(name);
    }
    let mut needed = capture_set.clone();
    for argument in &region.arguments {
        needed.extend(free_variables(argument));
    }
    let mut selected = vec![false; region.prefix.len()];
    for (index, binding) in region.prefix.iter().enumerate().rev() {
        let name = &prefix_names[index];
        if needed.contains(name) {
            selected[index] = true;
            needed.extend(free_variables(&binding.value));
        }
    }

    let mut entry_vars = params.clone();
    let mut entry_types = param_types.clone();
    let mut entry_bindings = Vec::new();
    for (index, binding) in region.prefix.iter().enumerate() {
        if !selected[index] {
            continue;
        }
        let value_type = infer_native_type_with_constructors(
            &binding.value,
            &entry_types,
            function_types,
            constructors,
        )
        .ok_or_else(|| {
            format!(
                "error[native_ir.yield_type]: cannot infer captured scalar `{}`",
                prefix_names[index]
            )
        })?;
        let value = lower_expr_with_constructors(
            &binding.value,
            &entry_vars,
            &entry_types,
            functions,
            function_types,
            constructors,
        )?;
        entry_bindings.push(value);
        let value_index = params.len() + entry_bindings.len() - 1;
        entry_vars.insert(prefix_names[index].clone(), value_index);
        entry_types.insert(prefix_names[index].clone(), value_type);
    }

    let capture_names = param_names
        .iter()
        .chain(prefix_names.iter())
        .filter(|name| capture_set.contains(*name))
        .cloned()
        .collect::<Vec<_>>();
    if let Some(unknown) = capture_set
        .iter()
        .find(|name| !available_names.contains(*name))
    {
        return Err(format!(
            "error[native_ir.yield_capture]: resume references unavailable scalar `{unknown}`"
        ));
    }
    let values = capture_names
        .iter()
        .map(|name| {
            entry_vars
                .get(name)
                .copied()
                .map(NativeExpr::Param)
                .ok_or_else(|| {
                    format!("error[native_ir.yield_capture]: scalar `{name}` was not materialized")
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut continuation_params = region.result.iter().map(|(_, ty)| *ty).collect::<Vec<_>>();
    continuation_params.extend(
        capture_names
            .iter()
            .map(|name| {
                entry_types.get(name).copied().ok_or_else(|| {
                    format!("error[native_ir.yield_type]: scalar `{name}` has no native type")
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
    );
    let result_offset = usize::from(region.result.is_some());
    let mut resume_vars = capture_names
        .iter()
        .enumerate()
        .map(|(index, name)| (name.clone(), index + result_offset))
        .collect::<HashMap<_, _>>();
    if let Some((name, _)) = &region.result {
        resume_vars.insert(name.clone(), 0);
    }
    let operation_arguments = region
        .arguments
        .iter()
        .map(|argument| {
            lower_expr_with_constructors(
                argument,
                &entry_vars,
                &entry_types,
                functions,
                function_types,
                constructors,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let suspend = NativeExpr::Suspend {
        operation: region.operation,
        arguments: operation_arguments,
        continuation_id,
        values,
    };
    let mut resume_types = capture_names
        .iter()
        .filter_map(|name| entry_types.get(name).copied().map(|ty| (name.clone(), ty)))
        .collect::<HashMap<_, _>>();
    if let Some((name, ty)) = &region.result {
        resume_types.insert(name.clone(), *ty);
    }
    let mut resume_names = region
        .result
        .iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    resume_names.extend(capture_names);
    Ok(LoweredYield {
        entry: if entry_bindings.is_empty() {
            suspend
        } else {
            NativeExpr::Let {
                bindings: entry_bindings,
                body: Box::new(suspend),
            }
        },
        continuation_params,
        resume: region.resume.clone(),
        resume_names,
        resume_vars,
        resume_types,
    })
}

fn expr_calls_are_supported(
    expr: &CoreExpr,
    identities: &[(&str, usize)],
    suspending: &HashSet<(String, usize)>,
    composable: &HashSet<(String, usize)>,
    tail_position: bool,
) -> bool {
    if suspending_call_count(expr, suspending) > MAX_NATIVE_CALL_COMPOSITION_DEPTH {
        return false;
    }
    let is_composable =
        |function: &str, arity: usize| composable.contains(&(function.to_string(), arity));
    if let Some(region) = composed_call_region(expr, suspending, &is_composable, &HashSet::new()) {
        return expr_is_native_control(&region.resume)
            && expr_calls_are_supported(&region.resume, identities, suspending, composable, false)
            && region.gates.iter().all(|gate| {
                expr_is_scalar(&gate.condition)
                    && gate.prefix.iter().all(|binding| {
                        !expr_calls_suspending(&binding.value, suspending)
                            && expr_calls_are_local(&binding.value, identities)
                    })
                    && expr_is_native_control(&gate.bypass_resume)
                    // A gate bypass is the same surrounding continuation as
                    // `region.resume`, with only the short-circuit result
                    // substituted. Recursing into every bypass revalidates
                    // the shared suffix once per boolean term and makes
                    // admission exponential for long assertion pipelines.
                    && expr_calls_are_local(&gate.bypass_resume, identities)
            })
            && expr_calls_are_local(expr, identities);
    }
    if let Some(region) = condition_yield_region(expr) {
        return region.prefix.iter().all(|binding| {
            !expr_calls_suspending(&binding.value, suspending)
                && expr_calls_are_local(&binding.value, identities)
        }) && expr_calls_are_supported(
            &region.resume,
            identities,
            suspending,
            composable,
            tail_position,
        );
    }
    match expr {
        CoreExpr::Call { function, args } => {
            let identity = (function.clone(), args.len());
            let is_local = identities
                .iter()
                .any(|(name, arity)| *name == function && *arity == args.len());
            is_local
                && (!suspending.contains(&identity) || tail_position)
                && args.iter().all(|arg| {
                    !expr_calls_suspending(arg, suspending) && expr_calls_are_local(arg, identities)
                })
        }
        CoreExpr::ConstructorCall { args, .. }
        | CoreExpr::Intrinsic(crate::terlan_typeck::CoreIntrinsicCall { args, .. }) => {
            args.iter().all(|arg| {
                !expr_calls_suspending(arg, suspending) && expr_calls_are_local(arg, identities)
            })
        }
        CoreExpr::RecordConstruct { fields, .. } => fields.iter().all(|field| {
            !expr_calls_suspending(&field.value, suspending)
                && expr_calls_are_local(&field.value, identities)
        }),
        CoreExpr::RecordUpdate { base, fields, .. } => {
            !expr_calls_suspending(base, suspending)
                && expr_calls_are_local(base, identities)
                && fields.iter().all(|field| {
                    !expr_calls_suspending(&field.value, suspending)
                        && expr_calls_are_local(&field.value, identities)
                })
        }
        CoreExpr::UnaryOp { operand, .. } => {
            expr_calls_are_supported(operand, identities, suspending, composable, false)
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            expr_calls_are_supported(base, identities, suspending, composable, false)
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            expr_calls_are_supported(left, identities, suspending, composable, false)
                && !expr_calls_suspending(right, suspending)
                && expr_calls_are_local(right, identities)
        }
        CoreExpr::Let { bindings, body } => {
            bindings.iter().all(|binding| {
                !expr_calls_suspending(&binding.value, suspending)
                    && expr_calls_are_local(&binding.value, identities)
            }) && expr_calls_are_supported(body, identities, suspending, composable, tail_position)
        }
        CoreExpr::If { clauses } => clauses.iter().all(|clause| {
            expr_calls_are_supported(&clause.condition, identities, suspending, composable, false)
                && expr_calls_are_supported(
                    &clause.body,
                    identities,
                    suspending,
                    composable,
                    tail_position,
                )
        }),
        CoreExpr::Case { scrutinee, clauses } => {
            expr_calls_are_supported(scrutinee, identities, suspending, composable, false)
                && clauses.iter().all(|clause| {
                    clause.guard.as_ref().is_none_or(|guard| {
                        expr_calls_are_supported(guard, identities, suspending, composable, false)
                    }) && expr_calls_are_supported(
                        &clause.body,
                        identities,
                        suspending,
                        composable,
                        tail_position,
                    )
                })
        }
        CoreExpr::Lam { body, .. } => {
            expr_calls_are_supported(body, identities, suspending, composable, true)
        }
        _ => true,
    }
}

fn native_error(function: &CoreFunction, message: &str) -> String {
    format!(
        "error[native_ir.function]: `{}/{}` {message}",
        function.name, function.arity
    )
}
