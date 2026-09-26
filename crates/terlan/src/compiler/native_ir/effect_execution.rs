//! Typed native runners for deferred Effect plans; no runtime source interpreter.

use std::collections::{BTreeMap, HashMap};

use crate::terlan_typeck::{
    visit_core_expr_mut, CoreExpr, CoreFunction, CoreIntrinsicId, CoreModule,
    CorePrimitiveIntrinsic, CoreType,
};

use super::{
    generic_specialization::substitute,
    specialization_budget::{SpecializationBudget, SpecializationKind},
    NativeIrResult,
};

mod callbacks;
mod runner;

#[cfg(test)]
#[path = "effect_execution_test.rs"]
mod tests;

const MODULE: &str = "std.core.Effect";

/// One concrete callback signature admitted by a checked Effect constructor.
#[derive(Clone, PartialEq, Eq)]
struct Callback {
    input: CoreType,
    output: CoreType,
    flat: bool,
}

/// The canonical standard declaration supplies every generated plan layout.
struct Schema {
    parameter: String,
    body: CoreType,
}

impl Schema {
    fn plan(&self, result: &CoreType) -> CoreType {
        substitute(
            &self.body,
            std::slice::from_ref(&self.parameter),
            &HashMap::from([(self.parameter.clone(), result.clone())]),
        )
    }
}

/// Replaces runtime run boundaries with bounded, concretely typed AOT helpers.
/// Ordinary recursive-call and closure lowering retains scheduler ownership.
pub(super) fn lower(
    cores: &mut [CoreModule],
    budget: &mut SpecializationBudget,
) -> NativeIrResult<()> {
    let mut results = BTreeMap::new();
    let mut owner = None;
    for core in cores.iter_mut() {
        for function in &mut core.functions {
            let mut contains_run = false;
            visit_function(function, &mut |expr| {
                if let CoreExpr::Intrinsic(call) = expr {
                    if call.id == CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmEffectRun) {
                        results.insert(call.return_type.contract_text(), call.return_type.clone());
                        contains_run = true;
                    }
                }
            });
            if contains_run && owner.is_none() {
                owner = Some(function.clone());
            }
        }
    }
    let Some(owner) = owner else { return Ok(()) };
    let module_index = cores.iter().position(|core| core.module == MODULE).ok_or(
        "error[native_ir.effect_schema]: Effect.run requires the canonical standard declaration",
    )?;
    let declaration = cores[module_index]
        .types
        .iter()
        .find(|ty| ty.name == "Effect")
        .ok_or("error[native_ir.effect_schema]: canonical Effect declaration is missing")?;
    let ([parameter], Some(body)) = (declaration.params.as_slice(), &declaration.core_body) else {
        return Err("error[native_ir.effect_schema]: expected one checked Effect parameter".into());
    };
    let schema = Schema {
        parameter: parameter.clone(),
        body: body.clone(),
    };
    let callbacks = callbacks::collect(cores, &schema)?;
    // Close the type dependency graph before mutation. Cycles consume one
    // helper per result type, never one helper per runtime plan node.
    let mut queue = results.values().cloned().collect::<Vec<_>>();
    let mut cursor = 0;
    while cursor < queue.len() {
        let output = queue[cursor].clone();
        require_concrete(&output)?;
        for callback in callbacks
            .iter()
            .filter(|callback| callback.output == output)
        {
            let key = callback.input.contract_text();
            if let std::collections::btree_map::Entry::Vacant(entry) = results.entry(key) {
                entry.insert(callback.input.clone());
                queue.push(callback.input.clone());
            }
        }
        cursor += 1;
    }
    budget.reserve(SpecializationKind::Effect, MODULE, results.len())?;
    let mut names = BTreeMap::new();
    let mut helpers = Vec::new();
    for (key, result) in &results {
        let name = runner_name(result);
        if cores[module_index]
            .functions
            .iter()
            .any(|function| function.name == name)
            || names.insert(name.clone(), key.clone()).is_some()
        {
            return Err(
                "error[native_ir.effect_identity]: generated runner identity collision".into(),
            );
        }
        helpers.push(runner::build(&owner, &schema, result, &callbacks, name)?);
    }
    let mut malformed = false;
    for core in cores.iter_mut() {
        for function in &mut core.functions {
            visit_function(function, &mut |expr| {
                let CoreExpr::Intrinsic(call) = expr else {
                    return;
                };
                if call.id != CoreIntrinsicId::Primitive(CorePrimitiveIntrinsic::VmEffectRun) {
                    return;
                }
                if call.args.len() != 1 {
                    malformed = true;
                    return;
                }
                *expr = run_call(&call.return_type, call.args[0].clone());
            });
        }
    }
    if malformed {
        return Err("error[native_ir.effect_run]: expected exactly one typed plan".into());
    }
    cores[module_index].functions.extend(helpers);
    Ok(())
}

fn require_concrete(ty: &CoreType) -> NativeIrResult<()> {
    if super::expression::native_type(Some(ty), &ty.contract_text()).is_none() {
        return Err(format!(
            "error[native_ir.effect_type]: Effect execution needs a concrete native type, got `{}`",
            ty.contract_text(),
        )
        .into());
    }
    Ok(())
}

fn runner_name(result: &CoreType) -> String {
    let identity = super::identity::stable_export_id("$terlan.effect", &result.contract_text(), 1);
    format!("$aot_effect_run_{identity:016x}")
}

fn run_call(result: &CoreType, plan: CoreExpr) -> CoreExpr {
    CoreExpr::Cast {
        expr: Box::new(CoreExpr::Call {
            function: format!("{MODULE}.{}", runner_name(result)),
            type_args: Vec::new(),
            args: vec![plan],
        }),
        target_type: result.clone(),
    }
}

fn visit_function(function: &mut CoreFunction, visit: &mut impl FnMut(&mut CoreExpr)) {
    for clause in &mut function.clauses {
        for summary in clause
            .guard
            .iter_mut()
            .chain(std::iter::once(&mut clause.body))
        {
            if let Some(expr) = &mut summary.core_expr {
                visit_core_expr_mut(expr, visit);
            }
        }
    }
}
