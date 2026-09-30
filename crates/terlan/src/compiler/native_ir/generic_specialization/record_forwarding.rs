//! Inlines identity record constructors without changing argument evaluation order.

use crate::terlan_typeck::{CoreExpr, CoreFunction, CorePattern};
use std::collections::{HashMap, HashSet};

pub(super) fn inline_record_forwarder(
    template: &CoreFunction,
    arguments: &[CoreExpr],
) -> Option<CoreExpr> {
    let [clause] = template.clauses.as_slice() else {
        return None;
    };
    if clause.guard.is_some()
        || arguments.len() != template.params.len()
        || !clause
            .core_patterns
            .iter()
            .zip(&template.params)
            .all(|(pattern, parameter)| {
                matches!(pattern, Some(CorePattern::Var(name)) if name == &parameter.name)
            })
    {
        return None;
    }
    let CoreExpr::RecordConstruct { name, fields } = clause.body.core_expr.as_ref()? else {
        return None;
    };
    if fields.len() != template.params.len() {
        return None;
    }
    let parameter_indices = template
        .params
        .iter()
        .enumerate()
        .map(|(index, parameter)| (parameter.name.as_str(), index))
        .collect::<HashMap<_, _>>();
    let mut used = HashSet::new();
    let fields = fields
        .iter()
        .map(|field| {
            let CoreExpr::Var(parameter) = &field.value else {
                return None;
            };
            let index = *parameter_indices.get(parameter.as_str())?;
            if index != used.len() || !used.insert(index) {
                return None;
            }
            Some(crate::terlan_typeck::CoreRecordExprField {
                key: field.key.clone(),
                required: field.required,
                value: arguments[index].clone(),
            })
        })
        .collect::<Option<Vec<_>>>()?;
    (used.len() == arguments.len()).then(|| CoreExpr::RecordConstruct {
        name: name.clone(),
        fields,
    })
}
