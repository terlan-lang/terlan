//! Physical struct initializer signatures used by generic argument inference.

use super::*;
use crate::terlan_typeck::{CoreParam, CoreRecordExprField};

/// Infers a nominal application from checked field signatures, matching fields
/// by name rather than assuming the source literal follows declaration order.
pub(super) fn infer_record(
    name: &str,
    fields: &[CoreRecordExprField],
    variables: &HashMap<String, CoreType>,
    templates: &CallableTemplates,
    module: &str,
) -> Option<CoreType> {
    let candidates = callable_templates(templates, module, name, fields.len());
    let mut records = candidates.into_iter().flatten().filter(|candidate| {
        candidate.clauses.is_empty()
            && matches!(candidate.core_return_type, Some(CoreType::Struct { .. }))
    });
    let Some(record) = records.next() else {
        return Some(CoreType::Named(name.into()));
    };
    if records.next().is_some() {
        return None;
    }
    if record.generic_params.is_empty() {
        return Some(CoreType::Named(name.into()));
    }
    let mut values = HashMap::new();
    for parameter in &record.params {
        let mut matches = fields.iter().filter(|field| field.key == parameter.name);
        let field = matches.next()?;
        if matches.next().is_some() {
            return None;
        }
        let actual = infer_type(&field.value, variables, templates, module)?;
        unify(
            parameter.core_ty.as_ref()?,
            &actual,
            &record.generic_params,
            &mut values,
        )
        .ok()?;
    }
    Some(CoreType::Apply {
        constructor: record.name.clone(),
        args: record
            .generic_params
            .iter()
            .map(|parameter| values.get(parameter).cloned())
            .collect::<Option<Vec<_>>>()?,
    })
}

/// Adds checked field signatures to inference, without generating executable bodies.
pub(super) fn collect(cores: &[CoreModule], templates: &mut CallableTemplates) {
    for core in cores {
        for declaration in &core.types {
            let Some(result @ CoreType::Struct { fields, .. }) = &declaration.core_body else {
                continue;
            };
            let name = format!("{}.{}", core.module, declaration.name);
            templates
                .functions
                .entry((name.clone(), fields.len()))
                .or_default()
                .push(CoreFunction {
                    name,
                    source: None,
                    receiver_method: false,
                    receiver_mutable: false,
                    receiver_command: false,
                    trait_method: None,
                    arity: fields.len(),
                    public: false,
                    generic_params: declaration.params.clone(),
                    native_operation: None,
                    params: fields
                        .iter()
                        .map(|field| CoreParam {
                            name: field.name.clone(),
                            ty: field.ty.contract_text(),
                            core_ty: Some(field.ty.clone()),
                        })
                        .collect(),
                    return_type: result.contract_text(),
                    core_return_type: Some(result.clone()),
                    clauses: Vec::new(),
                });
        }
    }
}

#[cfg(test)]
#[path = "constructor_signatures_test.rs"]
mod tests;
