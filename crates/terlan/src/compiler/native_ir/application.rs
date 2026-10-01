//! Application-wide NativeIR admission and symbol resolution.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use crate::terlan_typeck::{
    CoreExpr, CoreFunction, CoreImportKind, CoreModule, CorePattern, CoreType,
};

use super::{
    aggregate_types::{managed_aggregate_layouts, managed_expression_layouts},
    atom_inventory::application_atom_identities,
    collections::{managed_collection_layouts, managed_expression_collection_layouts},
    contains_process_yield, expr_calls_are_supported, is_composable_suspending_body,
    is_scalar_candidate, native_return_type_with_constructors, native_type_with_constructors,
    scalar_replacement::scalar_replace_fixed_aggregates,
    ComposedCallProfile, NativeCallableShape, NativeContinuation, NativeModule, NativeType,
    RecursiveReductionMember,
};

use super::LocalFunctionIdentity as CallIdentity;

#[path = "application/admission_diagnostics.rs"]
mod admission_diagnostics;
mod analysis;
mod callable_metadata;
pub(super) mod dynamic_targets;
mod list_builder_recursion;
pub(super) mod mutable_receivers;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use mutable_receivers::resolve_typed_mutable_receiver_calls;
mod native_packages;
#[cfg(test)]
mod native_packages_test;
mod normalization;
mod overloads;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use overloads::resolve_selected_imports;
mod record_forwarders;
mod remote_calls;
mod source_constructors;
mod struct_instances;
mod structural_patterns;
mod transparent_aliases;

use admission_diagnostics::candidate_admission_summary;
use analysis::*;
use callable_metadata::*;
use native_packages::{
    canonicalize_native_package_types, lower_compiler_native_declarations, native_handle_layouts,
    native_package_aliases, native_transparent_record_layouts,
};
use normalization::{normalize_dynamic_callable_aliases, normalize_static_callables};
use remote_calls::{normalize_remote_calls, RemoteCallPhase};

#[derive(Clone, Copy)]
struct Candidate<'a> {
    core: &'a CoreModule,
    function: &'a CoreFunction,
}

struct ApplicationLoweringTimings {
    enabled: bool,
    started: Instant,
    previous: Instant,
}

impl ApplicationLoweringTimings {
    fn begin() -> Self {
        let started = Instant::now();
        Self {
            enabled: std::env::var_os("TERLAN_NATIVE_AOT_TIMINGS").is_some(),
            started,
            previous: started,
        }
    }

    fn mark(&mut self, phase: &str) {
        if self.enabled {
            let now = Instant::now();
            eprintln!(
                "terlc native-aot: application.{phase}: +{}ms total={}ms",
                now.duration_since(self.previous).as_millis(),
                now.duration_since(self.started).as_millis(),
            );
            self.previous = now;
        }
    }
}

pub(super) fn normalize_application_remote_calls(
    cores: &mut [CoreModule],
    preserve_receivers: bool,
) {
    normalize_application_calls(
        cores,
        if preserve_receivers {
            RemoteCallPhase::Early
        } else {
            RemoteCallPhase::Final
        },
    );
}

