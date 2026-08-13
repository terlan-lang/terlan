use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::terlan_syntax::span::Span;
use crate::terlan_typeck::{CoreExpr, CoreFunction, CoreLetBinding, CorePattern, CoreType};

use super::application_calls::{eager_argument_yield, expr_calls_are_local, expr_calls_suspending};
use super::call_composition::{
    composed_call_region, has_uncomposed_suspending_call, suspending_call_count,
    ComposedCallProfile,
};
use super::closure_conversion::{
    lower_escaping_closure, lower_escaping_function_reference, ClosureLexicalScope,
    ClosureLoweringEnvironment, ClosureOwner, NativeCallableShape,
};
use super::constructors::NativeConstructorLayouts;
use super::control::{YieldLoweringEnvironment, YieldLoweringScope, YieldLoweringState};
use super::expression::{
    expr_is_scalar, free_variables, infer_native_type, infer_native_type_with_constructors,
    native_type,
};
use super::identity::stable_export_id;
use super::model::{NativeExpr, NativeTransitionOperation, NativeType};
use super::transitions::{is_process_transition, process_transition};
use super::{
    closure_invocation, collection_values, control, dynamic_return, scalar_replacement,
    structured_case, try_lowering,
};

mod call_admission;
pub(super) use call_admission::expr_calls_are_supported;

/// ABI implemented by the first direct-AOT scalar slice.
pub(crate) const NATIVE_ABI_VERSION: &str = "terlan-native-v2";

/// Bounds source-to-native expansion while non-linear conditions are still
/// represented as nested control regions rather than a shared control graph.
const MAX_NATIVE_CONDITION_COMPOSITION_DEPTH: usize = 1_024;
const MAX_NATIVE_CALL_COMPOSITION_DEPTH: usize = 1_024;

/// Stable native statuses returned across the direct-AOT boundary.
pub(crate) mod status;

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
    /// Exact source operation that created this resume point when preserved.
    pub(crate) source_span: Option<Span>,
    /// Source-level locals captured by the parked continuation, in value order.
    pub(crate) capture_names: Vec<String>,
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

pub(super) struct NativeFunctionLoweringEnvironment<'a> {
    pub(super) identities: &'a HashMap<(String, usize), usize>,
    pub(super) function_types: &'a HashMap<(String, usize), NativeType>,
    pub(super) function_core_types: &'a HashMap<(String, usize), CoreType>,
    pub(super) callable_shapes: &'a HashMap<(String, usize), NativeCallableShape>,
    pub(super) constructors: &'a NativeConstructorLayouts,
    pub(super) suspending_functions: &'a HashSet<(String, usize)>,
    pub(super) call_profiles: &'a HashMap<usize, ComposedCallProfile>,
    pub(super) dynamic_call_profiles: &'a super::call_composition::DynamicCallProfiles,
}

pub(super) struct NativeFunctionLoweringOutputs<'a> {
    pub(super) lifted_functions: &'a mut Vec<NativeFunction>,
    pub(super) stable_ids: &'a mut HashSet<u64>,
}

