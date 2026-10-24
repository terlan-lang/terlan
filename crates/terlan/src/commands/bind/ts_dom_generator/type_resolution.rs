//! Resolves generated signatures against the actual generated type inventory.

use std::collections::BTreeMap;

use super::super::ts_dom_module_mapping::DomSkippedDeclaration;
use super::*;

/// Qualifies foreign types and refuses members that refer to unavailable types.
pub(super) fn qualify(mapping: &mut DomModuleMapping) {
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
    for module in &mut mapping.modules {
        let mut members = std::mem::take(&mut module.members);
        members.retain_mut(|member| {
            let mut unknown = Vec::new();
            let (name, annotations) = match member {
                DomMemberPlan::Property(property) => {
                    (property.js_name.clone(), vec![&mut property.terlan_type])
                }
                DomMemberPlan::Method(method) => {
                    let mut annotations = method
                        .params
                        .iter_mut()
                        .map(|parameter| &mut parameter.terlan_type)
                        .collect::<Vec<_>>();
                    annotations.push(&mut method.return_type);
                    (method.js_name.clone(), annotations)
                }
            };
            for annotation in annotations {
                *annotation = qualify_type(annotation, module, &types, &mut unknown);
            }
            unknown.sort();
            unknown.dedup();
            for missing in &unknown {
                mapping.skipped.push(DomSkippedDeclaration {
                    source: format!("{}.{}", module.source_interface, name),
                    reason: "ts_bindgen.unresolved_type_reference",
                    detail: format!("member {name} references unavailable type {missing}"),
                });
            }
            unknown.is_empty()
        });
        module.members = members;
        // JavaScript has one callable per property, while Terlan exports need
        // distinct names. Preserve every supported overload under a stable name.
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
        if let Some(alias) = &module.alias_target {
            module.alias_target = Some(qualify_type(alias, module, &types, &mut Vec::new()));
        }
    }
}

fn qualify_type(
    source: &str,
    module: &DomModulePlan,
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
            if name.contains('.')
                || name == module.type_name
                || module.type_params.iter().any(|parameter| parameter == name)
                || matches!(
                    name,
                    "Bool" | "Int" | "Float" | "String" | "Atom" | "Dynamic" | "Never"
                )
                || !name.as_bytes()[0].is_ascii_uppercase()
            {
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
