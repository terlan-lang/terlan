use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::{Mutex, OnceLock};

use crate::terlan_hir::{
    parse_interface_dependency_entries, parse_interface_file, ModuleInterface,
};
use crate::terlan_syntax::{
    syntax_module_import_identities, SyntaxDeclarationPayload, SyntaxImportKind, SyntaxModuleOutput,
};

static STD_INTERFACES: OnceLock<Mutex<HashMap<String, ModuleInterface>>> = OnceLock::new();

/// Loads the minimal checked-in std interface closure for one test module.
///
/// Inputs:
/// - `module`: syntax-output fixture whose imports define the roots.
///
/// Output:
/// - Direct std interfaces and all dependencies declared by `.typi.deps`.
///
/// Transformation:
/// - Traverses structured dependency manifests and parses each exact interface
///   summary at most once per test process.
pub(crate) fn checked_in_std_interfaces_for_module(
    module: &SyntaxModuleOutput,
) -> HashMap<String, ModuleInterface> {
    let summaries = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join("std/summaries");
    let mut pending = syntax_module_import_identities(module)
        .into_iter()
        .filter(|module_name| module_name.starts_with("std."))
        .collect::<Vec<_>>();
    pending.extend(collapsed_std_module_candidates(module));
    let mut selected = HashSet::new();
    let mut interfaces = HashMap::new();

    while let Some(module_name) = pending.pop() {
        if !selected.insert(module_name.clone()) {
            continue;
        }
        let Some(interface) = cached_std_interface(&summaries, &module_name) else {
            continue;
        };
        interfaces.insert(module_name.clone(), interface);

        let manifest_path = summaries.join(format!("{module_name}.typi.deps"));
        let dependencies = fs::read_to_string(manifest_path)
            .ok()
            .and_then(|contents| parse_interface_dependency_entries(&contents))
            .unwrap_or_default();
        pending.extend(dependencies.into_iter().map(|(dependency, _)| dependency));
    }

    interfaces
}

fn collapsed_std_module_candidates(module: &SyntaxModuleOutput) -> Vec<String> {
    module
        .declarations
        .iter()
        .filter_map(|declaration| match &declaration.payload {
            SyntaxDeclarationPayload::Import {
                import_kind: SyntaxImportKind::Module,
                module_name,
                items,
                ..
            } if module_name.starts_with("std.") && items.len() > 1 => Some((module_name, items)),
            _ => None,
        })
        .flat_map(|(module_name, items)| {
            items
                .iter()
                .map(move |item| format!("{module_name}.{}", item.name))
        })
        .collect()
}

/// Loads and caches one exact checked-in std interface summary.
fn cached_std_interface(summaries: &std::path::Path, module_name: &str) -> Option<ModuleInterface> {
    let cache = STD_INTERFACES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().expect("std interface cache lock");
    if let Some(interface) = cache.get(module_name).cloned() {
        return Some(interface);
    }

    let path = summaries.join(format!("{module_name}.typi"));
    let (parsed_name, interface) = parse_interface_file(&path)?;
    if parsed_name != module_name {
        return None;
    }

    cache.insert(module_name.to_string(), interface.clone());
    Some(interface)
}
