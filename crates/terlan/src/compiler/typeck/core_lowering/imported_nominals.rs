//! Preserve checked imported record/opaque identities before dependency discovery.

use super::*;

pub(super) fn qualify(core: &mut CoreModule, resolved: &ResolvedModule) {
    let names = resolved
        .imported_types
        .iter()
        .filter_map(|(local, imported)| {
            let provider = resolved.interface_map.get(&imported.source_module)?;
            (provider.struct_fields.contains_key(&imported.source_name)
                || provider.opaque_types.contains(&imported.source_name))
            .then(|| {
                (
                    local.clone(),
                    format!("{}.{}", imported.source_module, imported.source_name),
                )
            })
        })
        .collect::<HashMap<_, _>>();
    if names.is_empty() {
        return;
    }
    for declaration in &mut core.types {
        if let Some(body) = &mut declaration.core_body {
            qualify_type(body, &names, &declaration.params);
        }
    }
    for function in &mut core.functions {
        for ty in function
            .params
            .iter_mut()
            .filter_map(|param| param.core_ty.as_mut())
            .chain(function.core_return_type.as_mut())
        {
            qualify_type(ty, &names, &function.generic_params);
        }
    }
}

fn qualify_type(ty: &mut CoreType, names: &HashMap<String, String>, parameters: &[String]) {
    ty.visit_names_mut(&mut |name| {
        if !parameters
            .iter()
            .any(|parameter| normalize_type_param_name(parameter) == *name)
        {
            if let Some(qualified) = names.get(name) {
                *name = qualified.clone();
            }
        }
    });
}

#[cfg(test)]
#[path = "imported_nominals_test.rs"]
mod tests;
