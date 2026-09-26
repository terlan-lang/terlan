//! Per-module worklist for bounded generic function instantiation.

use super::*;

pub(super) fn specialize_core(
    core: &mut CoreModule,
    templates: &CallableTemplates,
    budget: &mut super::super::specialization_budget::SpecializationBudget,
) -> Result<(), String> {
    let mut cache = BTreeMap::<(String, Vec<String>), String>::new();
    let mut cursor = 0usize;
    while cursor < core.functions.len() {
        if function_is_generic(&core.functions[cursor]) {
            cursor += 1;
            continue;
        }
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
        cursor += 1;
    }
    core.functions
        .retain(|function| !function_is_generic(function));
    Ok(())
}