fn normalize_application_calls(cores: &mut [CoreModule], phase: RemoteCallPhase) {
    let mut functions = HashMap::<(String, usize), Option<String>>::new();
    let mut public_by_import = HashMap::<String, Vec<(String, String, usize, String)>>::new();
    for core in cores.iter() {
        for function in &core.functions {
            let identity = (function.name.clone(), function.arity);
            let qualified = format!("{}.{}", core.module, function.name);
            functions
                .entry(identity)
                .and_modify(|target| *target = None)
                .or_insert_with(|| Some(qualified.clone()));
            if function.public {
                let candidate = (
                    core.module.clone(),
                    function.name.clone(),
                    function.arity,
                    qualified,
                );
                public_by_import
                    .entry(core.module.clone())
                    .or_default()
                    .push(candidate.clone());
                public_by_import
                    .entry(format!("{}.{}", core.module, function.name))
                    .or_default()
                    .push(candidate);
            }
        }
    }
    let visible_functions = cores
        .iter()
        .map(|caller| {
            let mut visible = functions.clone();
            let mut imported = HashMap::<(String, usize), Option<String>>::new();
            let mut seen = HashSet::<(String, String, usize)>::new();
            for import in caller
                .imports
                .iter()
                .filter(|import| import.kind == CoreImportKind::Module)
            {
                for (provider, name, arity, qualified) in
                    public_by_import.get(&import.module).into_iter().flatten()
                {
                    if provider == &caller.module
                        || !seen.insert((provider.clone(), name.clone(), *arity))
                    {
                        continue;
                    }
                    let identity = (name.clone(), *arity);
                    imported
                        .entry(identity)
                        .and_modify(|target| *target = None)
                        .or_insert_with(|| Some(qualified.clone()));
                }
            }
            visible.extend(imported);
            visible
        })
        .collect::<Vec<_>>();
    for (core, visible) in cores.iter_mut().zip(&visible_functions) {
        normalize_remote_calls(core, phase, visible);
    }
}

