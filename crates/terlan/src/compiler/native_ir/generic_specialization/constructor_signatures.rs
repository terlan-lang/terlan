//! Physical struct initializer signatures used by generic argument inference.

use super::*;
use crate::terlan_typeck::CoreParam;

/// Adds checked field signatures to inference, without generating executable bodies.
pub(super) fn collect(cores: &[CoreModule], templates: &mut CallableTemplates) {
    for core in cores {
        for declaration in &core.types {
            let Some(result @ CoreType::Struct { fields, .. }) = &declaration.core_body else {
                continue;
            };
            let name = format!("{}.{}", core.module, declaration.name);
            templates
                .entry((name.clone(), fields.len()))
                .or_default()
                .push(CoreFunction {
                    name,
                    source: None,
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