pub(super) fn lower_native_function_with_callables(
    module: &str,
    function: &CoreFunction,
    environment: NativeFunctionLoweringEnvironment<'_>,
    outputs: NativeFunctionLoweringOutputs<'_>,
) -> Result<(NativeFunction, Vec<NativeContinuation>), String> {
    let NativeFunctionLoweringEnvironment {
        identities,
        function_types,
        function_core_types,
        callable_shapes,
        constructors,
        suspending_functions,
        call_profiles,
        dynamic_call_profiles,
    } = environment;
    let NativeFunctionLoweringOutputs {
        lifted_functions,
        stable_ids,
    } = outputs;
    let lifted_start = lifted_functions.len();
    let (source_module, source_function, source_arity) =
        source_declaration_identity(module, function);
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
    let contextual_body = function
        .core_return_type
        .as_ref()
        .map(|target| contextualize_tail_construction(core_body, target));
    let core_body = scalar_replacement::scalar_replace_fixed_aggregates(
        contextual_body.as_ref().unwrap_or(core_body),
        constructors,
    );
    let native_params = function
        .params
        .iter()
        .map(|param| native_type_with_constructors(param.core_ty.as_ref(), &param.ty, constructors))
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
    let param_core_types = function
        .params
        .iter()
        .filter_map(|param| {
            param
                .core_ty
                .as_ref()
                .map(|core_type| (param.name.clone(), core_type.clone()))
        })
        .collect::<HashMap<_, _>>();
    let dynamic_return_contract = function.core_return_type.as_ref().is_some_and(|ty| {
        matches!(ty, CoreType::Dynamic) || matches!(ty, CoreType::Named(name) if name == "Dynamic")
    });
    let inferred_dynamic_return = dynamic_return::inferred_dynamic_return_type(function)
        .filter(|ty| {
            !matches!(ty, CoreType::Dynamic)
                && !matches!(ty, CoreType::Named(name) if name == "Dynamic")
        })
        .or_else(|| {
            dynamic_return_contract
                .then(|| {
                    structured_case::core_expr_type(
                        &core_body,
                        &param_core_types,
                        function_core_types,
                    )
                })
                .flatten()
                .filter(|ty| {
                    !matches!(ty, CoreType::Dynamic)
                        && !matches!(ty, CoreType::Named(name) if name == "Dynamic")
                })
        });
    let effective_function_core_types = inferred_dynamic_return.as_ref().map(|inferred| {
        let mut effective = function_core_types.clone();
        effective.insert((function.name.clone(), function.arity), inferred.clone());
        effective.insert(
            (format!("{module}.{}", function.name), function.arity),
            inferred.clone(),
        );
        effective
    });
    let function_core_types = effective_function_core_types
        .as_ref()
        .unwrap_or(function_core_types);
    let return_type = (!dynamic_return_contract)
        .then(|| native_return_type_with_constructors(function, constructors))
        .flatten()
        .or_else(|| {
            inferred_dynamic_return.as_ref().and_then(|ty| {
                native_type_with_constructors(Some(ty), &ty.contract_text(), constructors)
            })
        })
        .ok_or_else(|| native_error(function, "has an unsupported return type"))?;
    let boundary_return_type = function
        .core_return_type
        .as_ref()
        .filter(|_| !dynamic_return_contract)
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
    let mut structured_case = if expr_calls_suspending(&core_body, suspending_functions)
        || contains_process_yield(&core_body)
    {
        None
    } else {
        structured_case::lower_structured_case(
            &core_body,
            function,
            &params,
            &param_types,
            structured_case::StructuredCaseEnvironment {
                functions: identities,
                function_types,
                function_core_types,
                constructors,
            },
        )?
    };
    let profiled_suspending = call_profiles.keys().copied().collect::<HashSet<_>>();
    if structured_case
        .as_ref()
        .is_some_and(|body| has_uncomposed_suspending_call(body, &profiled_suspending))
    {
        structured_case = None;
    }
    let try_expr = try_lowering::lower_try(
        &core_body,
        function,
        &params,
        &param_types,
        identities,
        function_types,
        constructors,
    )?;
    let opaque_dynamic_boundary = matches!(core_body, CoreExpr::FunctionCall { .. })
        && !suspending_functions.contains(&(function.name.clone(), function.arity));
    let indirect_invocation = if (expr_calls_suspending(&core_body, suspending_functions)
        || contains_process_yield(&core_body))
        && !opaque_dynamic_boundary
    {
        None
    } else {
        closure_invocation::lower_boundary_closure_invocation(
            &core_body,
            function,
            &params,
            &param_types,
            &closure_invocation::ClosureInvocationEnvironment {
                functions: identities,
                function_types,
                callable_shapes,
                constructors,
            },
        )?
    };
    let escaping_reference = lower_escaping_function_reference(
        &core_body,
        function.core_return_type.as_ref(),
        &params,
        callable_shapes,
    )?;
    let escaping_lambda = lower_escaping_closure(
        &core_body,
        function.core_return_type.as_ref(),
        ClosureLexicalScope {
            available: &params,
            available_types: &param_types,
        },
        &ClosureLoweringEnvironment {
            identities,
            function_types,
            constructors,
            suspending: suspending_functions,
            callable_shapes,
        },
        ClosureOwner {
            module,
            name: &function.name,
            arity: function.arity,
        },
    )?;
    let (body, mut continuations) = if let Some(body) = collection_value {
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
            YieldLoweringScope {
                param_names: &param_names,
                params: &params,
                param_types: &param_types,
                param_core_types: &param_core_types,
                completion: None,
            },
            &YieldLoweringEnvironment {
                functions: identities,
                function_types,
                function_core_types,
                constructors,
                suspending_functions,
                terminal_profiles: call_profiles,
                dynamic_profiles: dynamic_call_profiles,
                module,
                function: &function.name,
                arity: function.arity,
                return_type,
            },
            &mut YieldLoweringState {
                ordinal: &mut continuation_ordinal,
                stable_ids,
            },
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
    for continuation in &mut continuations {
        if continuation.source_module == module
            && continuation.source_function == function.name
            && continuation.source_arity == function.arity
        {
            continuation.source_module.clone_from(&source_module);
            continuation.source_function.clone_from(&source_function);
            continuation.source_arity = source_arity;
        }
    }
    for lifted in &mut lifted_functions[lifted_start..] {
        if lifted.source_module == module
            && lifted.source_function == function.name
            && lifted.source_arity == function.arity
        {
            lifted.source_module.clone_from(&source_module);
            lifted.source_function.clone_from(&source_function);
            lifted.source_arity = source_arity;
        }
    }
    Ok((
        NativeFunction {
            export_id,
            name: function.name.clone(),
            public: function.public,
            arity: function.arity,
            source_module,
            source_function,
            source_arity,
            callable_captures: Vec::new(),
            params: native_params,
            return_type,
            body,
        },
        continuations,
    ))
}

/// Recovers the source declaration represented by a concrete generic clone.
///
/// Generic specialization symbols retain the fully qualified template name so
/// a clone emitted into a consumer module can still point debugger metadata at
/// the declaration that supplied its body. Runtime dispatch continues to use
/// the consumer module and generated symbol independently of this provenance.
fn source_declaration_identity(module: &str, function: &CoreFunction) -> (String, String, usize) {
    let origin = function
        .name
        .strip_prefix("$aot_generic_")
        .and_then(|name| name.rsplit_once('_').map(|(qualified, _ordinal)| qualified))
        .and_then(|qualified| qualified.rsplit_once('.'));
    match origin {
        Some((source_module, source_function))
            if !source_module.is_empty() && !source_function.is_empty() =>
        {
            (
                source_module.to_string(),
                source_function.to_string(),
                function.arity,
            )
        }
        _ => (module.to_string(), function.name.clone(), function.arity),
    }
}

/// Retains a concrete generic return type at the construction that produces it.
/// CoreIR record/constructor nodes do not carry their surrounding expected
/// type, while direct AOT needs that type to choose an exact managed semantic
/// identity. Only tail constructions are annotated, so evaluation order and
/// non-tail inference remain unchanged.
fn contextualize_tail_construction(expr: &CoreExpr, target: &CoreType) -> CoreExpr {
    match expr {
        CoreExpr::ConstructorCall { .. } | CoreExpr::RecordConstruct { .. } => CoreExpr::Cast {
            expr: Box::new(expr.clone()),
            target_type: target.clone(),
        },
        CoreExpr::Let { bindings, body } => CoreExpr::Let {
            bindings: bindings.clone(),
            body: Box::new(contextualize_tail_construction(body, target)),
        },
        CoreExpr::If { clauses } => CoreExpr::If {
            clauses: clauses
                .iter()
                .cloned()
                .map(|mut clause| {
                    clause.body = contextualize_tail_construction(&clause.body, target);
                    clause
                })
                .collect(),
        },
        CoreExpr::Case { scrutinee, clauses } => CoreExpr::Case {
            scrutinee: scrutinee.clone(),
            clauses: clauses
                .iter()
                .cloned()
                .map(|mut clause| {
                    clause.body = contextualize_tail_construction(&clause.body, target);
                    clause
                })
                .collect(),
        },
        _ => expr.clone(),
    }
}

pub(super) fn contains_process_yield(expr: &CoreExpr) -> bool {
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
        CoreExpr::UnaryOp { operand, .. } | CoreExpr::Cast { expr: operand, .. } => {
            contains_process_yield(operand)
        }
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
        CoreExpr::Case { scrutinee, clauses } => {
            contains_process_yield(scrutinee)
                || clauses.iter().any(|clause| {
                    clause.guard.as_ref().is_some_and(contains_process_yield)
                        || contains_process_yield(&clause.body)
                })
        }
        _ => false,
    }
}