impl NativeModule {
    /// Lowers one checked CoreIR closure against one application-wide symbol table.
    pub(crate) fn lower_application(cores: &[&CoreModule]) -> Result<Vec<Self>, String> {
        let mut timings = ApplicationLoweringTimings::begin();
        let mut normalized_cores = cores.iter().map(|core| (*core).clone()).collect::<Vec<_>>();
        normalized_cores.sort_by(|left, right| left.module.cmp(&right.module));
        if let Some(duplicate) = normalized_cores
            .windows(2)
            .find(|pair| pair[0].module == pair[1].module)
        {
            return Err(super::application_admission::duplicate_module_diagnostic(
                &duplicate[0].module,
            ));
        }
        overloads::resolve_selected_imports(&mut normalized_cores)?;
        super::application_admission::reject_ambiguous_source_import_calls(&normalized_cores)?;
        // Expose constructor-chain bases before resolving executable bodies or
        // expanding transparent aliases, just as for direct constructor calls.
        normalized_cores
            .iter_mut()
            .for_each(super::constructor_chain::lower_constructor_chains);
        source_constructors::lower(&mut normalized_cores)?;
        for core in &mut normalized_cores {
            for function in &mut core.functions {
                function.source = Some(function.source_declaration(&core.module));
            }
        }
        super::nominal_identity::qualify_application_nominal_types(&mut normalized_cores);
        super::atom_alias_values::lower_atom_alias_values(&mut normalized_cores);
        // Resolve source receiver names before overloads rename their declarations.
        // Otherwise a method sharing a name with a free function loses its target.
        normalize_application_remote_calls(&mut normalized_cores, true);
        mutable_receivers::resolve_typed_mutable_receiver_calls(&mut normalized_cores)?;
        overloads::resolve_typed_overloads(&mut normalized_cores)?;
        let native_aliases = native_package_aliases(&normalized_cores);
        for core in &mut normalized_cores {
            lower_compiler_native_declarations(core)?;
        }
        canonicalize_native_package_types(&mut normalized_cores, &native_aliases, false)?;
        normalized_cores.iter_mut().for_each(
            super::collection_intrinsic_specialization::annotate_function_result_constructors,
        );
        transparent_aliases::expand_transparent_aliases(&mut normalized_cores);
        super::collection_intrinsic_specialization::specialize_collection_intrinsic_results(
            &mut normalized_cores,
        );
        let mut specialization_budget =
            super::specialization_budget::SpecializationBudget::default();
        for core in &mut normalized_cores {
            super::list_comprehension::lower_list_comprehensions(core)?;
            super::template_values::lower_template_values(core)?;
        }
        // Generated deferred collectors introduce checked Effect applications.
        // Resolve those new witnesses before result-only generic inference;
        // otherwise a no-argument producer can be specialized with an open T.
        transparent_aliases::expand_transparent_aliases(&mut normalized_cores);
        normalize_application_calls(&mut normalized_cores, RemoteCallPhase::BeforeSpecialization);
        timings.mark("pre-specialization");
        // Monomorphization must observe typed constructor patterns before
        // scalar case lowering erases their payload types into managed words.
        super::generic_specialization::specialize_application_generics_with_budget(
            &mut normalized_cores,
            &mut specialization_budget,
        )?;
        timings.mark("generic-specialization");
        // Generic specialization substitutes concrete arguments into cloned
        // signatures and intrinsic payloads after the first alias pass. Run
        // the idempotent resolver again so every generated mailbox boundary
        // and continuation uses the same concrete structural identity.
        transparent_aliases::expand_transparent_aliases(&mut normalized_cores);
        // Concrete producer signatures are available only after specialization;
        // refresh dependent run results before choosing an Effect runner ABI.
        super::collection_intrinsic_specialization::specialize_collection_intrinsic_results(
            &mut normalized_cores,
        );
        super::effect_execution::lower(&mut normalized_cores, &mut specialization_budget)?;
        for core in &mut normalized_cores {
            super::higher_order_specialization::specialize_higher_order_helpers_with_budget(
                core,
                &mut specialization_budget,
            )?;
            super::higher_order_context::specialize_higher_order_contexts(
                core,
                &mut specialization_budget,
            )?;
        }
        super::nested_closure_lifting::lift_nested_closure_arguments(&mut normalized_cores)?;
        list_builder_recursion::normalize_recursive_list_builders(&mut normalized_cores);
        record_forwarders::inline_record_forwarders(&mut normalized_cores);
        timings.mark("effect-and-higher-order-specialization");
        // Specialization can introduce concrete trait adapters that are not
        // reachable from any executable root. Remove those before requiring
        // physical layouts for their signatures; reachable unsupported values
        // must still fail normal native admission.
        super::open_std_pruning::prune_unreachable_open_std_functions(&mut normalized_cores);
        timings.mark("first-open-std-pruning");
        structural_patterns::scalar_replace(&mut normalized_cores)?;
        for core in &mut normalized_cores {
            super::case_lowering::lower_scalar_cases(core)?;
        }
        timings.mark("first-scalar-case-lowering");
        normalize_application_remote_calls(&mut normalized_cores, false);
        timings.mark("first-remote-call-normalization");
        for core in &mut normalized_cores {
            normalize_static_callables(core, &mut specialization_budget)?;
            normalize_dynamic_callable_aliases(core);
        }
        timings.mark("static-callable-normalization");
        normalize_application_remote_calls(&mut normalized_cores, false);
        timings.mark("second-remote-call-normalization");
        super::collection_intrinsic_specialization::specialize_collection_intrinsic_results(
            &mut normalized_cores,
        );
        super::task_values::lower(&mut normalized_cores)?;
        timings.mark("task-value-lowering");
        // Generic specialization can make collection receiver types concrete
        // only after the first target-owned normalization pass. Re-run the
        // idempotent template lowering so newly specialized template calls
        // cannot leak into final NativeIR as open stdlib calls.
        for core in &mut normalized_cores {
            super::template_values::lower_template_values(core)?;
        }
        normalize_application_remote_calls(&mut normalized_cores, false);
        structural_patterns::scalar_replace(&mut normalized_cores)?;
        for core in &mut normalized_cores {
            super::case_lowering::lower_scalar_cases(core)?;
        }
        normalize_application_remote_calls(&mut normalized_cores, false);
        mutable_receivers::resolve_typed_mutable_receiver_calls(&mut normalized_cores)?;
        super::callee_scalar_replacement::specialize_projection_callees_with_budget(
            &mut normalized_cores,
            &mut specialization_budget,
        )?;
        // Preserve generic resource arguments through specialization, then
        // admit the same concrete capability layout as nongeneric resources.
        canonicalize_native_package_types(&mut normalized_cores, &native_aliases, true)?;
        super::typed_empty_lists::annotate_empty_list_arguments(&mut normalized_cores);
        super::short_circuit_normalization::right_associate_short_circuit_chains(
            &mut normalized_cores,
        );
        super::dynamic_return::close_application_returns(&mut normalized_cores);
        // Late receiver specialization and contextual constructor annotations
        // introduce fresh Option/collection aliases after generic expansion.
        // Close those generated types before admitting managed image layouts.
        transparent_aliases::expand_transparent_aliases(&mut normalized_cores);
        super::open_std_pruning::prune_unreachable_open_std_functions(&mut normalized_cores);
        for core in &mut normalized_cores {
            // Specialization may clone a typed constructor-chain expression
            // after the early normalization pass. Native admission is a hard
            // boundary, so eliminate any such late chain before coverage and
            // ABI analysis rather than relying on an interpreter fallback.
            super::constructor_chain::lower_constructor_chains(core);
            core.termination = crate::terlan_typeck::analyze_core_termination(core);
        }
        struct_instances::retain(&mut normalized_cores, &mut specialization_budget)?;
        timings.mark("late-normalization-and-struct-retention");
        let ordered_cores = normalized_cores.iter().collect::<Vec<_>>();
        let constructor_layouts =
            super::constructors::native_application_constructor_layouts(&normalized_cores)?;
        super::application_admission::validate_core_application(
            &normalized_cores,
            &constructor_layouts,
        )?;
        timings.mark("layout-and-admission");
        let unsupported = ordered_cores
            .iter()
            .flat_map(|core| {
                core.functions
                    .iter()
                    .filter(|function| {
                        !is_scalar_candidate(function, &constructor_layouts[&core.module])
                    })
                    .map(|function| (core.module.as_str(), function))
            })
            .next();
        if let Some((module, function)) = unsupported {
            let admission = candidate_admission_summary(function, &constructor_layouts[module]);
            return Err(format!(
                "error[native_ir.unsupported_application_function]: `{module}.{}/{}` cannot be lowered into the native application image ({admission}); runtime CoreIR interpretation has been removed",
                function.name, function.arity,
            ));
        }

        let candidates = ordered_cores
            .iter()
            .flat_map(|core| {
                let mut functions = core
                    .functions
                    .iter()
                    .filter(|function| {
                        is_scalar_candidate(function, &constructor_layouts[&core.module])
                    })
                    .collect::<Vec<_>>();
                functions.sort_by(|left, right| {
                    left.name
                        .cmp(&right.name)
                        .then_with(|| left.arity.cmp(&right.arity))
                });
                functions
                    .into_iter()
                    .map(|function| Candidate { core, function })
            })
            .collect::<Vec<_>>();
        let selected = vec![true; candidates.len()];
        timings.mark("candidate-inventory");

        let resolvers = application_resolvers(&ordered_cores, &candidates, &selected);
        let suspending = application_suspending(&candidates, &selected, &resolvers);
        let composable_candidates =
            application_composable_candidates(&candidates, &selected, &resolvers, &suspending);
        for (index, candidate) in candidates.iter().enumerate() {
            if !selected[index] {
                continue;
            }
            let resolver = &resolvers[&candidate.core.module];
            let identities = resolver
                .keys()
                .map(|(name, arity)| (name.as_str(), *arity))
                .collect::<Vec<_>>();
            let suspending_names = resolved_names(resolver, &suspending);
            let composable = resolver
                .iter()
                .filter(|(_, candidate_index)| composable_candidates.contains(candidate_index))
                .map(|(identity, _)| identity.clone())
                .collect::<HashSet<_>>();
            // Application composition is a fixed-point proof that every
            // reachable suspension has a closed continuation profile. A
            // candidate admitted by that proof may contain grouped-let
            // cases whose recursive resume is intentionally broader than
            // the scalar fast-path checker; suspension-aware lowering is
            // the authoritative shape check for those candidates.
            let supported = candidate
                .function
                .clauses
                .first()
                .and_then(|clause| clause.body.core_expr.as_ref())
                .is_some_and(|body| {
                    let body = scalar_replace_fixed_aggregates(
                        body,
                        &constructor_layouts[&candidate.core.module],
                    );
                    expr_calls_are_supported(
                        &body,
                        &identities,
                        &suspending_names,
                        &composable,
                        true,
                    ) || composable_candidates.contains(&index)
                });
            if !supported {
                let gap = candidate
                    .function
                    .clauses
                    .first()
                    .and_then(|clause| clause.body.core_expr.as_ref())
                    .map(|body| {
                        super::call_composition::composable_suspension_gap_reason(
                            body,
                            &suspending_names,
                            &composable,
                        )
                    })
                    .unwrap_or_else(|| "missing checked body".to_string());
                let mut call_membership = Vec::new();
                if let Some(body) = candidate
                    .function
                    .clauses
                    .first()
                    .and_then(|clause| clause.body.core_expr.as_ref())
                {
                    dynamic_targets::walk_calls(body, &mut |function, args| {
                        let identity = (function.to_string(), args.len());
                        call_membership.push((
                            identity.clone(),
                            suspending_names.contains(&identity),
                            composable.contains(&identity),
                        ));
                    });
                }
                call_membership.sort();
                call_membership.dedup();
                let blocked_details = call_membership
                    .iter()
                    .filter(|(_, target_suspends, target_composes)| {
                        *target_suspends && !*target_composes
                    })
                    .filter_map(|(identity, _, _)| {
                        let target_index = *resolver.get(identity)?;
                        let target = candidates.get(target_index)?;
                        let target_resolver = &resolvers[&target.core.module];
                        let target_suspending = resolved_names(target_resolver, &suspending);
                        let target_composable = target_resolver
                            .iter()
                            .filter(|(_, candidate_index)| {
                                composable_candidates.contains(candidate_index)
                            })
                            .map(|(identity, _)| identity.clone())
                            .collect::<HashSet<_>>();
                        let target_body = target
                            .function
                            .clauses
                            .first()
                            .and_then(|clause| clause.body.core_expr.as_ref())?;
                        let target_gap = super::call_composition::composable_suspension_gap_reason(
                            target_body,
                            &target_suspending,
                            &target_composable,
                        );
                        let mut target_calls = Vec::new();
                        dynamic_targets::walk_calls(target_body, &mut |function, args| {
                            let called = (function.to_string(), args.len());
                            target_calls.push((
                                called.clone(),
                                target_suspending.contains(&called),
                                target_composable.contains(&called),
                            ));
                        });
                        target_calls.sort();
                        target_calls.dedup();
                        Some(format!(
                            "{}.{}/{}: gap={target_gap}; calls={target_calls:?}",
                            target.core.module, target.function.name, target.function.arity,
                        ))
                    })
                    .collect::<Vec<_>>();
                return Err(format!(
                        "error[native_ir.unsupported_application_function]: `{}.{}/{}` cannot be closed over the native application image; runtime CoreIR interpretation has been removed (gap={gap}; calls={call_membership:?}; blocked={blocked_details:?}; composable={composable:?})",
                        candidate.core.module, candidate.function.name, candidate.function.arity,
                    ));
            }
        }
        timings.mark("candidate-fixed-point");

        let modules = lower_selected_application(
            &ordered_cores,
            &candidates,
            &selected,
            &constructor_layouts,
            &resolvers,
            &suspending,
            &composable_candidates,
        )?;
        super::application_admission::validate_continuation_graph(&modules)?;
        timings.mark("selected-application-lowering");
        Ok(modules)
    }
}

/// Eliminates statically known non-escaping function values from one module.
#[path = "application/lowering.rs"]
mod lowering;

use lowering::lower_selected_application;
