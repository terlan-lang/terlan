//! Application-wide NativeIR admission and symbol resolution.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::terlan_typeck::{
    CoreExpr, CoreFunction, CoreImportKind, CoreModule, CorePattern, CoreType,
};

use super::{
    aggregate_types::{managed_aggregate_layouts, managed_expression_layouts},
    atom_inventory::application_atom_identities,
    collections::{managed_collection_layouts, managed_expression_collection_layouts},
    constructors::native_constructor_layouts,
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
mod dynamic_targets;
mod mutable_receivers;
pub(crate) use mutable_receivers::resolve_typed_mutable_receiver_calls;
mod native_packages;
#[cfg(test)]
mod native_packages_test;
mod normalization;
mod record_forwarders;
mod remote_calls;
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
use remote_calls::normalize_remote_calls;

#[derive(Clone, Copy)]
struct Candidate<'a> {
    core: &'a CoreModule,
    function: &'a CoreFunction,
}

pub(super) fn normalize_application_remote_calls(
    cores: &mut [CoreModule],
    preserve_receivers: bool,
) {
    let mut functions = HashMap::<(String, usize), Option<String>>::new();
    for core in cores.iter() {
        for function in &core.functions {
            let identity = (function.name.clone(), function.arity);
            let qualified = format!("{}.{}", core.module, function.name);
            functions
                .entry(identity)
                .and_modify(|target| *target = None)
                .or_insert_with(|| Some(qualified));
        }
    }
    let visible_functions = cores
        .iter()
        .map(|caller| {
            let mut visible = functions.clone();
            let mut imported = HashMap::<(String, usize), Option<String>>::new();
            for provider in cores
                .iter()
                .filter(|provider| provider.module != caller.module)
            {
                for function in provider.functions.iter().filter(|function| function.public) {
                    let imported_provider = caller.imports.iter().any(|import| {
                        import.kind == CoreImportKind::Module
                            && (import.module == provider.module
                                || import.module
                                    == format!("{}.{}", provider.module, function.name))
                    });
                    if !imported_provider {
                        continue;
                    }
                    let identity = (function.name.clone(), function.arity);
                    let qualified = format!("{}.{}", provider.module, function.name);
                    imported
                        .entry(identity)
                        .and_modify(|target| *target = None)
                        .or_insert_with(|| Some(qualified));
                }
            }
            visible.extend(imported);
            visible
        })
        .collect::<Vec<_>>();
    for (core, visible) in cores.iter_mut().zip(&visible_functions) {
        normalize_remote_calls(core, preserve_receivers, visible);
    }
}

