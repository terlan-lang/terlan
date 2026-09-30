//! Preserve checked managed return representations across synchronous case arms.

use std::collections::{HashMap, HashSet};

use crate::compiler::native_ir::control::{
    lower_expr_with_yields, YieldLoweringEnvironment, YieldLoweringScope, YieldLoweringState,
};
use crate::compiler::native_ir::{NativeExpr, NativeType};
use crate::terlan_typeck::{CoreExpr, CoreFunction, CoreType};

use super::StructuredCaseEnvironment;

/// Reuses type-directed control lowering rather than emitting untyped case tails.
pub(super) fn lower_managed_result(
    body: &CoreExpr,
    function: &CoreFunction,
    params: &HashMap<String, usize>,
    param_types: &HashMap<String, NativeType>,
    core_types: &HashMap<String, CoreType>,
    environment: StructuredCaseEnvironment<'_>,
) -> Result<Option<NativeExpr>, String> {
    let Some(return_core) = function.core_return_type.as_ref() else {
        return Ok(None);
    };
    let Some(return_type @ NativeType::ManagedRef(_)) =
        crate::compiler::native_ir::native_type(Some(return_core), &function.return_type)
    else {
        return Ok(None);
    };
    let mut core_returns = environment.function_core_types.clone();
    core_returns.insert((function.name.clone(), function.arity), return_core.clone());
    let mut names = params.iter().collect::<Vec<_>>();
    names.sort_by_key(|(_, slot)| **slot);
    let names = names
        .into_iter()
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    let (body, continuations) = lower_expr_with_yields(
        body,
        YieldLoweringScope {
            param_names: &names,
            params,
            param_types,
            param_core_types: core_types,
            completion: None,
        },
        &YieldLoweringEnvironment {
            functions: environment.functions,
            function_types: environment.function_types,
            function_core_types: &core_returns,
            constructors: environment.constructors,
            suspending_functions: &HashSet::new(),
            terminal_profiles: &HashMap::new(),
            dynamic_profiles: &HashMap::new(),
            module: "",
            function: &function.name,
            arity: function.arity,
            return_type,
        },
        &mut YieldLoweringState {
            ordinal: &mut 0,
            stable_ids: &mut HashSet::new(),
        },
    )?;
    if !continuations.is_empty() {
        return Err(
            "error[native_ir.structured_case_boundary]: synchronous case produced a continuation"
                .into(),
        );
    }
    Ok(Some(body))
}
