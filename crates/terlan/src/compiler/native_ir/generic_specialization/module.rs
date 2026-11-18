//! Per-module worklist for bounded generic function instantiation.

use super::*;
use std::time::Instant;

pub(super) fn specialize_core(
    core: &mut CoreModule,
    templates: &CallableTemplates,
    budget: &mut super::super::specialization_budget::SpecializationBudget,
    retain_generic_templates: bool,
) -> Result<(), String> {
    let mut cache = BTreeMap::<(String, Vec<String>), String>::new();
    let trace = crate::compiler_timings::native_enabled();
    let mut cursor = 0usize;
    while cursor < core.functions.len() {
        if function_is_generic(&core.functions[cursor]) {
            cursor += 1;
            continue;
        }
        let started = Instant::now();
        let function_name = core.functions[cursor].name.clone();
        let specialization_count = cache.len();
        let mut generated = Vec::new();
        let expected_result = core.functions[cursor].core_return_type.clone();
        let parameter_types = core.functions[cursor]
            .params
            .iter()
            .filter_map(|parameter| {
                parameter
                    .core_ty
                    .as_ref()
                    .map(|ty| (parameter.name.clone(), ty.clone()))
            })
            .collect::<HashMap<_, _>>();
        for clause in &mut core.functions[cursor].clauses {
            if let Some(guard) = clause
                .guard
                .as_mut()
                .and_then(|guard| guard.core_expr.as_mut())
            {
                rewrite_expr(
                    guard,
                    &parameter_types,
                    templates,
                    &mut cache,
                    &mut generated,
                    &core.module,
                    budget,
                )?;
            }
            if let Some(body) = clause.body.core_expr.as_mut() {
                if let Some(expected) = &expected_result {
                    contextual_result::seed(
                        body,
                        expected,
                        &parameter_types,
                        templates,
                        &core.module,
                    );
                }
                rewrite_expr(
                    body,
                    &parameter_types,
                    templates,
                    &mut cache,
                    &mut generated,
                    &core.module,
                    budget,
                )?;
            }
        }
        core.functions.extend(generated);
        if trace && started.elapsed().as_millis() >= 100 {
            eprintln!(
                "terlc native-aot: generic-specialization.{}.{function_name}: {}ms specializations={}",
                core.module,
                started.elapsed().as_millis(),
                cache.len().saturating_sub(specialization_count),
            );
        }
        cursor += 1;
    }
    if !retain_generic_templates {
        core.functions
            .retain(|function| !function_is_generic(function));
    }
    Ok(())
}
