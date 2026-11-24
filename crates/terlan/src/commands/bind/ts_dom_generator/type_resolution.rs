//! Resolves generated signatures against the actual generated type inventory.

use std::collections::BTreeMap;

use super::super::ts_dom_module_mapping::DomSkippedDeclaration;
use super::*;

/// Qualifies foreign types and refuses members that refer to unavailable types.
pub(super) fn qualify(mapping: &mut DomModuleMapping) {
    // Removing an unresolved alias can invalidate aliases that refer to it.
    loop {
        let types = inventory(mapping);
        let before = mapping.modules.len();
        mapping.modules.retain_mut(|module| {
            let Some(alias) = &module.alias_target else {
                return true;
            };
            let mut unknown = Vec::new();
            let qualified = qualify_type(alias, module, &[], &types, &mut unknown);
            if unknown.is_empty() {
                module.alias_target = Some(qualified);
                return true;
            }
            mapping.skipped.push(DomSkippedDeclaration {
                source: module.module_path.clone(),
                reason: "ts_bindgen.unresolved_type_reference",
                detail: format!("alias references unavailable types {}", unknown.join(", ")),
            });
            false
        });
        if mapping.modules.len() == before {
            break;
        }
    }
    let types = inventory(mapping);
    for module in &mut mapping.modules {
        let mut members = std::mem::take(&mut module.members);
        let original_count = members.len();
        members.retain_mut(|member| {
            let mut unknown = Vec::new();
            let (name, retained) = match member {
                DomMemberPlan::Property(property) => {
                    property.terlan_type =
                        qualify_type(&property.terlan_type, module, &[], &types, &mut unknown);
                    (&property.js_name, unknown.is_empty())
                }
                DomMemberPlan::Method(method) => {
                    let retained = qualify_method(method, module, &types, &mut unknown);
                    (&method.js_name, retained)
                }
            };
            unknown.sort();
            unknown.dedup();
            for missing in &unknown {
                mapping.skipped.push(DomSkippedDeclaration {
                    source: format!("{}.{}", module.source_interface, name),
                    reason: "ts_bindgen.unresolved_type_reference",
                    detail: format!("member {name} references unavailable type {missing}"),
                });
            }
            retained
        });
        module.constructible &= members.len() == original_count;
        module.members = members;
        let mut names = BTreeMap::<String, usize>::new();
        for member in &mut module.members {
            let name = match member {
                DomMemberPlan::Property(property) => &mut property.terlan_name,
                DomMemberPlan::Method(method) => &mut method.terlan_name,
            };
            let count = names.entry(name.clone()).or_default();
            *count += 1;
            if *count > 1 {
                *name = format!("{name}_overload_{count}");
            }
        }
    }
}

fn qualify_method(
    method: &mut DomMethodPlan,
    module: &DomModulePlan,
    types: &BTreeMap<String, Vec<String>>,
    unknown: &mut Vec<String>,
) -> bool {
    let required = method
        .params
        .iter()
        .rposition(|parameter| !parameter.optional)
        .map_or(0, |index| index + 1);
    method.return_type = qualify_type(
        &method.return_type,
        module,
        &method.type_params,
        types,
        unknown,
    );
    for constraint in method.type_constraints.iter_mut().flatten() {
        *constraint = qualify_type(constraint, module, &method.type_params, types, unknown);
    }
    for bound in &mut method.callable_bounds {
        *bound = qualify_type(bound, module, &method.type_params, types, unknown);
    }
    let mut retained = unknown.is_empty();
    for (index, parameter) in method.params.iter_mut().enumerate() {
        let before = unknown.len();
        parameter.terlan_type = qualify_type(
            &parameter.terlan_type,
            module,
            &method.type_params,
            types,
            unknown,
        );
        if unknown.len() == before {
            continue;
        }
        if index < required {
            retained = false;
        } else {
            // This prefix is a valid JavaScript call because the whole suffix is optional.
            method.params.truncate(index);
            method.partial = true;
            break;
        }
    }
    retained
}

fn inventory(mapping: &DomModuleMapping) -> BTreeMap<String, Vec<String>> {
    let mut types = BTreeMap::<String, Vec<String>>::new();
    for module in &mapping.modules {
        types.entry(module.type_name.clone()).or_default().push(
            crate::terlan_hir::public_type_path(&module.module_path, &module.type_name),
        );
    }
    for (name, provider) in [
        ("JsString", "std.js.String.JsString"),
        ("JsNumber", "std.js.Number.JsNumber"),
        ("Array", "std.js.Array"),
        ("Promise", "std.js.Promise"),
        ("Option", "std.core.Option"),
        ("Unit", "std.core.Unit"),
    ] {
        types.insert(name.into(), vec![provider.into()]);
    }
    types
}

fn qualify_type(
    source: &str,
    module: &DomModulePlan,
    bound: &[String],
    types: &BTreeMap<String, Vec<String>>,
    unknown: &mut Vec<String>,
) -> String {
    let mut result = String::new();
    let mut cursor = 0;
    let bytes = source.as_bytes();
    while cursor < bytes.len() {
        let start = cursor;
        if bytes[cursor].is_ascii_alphabetic() || bytes[cursor] == b'_' {
            cursor += 1;
            while cursor < bytes.len()
                && (bytes[cursor].is_ascii_alphanumeric() || matches!(bytes[cursor], b'_' | b'.'))
            {
                cursor += 1;
            }
            let name = &source[start..cursor];
            if name == module.type_name
                || module.type_params.iter().any(|parameter| parameter == name)
                || bound.iter().any(|parameter| parameter == name)
                || matches!(
                    name,
                    "Bool" | "Int" | "Float" | "String" | "Atom" | "Dynamic" | "Never"
                )
                || (!name.contains('.') && !name.as_bytes()[0].is_ascii_uppercase())
            {
                result.push_str(name);
            } else if name.contains('.') {
                if !name.starts_with("std.") && !types.values().flatten().any(|value| value == name)
                {
                    unknown.push(name.into());
                }
                result.push_str(name);
            } else if let Some(candidates) = types.get(name) {
                let namespace = module.module_path.rsplit_once('.').unwrap().0;
                let selected = candidates
                    .iter()
                    .find(|candidate| candidate.starts_with(&format!("{namespace}.")))
                    .or_else(|| (candidates.len() == 1).then(|| &candidates[0]));
                if let Some(selected) = selected {
                    result.push_str(selected);
                } else {
                    unknown.push(name.into());
                    result.push_str(name);
                }
            } else {
                unknown.push(name.into());
                result.push_str(name);
            }
        } else {
            let character = source[cursor..].chars().next().unwrap();
            result.push(character);
            cursor += character.len_utf8();
        }
    }
    result
}
