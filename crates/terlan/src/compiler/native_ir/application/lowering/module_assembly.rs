//! Per-module managed metadata assembly after function lowering.

use std::collections::{HashMap, HashSet};
use std::time::Instant;

use super::super::*;
use super::support::trace_native_aot;

/// Closes one non-empty lowered module over every managed layout reachable
/// from its selected functions and checked HTTP/native-package boundaries.
pub(super) struct ModuleAssemblyContext<'a, 'candidate> {
    pub(super) started: Instant,
    pub(super) core: &'a CoreModule,
    pub(super) candidates: &'a [Candidate<'candidate>],
    pub(super) selected: &'a [bool],
    pub(super) constructor_layouts:
        &'a HashMap<String, super::super::super::constructors::NativeConstructorLayouts>,
    pub(super) atoms: &'a [String],
}

pub(super) fn assemble_native_module(
    context: ModuleAssemblyContext<'_, '_>,
    functions: Vec<super::super::super::NativeFunction>,
    continuations: Vec<NativeContinuation>,
) -> Result<Option<NativeModule>, super::super::super::NativeIrError> {
    let ModuleAssemblyContext {
        started,
        core,
        candidates,
        selected,
        constructor_layouts,
        atoms,
    } = context;
    if functions.is_empty() {
        return Ok(None);
    }
    trace_native_aot(
        started,
        "module-functions",
        format_args!(
            "module={} functions={} continuations={}",
            core.module,
            functions.len(),
            continuations.len()
        ),
    );
    let selected_candidates = || {
        candidates
            .iter()
            .enumerate()
            .filter(|(index, candidate)| selected[*index] && candidate.core.module == core.module)
            .map(|(_, candidate)| candidate)
    };
    let inferred_dynamic_returns = selected_candidates()
        .filter_map(|candidate| {
            super::super::super::dynamic_return::inferred_dynamic_return_type(candidate.function)
        })
        .collect::<Vec<_>>();
    let candidate_types = || {
        selected_candidates().flat_map(|candidate| {
            candidate
                .function
                .params
                .iter()
                .filter_map(|parameter| parameter.core_ty.as_ref())
                .chain(candidate.function.core_return_type.iter())
        })
    };
    let candidate_expressions = || {
        selected_candidates()
            .flat_map(|candidate| &candidate.function.clauses)
            .filter_map(|clause| clause.body.core_expr.as_ref())
    };

    let mut managed_layouts = constructor_layouts[&core.module]
        .iter()
        .map(|((identity, arity), layout)| {
            if let NativeType::ManagedRef(expected) = layout.result {
                let actual = layout.descriptor.managed().semantic_id();
                if actual != expected {
                    return Err(format!(
                        "error[native_ir.constructor_semantic]: `{identity}/{arity}` result semantic does not match its managed descriptor"
                    ));
                }
            }
            Ok(layout.encoded_layout.clone())
        })
        .collect::<Result<Vec<_>, String>>()?;
    managed_layouts.extend(managed_aggregate_layouts(candidate_types())?);
    managed_layouts.extend(managed_aggregate_layouts(inferred_dynamic_returns.iter())?);
    managed_layouts.extend(managed_expression_layouts(candidate_expressions())?);
    merge_managed_layouts(
        &mut managed_layouts,
        super::super::super::http_values::http_managed_layouts(core)?,
    )?;
    managed_layouts.extend(native_handle_layouts(core)?);
    managed_layouts.extend(native_transparent_record_layouts(core)?);
    managed_layouts.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
    managed_layouts.dedup_by(|left, right| left.as_ref() == right.as_ref());

    let mut managed_collections = managed_collection_layouts(candidate_types())?;
    managed_collections.extend(managed_collection_layouts(inferred_dynamic_returns.iter())?);
    managed_collections.extend(managed_expression_collection_layouts(
        candidate_expressions(),
    )?);
    managed_collections.extend(super::super::super::http_values::http_managed_collections(
        core,
    )?);
    managed_collections.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
    managed_collections.dedup_by(|left, right| left.as_ref() == right.as_ref());

    Ok(Some(NativeModule {
        name: core.module.clone(),
        functions,
        continuations,
        managed_layouts,
        managed_collections,
        atoms: atoms.to_vec(),
    }))
}

/// Installs closure metadata, validates every completion destination, and
/// performs the final application-wide tail/continuation transformations.
pub(super) struct ApplicationFinalizationContext<'a> {
    pub(super) started: Instant,
    pub(super) atoms: &'a [String],
    pub(super) call_profiles: &'a HashMap<usize, ComposedCallProfile>,
    pub(super) function_labels: &'a HashMap<usize, String>,
    pub(super) suspending_native: &'a HashSet<usize>,
    pub(super) suspending_targets: &'a [String],
}