impl NativeModule {
    /// Lowers one checked CoreIR closure against one application-wide symbol table.
    pub(crate) fn lower_application(cores: &[&CoreModule]) -> Result<Vec<Self>, String> {
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
        super::open_std_pruning::prune_compile_time_router_builders(&mut normalized_cores);
        super::nominal_identity::qualify_application_nominal_types(&mut normalized_cores);
        super::atom_alias_values::lower_atom_alias_values(&mut normalized_cores);
        let native_aliases = native_package_aliases(&normalized_cores);
        for core in &mut normalized_cores {
            lower_compiler_native_declarations(core)?;
        }
        canonicalize_native_package_types(&mut normalized_cores, &native_aliases)?;
        normalize_application_remote_calls(&mut normalized_cores, true);
        mutable_receivers::resolve_typed_mutable_receiver_calls(&mut normalized_cores)?;
        // Constructor-chain bases can themselves be imported transparent
        // aliases. Expose them as ordinary constructor calls before alias
        // expansion so the same structural rewrite handles both direct calls
        // and chain bases.
        normalized_cores
            .iter_mut()
            .for_each(super::constructor_chain::lower_constructor_chains);
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
            super::http_values::lower_http_values(core)?;
        }
        normalize_application_remote_calls(&mut normalized_cores, false);
        // Monomorphization must observe typed constructor patterns before
        // scalar case lowering erases their payload types into managed words.
        super::generic_specialization::specialize_application_generics_with_budget(
            &mut normalized_cores,
            &mut specialization_budget,
        )?;
        // Generic specialization substitutes concrete arguments into cloned
        // signatures and intrinsic payloads after the first alias pass. Run
        // the idempotent resolver again so every generated mailbox boundary
        // and continuation uses the same concrete structural identity.
        transparent_aliases::expand_transparent_aliases(&mut normalized_cores);
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
        record_forwarders::inline_record_forwarders(&mut normalized_cores);
        structural_patterns::scalar_replace(&mut normalized_cores)?;
        for core in &mut normalized_cores {
            super::case_lowering::lower_scalar_cases(core)?;
        }
        normalize_application_remote_calls(&mut normalized_cores, false);
        for core in &mut normalized_cores {
            normalize_static_callables(core, &mut specialization_budget)?;
            normalize_dynamic_callable_aliases(core);
        }
        normalize_application_remote_calls(&mut normalized_cores, false);
        super::collection_intrinsic_specialization::specialize_collection_intrinsic_results(
            &mut normalized_cores,
        );
        // Generic specialization can make collection receiver types concrete
        // only after the first target-owned normalization pass. Re-run the
        // idempotent HTTP/template lowerings so newly specialized Map/Option
        // and template calls cannot leak into final NativeIR as open stdlib
        // calls.
        for core in &mut normalized_cores {
            super::template_values::lower_template_values(core)?;
            super::http_values::lower_http_values(core)?;
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
        super::typed_empty_lists::annotate_empty_list_arguments(&mut normalized_cores);
        super::short_circuit_normalization::right_associate_short_circuit_chains(
            &mut normalized_cores,
        );
        super::open_std_pruning::prune_unreachable_open_std_functions(&mut normalized_cores);
        for core in &mut normalized_cores {
            // Specialization may clone a typed constructor-chain expression
            // after the early normalization pass. Native admission is a hard
            // boundary, so eliminate any such late chain before coverage and
            // ABI analysis rather than relying on an interpreter fallback.
            super::constructor_chain::lower_constructor_chains(core);
            core.termination = crate::terlan_typeck::analyze_core_termination(core);
        }
        let ordered_cores = normalized_cores.iter().collect::<Vec<_>>();
        let constructor_modules = ordered_cores
            .iter()
            .map(|core| (core.module.as_str(), core.constructors.as_slice()))
            .collect::<Vec<_>>();
        let type_modules = ordered_cores
            .iter()
            .map(|core| (core.module.as_str(), core.types.as_slice()))
            .collect::<Vec<_>>();
        let mut constructor_layouts = ordered_cores
            .iter()
            .map(|core| {
                native_constructor_layouts(&constructor_modules, &core.module).and_then(
                    |mut layouts| {
                        super::constructors::install_struct_layouts(
                            &type_modules,
                            &core.module,
                            &mut layouts,
                        )?;
                        Ok((core.module.clone(), layouts))
                    },
                )
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        for core in &ordered_cores {
            super::http_values::install_http_constructors(
                core,
                constructor_layouts
                    .get_mut(&core.module)
                    .expect("constructor layouts built for every ordered module"),
            )?;
        }
        super::application_admission::validate_core_application(
            &normalized_cores,
            &constructor_layouts,
        )?;
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

        loop {
            let resolvers = application_resolvers(&ordered_cores, &candidates, &selected);
            let suspending = application_suspending(&candidates, &selected, &resolvers);
            let composable_candidates =
                application_composable_candidates(&candidates, &selected, &resolvers, &suspending);
            let before = selected.iter().filter(|selected| **selected).count();
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
                        )
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
                    return Err(format!(
                        "error[native_ir.unsupported_application_function]: `{}.{}/{}` cannot be closed over the native application image; runtime CoreIR interpretation has been removed (gap={gap}; calls={call_membership:?}; composable={composable:?})",
                        candidate.core.module, candidate.function.name, candidate.function.arity,
                    ));
                }
            }
            if selected.iter().filter(|selected| **selected).count() == before {
                break;
            }
        }

        let modules = lower_selected_application(
            &ordered_cores,
            &candidates,
            &selected,
            &constructor_layouts,
        )?;
        super::application_admission::validate_continuation_graph(&modules)?;
        Ok(modules)
    }
}

/// Eliminates statically known non-escaping function values from one module.
#[path = "application/lowering.rs"]
mod lowering;

use lowering::lower_selected_application;