pub(super) fn is_scalar_candidate(
    function: &CoreFunction,
    constructors: &NativeConstructorLayouts,
) -> bool {
    function.native_operation.is_none()
        && function
            .params
            .iter()
            .all(|param| native_type(param.core_ty.as_ref(), &param.ty).is_some())
        && native_return_type_with_constructors(function, constructors).is_some()
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
pub(super) fn native_return_type(function: &CoreFunction) -> Option<NativeType> {
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

pub(super) fn native_return_type_with_constructors(
    function: &CoreFunction,
    constructors: &NativeConstructorLayouts,
) -> Option<NativeType> {
    native_type_with_constructors(
        function.core_return_type.as_ref(),
        &function.return_type,
        constructors,
    )
    .or_else(|| native_return_type(function))
    .or_else(|| {
        matches!(
            function.core_return_type.as_ref(),
            Some(crate::terlan_typeck::CoreType::Dynamic)
        )
        .then_some(())?;
        let variables = function
            .params
            .iter()
            .map(|param| {
                native_type_with_constructors(param.core_ty.as_ref(), &param.ty, constructors)
                    .map(|ty| (param.name.clone(), ty))
            })
            .collect::<Option<HashMap<_, _>>>()?;
        let body = function.clauses.first()?.body.core_expr.as_ref()?;
        infer_native_type_with_constructors(body, &variables, &HashMap::new(), constructors)
    })
}

pub(super) fn native_type_with_constructors(
    core: Option<&crate::terlan_typeck::CoreType>,
    text: &str,
    constructors: &NativeConstructorLayouts,
) -> Option<NativeType> {
    let refined_core = core
        .filter(|core| core_type_contains_dynamic(core))
        .and_then(|_| concrete_textual_core_type(text, constructors));
    let core = refined_core.as_ref().or(core);
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

/// Recovers concrete imported record types erased to `Dynamic` inside a
/// collection/function annotation. Runtime collection identities include the
/// structural element contract, so retaining `List(Dynamic)` for a source
/// `List[Payload]` would make an AOT continuation reject the actual list.
fn concrete_textual_core_type(
    text: &str,
    constructors: &NativeConstructorLayouts,
) -> Option<CoreType> {
    let mut recovered = crate::terlan_typeck::core_type_from_text(text)?;
    resolve_constructor_types(&mut recovered, constructors);
    (!core_type_contains_dynamic(&recovered)).then_some(recovered)
}

fn resolve_constructor_types(ty: &mut CoreType, constructors: &NativeConstructorLayouts) {
    match ty {
        CoreType::Named(name) => {
            let replacement = unique_constructor_core_type(name, constructors);
            if let Some(replacement) = replacement {
                *ty = replacement;
            }
        }
        CoreType::Apply { args, .. } => args
            .iter_mut()
            .for_each(|ty| resolve_constructor_types(ty, constructors)),
        CoreType::Tuple(items) => {
            items.iter_mut().for_each(|item| match item {
                crate::terlan_typeck::CoreTupleTypeElem::Type(ty)
                | crate::terlan_typeck::CoreTupleTypeElem::Field { ty, .. } => {
                    resolve_constructor_types(ty, constructors)
                }
            });
        }
        CoreType::List(item) => resolve_constructor_types(item, constructors),
        CoreType::Struct { fields, .. } => fields
            .iter_mut()
            .for_each(|field| resolve_constructor_types(&mut field.ty, constructors)),
        CoreType::Map(fields) => fields
            .iter_mut()
            .for_each(|field| resolve_constructor_types(&mut field.value, constructors)),
        CoreType::Arrow {
            params,
            return_type,
        } => {
            params
                .iter_mut()
                .for_each(|param| resolve_constructor_types(param, constructors));
            resolve_constructor_types(return_type, constructors);
        }
        CoreType::Union(types) => types
            .iter_mut()
            .for_each(|ty| resolve_constructor_types(ty, constructors)),
        CoreType::Int
        | CoreType::Float
        | CoreType::Number
        | CoreType::String
        | CoreType::Binary
        | CoreType::Atom
        | CoreType::Bool
        | CoreType::Term
        | CoreType::Dynamic
        | CoreType::Never
        | CoreType::AtomLiteral(_) => {}
    }
}

fn unique_constructor_core_type(
    name: &str,
    constructors: &NativeConstructorLayouts,
) -> Option<CoreType> {
    let mut matches = constructors.iter().filter_map(|((identity, _), layout)| {
        (identity == name || identity.rsplit('.').next() == Some(name))
            .then_some(layout.result_core_type.as_ref())
            .flatten()
    });
    let first = matches.next()?.clone();
    matches
        .all(|candidate| candidate == &first)
        .then_some(first)
}

fn core_type_contains_dynamic(ty: &CoreType) -> bool {
    match ty {
        CoreType::Dynamic => true,
        CoreType::Named(name) if name == "Dynamic" => true,
        CoreType::Apply { args, .. } => args.iter().any(core_type_contains_dynamic),
        CoreType::List(item) => core_type_contains_dynamic(item),
        CoreType::Tuple(items) => items.iter().any(|item| match item {
            crate::terlan_typeck::CoreTupleTypeElem::Type(ty)
            | crate::terlan_typeck::CoreTupleTypeElem::Field { ty, .. } => {
                core_type_contains_dynamic(ty)
            }
        }),
        CoreType::Struct { fields, .. } => fields
            .iter()
            .any(|field| core_type_contains_dynamic(&field.ty)),
        CoreType::Map(fields) => fields
            .iter()
            .any(|field| core_type_contains_dynamic(&field.value)),
        CoreType::Arrow {
            params,
            return_type,
        } => {
            params.iter().any(core_type_contains_dynamic) || core_type_contains_dynamic(return_type)
        }
        CoreType::Union(types) => types.iter().any(core_type_contains_dynamic),
        CoreType::Int
        | CoreType::Float
        | CoreType::Number
        | CoreType::String
        | CoreType::Binary
        | CoreType::Atom
        | CoreType::Bool
        | CoreType::Term
        | CoreType::Never
        | CoreType::AtomLiteral(_)
        | CoreType::Named(_) => false,
    }
}

pub(super) fn expr_is_native_control(expr: &CoreExpr) -> bool {
    if expr_is_scalar(expr) {
        return true;
    }
    if process_transition(expr)
        .is_some_and(|(_, arguments, _)| arguments.iter().all(expr_is_scalar))
    {
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
                            || (contains_process_yield(&binding.value)
                                && expr_is_native_control(&binding.value))
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
                && (expr_is_scalar(scrutinee)
                    || (contains_process_yield(scrutinee) && expr_is_native_control(scrutinee)))
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
        } if matches!(operator.as_str(), "and" | "or") => {
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
            if matches!(operator.as_str(), "and" | "or")
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
        if operator == "or" && expr_is_scalar(left) && expr_is_native_condition(right) {
            return expr_is_native_control(body);
        }
    }
    expr_is_native_condition(condition)
}

#[cfg(test)]
mod source_identity_test;
mod yield_regions;
#[cfg(test)]
mod yield_regions_test;

pub(super) use yield_regions::*;

fn native_error(function: &CoreFunction, message: &str) -> String {
    format!(
        "error[native_ir.function]: `{}/{}` {message}",
        function.name, function.arity
    )
}