pub(super) fn finalize_native_application(
    context: ApplicationFinalizationContext<'_>,
    mut modules: Vec<NativeModule>,
    lifted_functions: Vec<super::super::super::NativeFunction>,
) -> Result<Vec<NativeModule>, super::super::super::NativeIrError> {
    let ApplicationFinalizationContext {
        started,
        atoms,
        call_profiles,
        function_labels,
        suspending_native,
        suspending_targets,
    } = context;
    if !lifted_functions.is_empty() {
        let mut managed_layouts = modules
            .iter()
            .flat_map(|module| module.managed_layouts.iter().cloned())
            .collect::<Vec<_>>();
        managed_layouts.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
        managed_layouts.dedup_by(|left, right| left.as_ref() == right.as_ref());
        let mut managed_collections = modules
            .iter()
            .flat_map(|module| module.managed_collections.iter().cloned())
            .collect::<Vec<_>>();
        managed_collections.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
        managed_collections.dedup_by(|left, right| left.as_ref() == right.as_ref());
        modules.push(NativeModule {
            name: "$terlan.closures".to_string(),
            functions: lifted_functions,
            continuations: Vec::new(),
            managed_layouts,
            managed_collections,
            atoms: atoms.to_vec(),
        });
    }
    let mut destination_capture_counts = call_profiles
        .values()
        .flat_map(|profile| {
            profile
                .continuations
                .iter()
                .map(|continuation| (continuation.id, continuation.params.len()))
        })
        .collect::<HashMap<_, _>>();
    destination_capture_counts.extend(modules.iter().flat_map(|module| {
        module
            .continuations
            .iter()
            .map(|continuation| (continuation.id, continuation.params.len()))
    }));
    for module in &modules {
        for function in &module.functions {
            super::super::super::call_composition::validate_call_then_contracts_with_destinations(
                &function.body,
                call_profiles,
                function_labels,
                &destination_capture_counts,
            )
            .map_err(|error| {
                format!(
                    "error[native_ir.call_then_contract]: {error}; in `{}.{}/{}`",
                    function.source_module, function.source_function, function.source_arity
                )
            })?;
        }
        for continuation in &module.continuations {
            super::super::super::call_composition::validate_call_then_contracts_with_destinations(
                &continuation.body,
                call_profiles,
                function_labels,
                &destination_capture_counts,
            )
            .map_err(|error| {
                format!(
                    "error[native_ir.call_then_contract]: {error}; in continuation {} for `{}.{}/{}`",
                    continuation.id,
                    continuation.source_module,
                    continuation.source_function,
                    continuation.source_arity
                )
            })?;
        }
    }
    super::super::super::tail_position::validate_recursive_tail_targets(&modules)?;
    super::super::super::tail_position::install_reduction_continuations(&mut modules)?;
    super::super::super::tail_position::lower_recursive_tail_calls(&mut modules);
    super::super::super::tail_position::attach_installed_reduction_yields(&mut modules);
    super::super::super::application_admission::validate_continuation_graph(&modules)
        .map_err(|error| format!("{error}; before application continuation sharing"))?;
    super::super::super::continuation_sharing::materialize_shared_continuations(&mut modules)?;
    super::super::super::tail_position::lower_recursive_tail_calls(&mut modules);
    super::super::super::tail_position::attach_installed_reduction_yields(&mut modules);
    if std::env::var_os("TERLAN_NATIVE_AOT_TRACE").is_some() {
        let tail_components = super::super::super::tail_position::mutual_tail_components(&modules);
        let largest = tail_components.iter().map(Vec::len).max().unwrap_or(0);
        let members = tail_components.iter().map(Vec::len).sum::<usize>();
        trace_native_aot(
            started,
            "tail-components",
            format_args!(
                "recursive-components={} recursive-functions={members} largest-component={largest}",
                tail_components.len()
            ),
        );
    }
    trace_native_aot(
        started,
        "continuations-shared",
        format_args!(
            "modules={} functions={} continuations={}",
            modules.len(),
            modules
                .iter()
                .map(|module| module.functions.len())
                .sum::<usize>(),
            modules
                .iter()
                .map(|module| module.continuations.len())
                .sum::<usize>()
        ),
    );
    validate_composed_suspending_calls(&modules, suspending_native, suspending_targets)?;
    Ok(modules)
}
