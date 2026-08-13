//! Suspension-aware lowering for structured case clauses.

use std::collections::HashSet;

use crate::terlan_typeck::CoreExpr;

use super::{
    bind_values, bool_and, core_expr_type, extend_bindings,
    lowering::{lower_plain, StructuredCaseEnvironment},
    pattern_plan, validate_bindings, NativeExpr,
};
use crate::compiler::native_ir::{
    control,
    control::{YieldLoweringEnvironment, YieldLoweringScope, YieldLoweringState},
    expr_calls_suspending, infer_native_type_with_constructors, NativeContinuation,
};

const MAX_SUSPENDING_CASE_CLAUSES: usize = 256;

/// Lowers a top-level structured case whose scrutinee has already been
/// resumed, while allowing each selected clause body to suspend independently.
pub(crate) fn lower_suspending_case(
    expr: &CoreExpr,
    scope: YieldLoweringScope<'_>,
    environment: &YieldLoweringEnvironment<'_>,
    state: &mut YieldLoweringState<'_>,
) -> Result<(NativeExpr, Vec<NativeContinuation>), String> {
    let YieldLoweringScope {
        params,
        param_types,
        param_core_types,
        completion,
        ..
    } = scope;
    let YieldLoweringEnvironment {
        functions,
        function_types,
        function_core_types,
        constructors,
        suspending_functions,
        terminal_profiles,
        dynamic_profiles,
        module,
        function,
        arity,
        return_type,
    } = *environment;
    let ordinal = &mut *state.ordinal;
    let stable_ids = &mut *state.stable_ids;
    let CoreExpr::Case { scrutinee, clauses } = expr else {
        return Err(
            "error[native_ir.suspending_case_route]: expected a top-level structured case"
                .to_string(),
        );
    };
    if clauses.is_empty() || clauses.len() > MAX_SUSPENDING_CASE_CLAUSES {
        return Err(format!(
            "error[native_ir.suspending_case_clauses]: structured case has {} clauses; limit is {MAX_SUSPENDING_CASE_CLAUSES}",
            clauses.len()
        ));
    }
    if expr_calls_suspending(scrutinee, suspending_functions)
        || super::super::contains_process_yield(scrutinee)
    {
        return Err(
            format!(
                "error[native_ir.suspending_case_scrutinee]: structured case scrutinee `{}` was not composed before pattern lowering",
                scrutinee.contract_text()
            ),
        );
    }
    let scrutinee_type =
        infer_native_type_with_constructors(scrutinee, param_types, function_types, constructors)
            .or_else(|| {
                core_expr_type(scrutinee, param_core_types, function_core_types).and_then(
                    |core_type| {
                        crate::compiler::native_ir::native_type(
                            Some(&core_type),
                            &core_type.contract_text(),
                        )
                    },
                )
            })
            .ok_or_else(|| {
                format!(
                    "error[native_ir.suspending_case_type]: unknown scrutinee type for `{}`",
                    scrutinee.contract_text()
                )
            })?;
    let scrutinee_core = core_expr_type(scrutinee, param_core_types, function_core_types);
    let scrutinee = lower_plain(
        scrutinee,
        params,
        param_types,
        param_core_types,
        StructuredCaseEnvironment {
            functions,
            function_types,
            function_core_types,
            constructors,
        },
    )?;
    let scrutinee_slot = params
        .values()
        .copied()
        .max()
        .map_or(0, |slot| slot.saturating_add(1));
    let mut outer_slots = params.clone();
    let mut outer_types = param_types.clone();
    let mut outer_core_types = param_core_types.clone();
    let scrutinee_name = "$native_suspending_case_scrutinee".to_string();
    outer_slots.insert(scrutinee_name.clone(), scrutinee_slot);
    outer_types.insert(scrutinee_name.clone(), scrutinee_type);
    if let Some(core_type) = scrutinee_core.clone() {
        outer_core_types.insert(scrutinee_name, core_type);
    }
    let scrutinee_value = NativeExpr::Param(scrutinee_slot);

    let mut continuations = Vec::new();
    let mut native_clauses = Vec::with_capacity(clauses.len());
    for clause in clauses {
        let plan = pattern_plan(
            &clause.pattern,
            scrutinee_value.clone(),
            scrutinee_type,
            scrutinee_core.as_ref(),
            constructors,
            0,
        )?;
        validate_bindings(&plan.bindings)?;
        let (binding_slots, binding_types) = extend_bindings(
            &outer_slots,
            &outer_types,
            scrutinee_slot.saturating_add(1),
            &plan.bindings,
        );
        let binding_core_types =
            plan.bindings
                .iter()
                .fold(outer_core_types.clone(), |mut types, binding| {
                    if let Some(core_type) = &binding.core_ty {
                        types.insert(binding.name.clone(), core_type.clone());
                    }
                    types
                });
        if clause.guard.as_ref().is_some_and(|guard| {
            expr_calls_suspending(guard, suspending_functions)
                || super::super::contains_process_yield(guard)
        }) {
            return Err(
                "error[native_ir.suspending_case_guard]: suspending structured-case guards are not supported"
                    .to_string(),
            );
        }
        let guard = clause
            .guard
            .as_ref()
            .map(|guard| {
                lower_plain(
                    guard,
                    &binding_slots,
                    &binding_types,
                    &binding_core_types,
                    StructuredCaseEnvironment {
                        functions,
                        function_types,
                        function_core_types,
                        constructors,
                    },
                )
            })
            .transpose()?
            .unwrap_or(NativeExpr::Bool(true));
        let condition = bool_and(plan.predicate, bind_values(&plan.bindings, guard));
        let mut clause_names = binding_slots
            .iter()
            .map(|(name, slot)| (*slot, name.clone()))
            .collect::<Vec<_>>();
        clause_names.sort_by_key(|(slot, _)| *slot);
        let clause_names = clause_names
            .into_iter()
            .map(|(_, name)| name)
            .collect::<Vec<_>>();
        let (selected, mut selected_continuations) = control::lower_owned_expr_with_yields(
            clause.body.clone(),
            YieldLoweringScope {
                param_names: &clause_names,
                params: &binding_slots,
                param_types: &binding_types,
                param_core_types: &binding_core_types,
                completion,
            },
            &YieldLoweringEnvironment {
                functions,
                function_types,
                function_core_types,
                constructors,
                suspending_functions,
                terminal_profiles,
                dynamic_profiles,
                module,
                function,
                arity,
                return_type,
            },
            &mut YieldLoweringState {
                ordinal,
                stable_ids,
            },
        )?;
        let suspending_native = suspending_functions
            .iter()
            .filter_map(|identity| functions.get(identity).copied())
            .collect::<HashSet<_>>();
        if super::super::has_uncomposed_suspending_call(&selected, &suspending_native) {
            return Err(format!(
                "error[native_ir.suspending_case_body]: structured-case clause retained an ordinary suspending call; profiled targets {:?}; functions {:?}; while lowering {:#?}",
                terminal_profiles.keys().collect::<Vec<_>>(),
                functions,
                clause.body,
            ));
        }
        continuations.append(&mut selected_continuations);
        native_clauses.push((condition, bind_values(&plan.bindings, selected)));
    }
    Ok((
        NativeExpr::Let {
            bindings: vec![scrutinee],
            body: Box::new(NativeExpr::If {
                clauses: native_clauses,
            }),
        },
        continuations,
    ))
}
