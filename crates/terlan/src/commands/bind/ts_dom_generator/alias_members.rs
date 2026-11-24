//! Public aliases retain their source identity and callable instance surface.
use super::*;
use std::collections::BTreeSet;

pub(super) fn expand(mapping: &mut DomModuleMapping) {
    let inventory = mapping
        .modules
        .iter()
        .map(|module| (module.module_path.clone(), module.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut invalid = BTreeSet::new();
    for module in &mut mapping.modules {
        if module.alias_target.is_none() {
            continue;
        }
        match members(module, &inventory, &mut BTreeSet::new()) {
            Ok(members) => module.members = members,
            Err((reason, detail)) => {
                invalid.insert(module.module_path.clone());
                mapping
                    .skipped
                    .push(super::super::ts_dom_module_mapping::DomSkippedDeclaration {
                        source: module.module_path.clone(),
                        reason,
                        detail,
                    });
            }
        }
    }
    mapping
        .modules
        .retain(|module| !invalid.contains(&module.module_path));
}

fn members(
    module: &DomModulePlan,
    inventory: &BTreeMap<String, DomModulePlan>,
    seen: &mut BTreeSet<String>,
) -> Result<Vec<DomMemberPlan>, (&'static str, String)> {
    if !seen.insert(module.module_path.clone()) {
        return Err((
            "ts_bindgen.cyclic_type_alias",
            "type alias cycle has no concrete declaration".into(),
        ));
    }
    let Some(alias) = &module.alias_target else {
        return Ok(module.members.clone());
    };
    let Some((name, arguments)) = alias_reference(alias) else {
        // A union containing a generic reference is not an instance of that
        // generic. Its alternatives must retain their own operations.
        return Ok(Vec::new());
    };
    let Some(target) = inventory.get(name) else {
        return Ok(Vec::new());
    };
    let mut result = members(target, inventory, seen)?;
    let args = arguments.map(split_arguments).unwrap_or_default();
    if args.len() != target.type_params.len() {
        return Err((
            "ts_bindgen.unresolved_generic_arguments",
            format!(
                "alias target {name} requires {} generic arguments; found {}",
                target.type_params.len(),
                args.len()
            ),
        ));
    }
    let replacements = target
        .type_params
        .iter()
        .cloned()
        .zip(args)
        .collect::<BTreeMap<_, _>>();
    for member in &mut result {
        match member {
            DomMemberPlan::Property(property) => {
                property.terlan_type = replace(&property.terlan_type, &replacements)
            }
            DomMemberPlan::Method(method) => {
                let replacements = replacements
                    .iter()
                    .filter(|(name, _)| !method.type_params.contains(name))
                    .map(|(name, value)| (name.clone(), value.clone()))
                    .collect();
                for param in &mut method.params {
                    param.terlan_type = replace(&param.terlan_type, &replacements);
                }
                for constraint in method.type_constraints.iter_mut().flatten() {
                    *constraint = replace(constraint, &replacements);
                }
                for bound in &mut method.callable_bounds {
                    *bound = replace(bound, &replacements);
                }
                method.return_type = replace(&method.return_type, &replacements);
            }
        }
    }
    Ok(result)
}

fn alias_reference(source: &str) -> Option<(&str, Option<&str>)> {
    let source = source.trim();
    let end = source
        .find(|c: char| !c.is_ascii_alphanumeric() && !matches!(c, '_' | '.'))
        .unwrap_or(source.len());
    if end == 0 {
        return None;
    }
    let (name, arguments) = source.split_at(end);
    let arguments = arguments.trim();
    if arguments.is_empty() {
        return Some((name, None));
    }
    if !arguments.starts_with('[') {
        return None;
    }
    let mut depth = 0;
    for (index, c) in arguments.char_indices() {
        match c {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return (index + 1 == arguments.len())
                        .then_some((name, Some(&arguments[1..index])));
                }
            }
            _ => {}
        }
    }
    None
}

fn split_arguments(source: &str) -> Vec<String> {
    let mut depth = 0;
    let mut start = 0;
    let mut result = Vec::new();
    for (index, c) in source.char_indices() {
        match c {
            '[' | '(' | '{' => depth += 1,
            ']' | ')' | '}' => depth -= 1,
            ',' if depth == 0 => {
                result.push(source[start..index].trim().into());
                start = index + 1;
            }
            _ => {}
        }
    }
    result.push(source[start..].trim().into());
    result
}

fn replace(source: &str, replacements: &BTreeMap<String, String>) -> String {
    let mut result = String::new();
    let mut start = 0;
    for (index, c) in source.char_indices() {
        if c.is_ascii_alphanumeric() || matches!(c, '_' | '.') {
            continue;
        }
        let token = &source[start..index];
        result.push_str(replacements.get(token).map(String::as_str).unwrap_or(token));
        result.push(c);
        start = index + c.len_utf8();
    }
    let token = &source[start..];
    result.push_str(replacements.get(token).map(String::as_str).unwrap_or(token));
    result
}
