//! Close generated dynamic boundaries using already resolved callable results.

use std::collections::HashMap;

use crate::terlan_typeck::{CoreModule, CoreType};

type Results = HashMap<(String, usize), CoreType>;

fn dynamic(ty: &CoreType) -> bool {
    matches!(ty, CoreType::Dynamic) || matches!(ty, CoreType::Named(name) if name == "Dynamic")
}

/// Retains only unambiguous results for an exact resolved name and arity.
fn results(cores: &[CoreModule]) -> Results {
    let mut results = HashMap::<_, Option<CoreType>>::new();
    for core in cores {
        for function in &core.functions {
            let result = super::inferred_dynamic_return_type(function)
                .or_else(|| function.core_return_type.clone())
                .filter(|ty| !dynamic(ty));
            let key = (format!("{}.{}", core.module, function.name), function.arity);
            results
                .entry(key)
                .and_modify(|prior| {
                    if *prior != result {
                        *prior = None;
                    }
                })
                .or_insert(result);
        }
    }
    results
        .into_iter()
        .filter_map(|(key, result)| result.map(|result| (key, result)))
        .collect()
}

/// Closes the normalized application's result without modifying checked source.
///
/// Run after call and case normalization. Function lowering already uses this
/// call-aware recovery; admission and managed-layout assembly must see the same
/// result. The application operates on cloned CoreIR; its checked inputs and
/// source interfaces retain their declared Dynamic result. Do not wrap a control
/// body in a cast: that obscures structured cases and suspension boundaries.
/// Each successful step closes a previously unresolved boundary, so the
/// fixed point terminates even when unresolved dynamic calls form a cycle.
pub(in crate::compiler::native_ir) fn close_application_returns(cores: &mut [CoreModule]) {
    loop {
        if !cores
            .iter()
            .flat_map(|core| &core.functions)
            .any(|function| {
                function.core_return_type.as_ref().is_some_and(dynamic)
                    && super::inferred_dynamic_return_type(function).is_none()
            })
        {
            break;
        }
        let results = results(cores);
        let mut changed = false;
        for core in cores.iter_mut() {
            if !core.functions.iter().any(|function| {
                function.core_return_type.as_ref().is_some_and(dynamic)
                    && super::inferred_dynamic_return_type(function).is_none()
            }) {
                continue;
            }
            let mut visible = results.clone();
            for function in &core.functions {
                let key = (format!("{}.{}", core.module, function.name), function.arity);
                if let Some(result) = results.get(&key) {
                    visible.insert((function.name.clone(), function.arity), result.clone());
                }
            }
            for function in &mut core.functions {
                if !function.core_return_type.as_ref().is_some_and(dynamic)
                    || super::inferred_dynamic_return_type(function).is_some()
                {
                    continue;
                }
                let [clause] = function.clauses.as_mut_slice() else {
                    continue;
                };
                let Some(body) = clause.body.core_expr.as_mut() else {
                    continue;
                };
                let variables = function
                    .params
                    .iter()
                    .filter_map(|parameter| {
                        parameter
                            .core_ty
                            .clone()
                            .map(|ty| (parameter.name.clone(), ty))
                    })
                    .collect();
                let Some(result) =
                    super::super::structured_case::core_expr_type(body, &variables, &visible)
                        .filter(|ty| !dynamic(ty))
                else {
                    continue;
                };
                function.return_type = result.contract_text();
                function.core_return_type = Some(result);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
}
