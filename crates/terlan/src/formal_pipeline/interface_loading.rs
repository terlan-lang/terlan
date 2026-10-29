//! Source-scoped loading for adjacent, cached, and embedded interfaces.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::OnceLock;

use crate::terlan_hir::{load_interfaces_from_dir, parse_interface_text, ModuleInterface};
use crate::terlan_syntax::SyntaxModuleOutput;

use super::EMBEDDED_STD_INTERFACES;

/// Loads the complete inventory for release-interface regression fixtures.
#[cfg(test)]
pub(crate) fn load_external_interfaces(
    path: &str,
    cache_dir: Option<&Path>,
) -> HashMap<String, ModuleInterface> {
    let mut interfaces = load_adjacent_and_cached_interfaces(path, cache_dir);
    load_embedded_std_interfaces(&mut interfaces);
    interfaces
}

/// Loads imported embedded modules and their transitive interface dependencies.
pub(crate) fn load_external_interfaces_for_module(
    path: &str,
    cache_dir: Option<&Path>,
    module: &SyntaxModuleOutput,
) -> HashMap<String, ModuleInterface> {
    let mut interfaces = load_adjacent_and_cached_interfaces(path, cache_dir);
    let mut required = source_dependency_modules(module);
    required.extend(
        EMBEDDED_STD_INTERFACES
            .iter()
            .filter(|entry| entry.module == "std.core" || entry.module.starts_with("std.core."))
            .map(|entry| entry.module.to_string()),
    );
    load_required_standard_interfaces(path, required, &mut interfaces);
    interfaces
}

/// Loads reachable shipped summaries before batch declarations are expanded.
/// This includes generated library modules that are shipped on disk rather
/// than embedded in the compiler's fixed core-interface inventory.
pub(crate) fn load_required_standard_interfaces(
    path: &str,
    mut required: BTreeSet<String>,
    interfaces: &mut HashMap<String, ModuleInterface>,
) {
    let mut visited = BTreeSet::new();
    while let Some(module_name) = required.pop_first() {
        if !visited.insert(module_name.clone()) {
            continue;
        }
        let shipped =
            crate::support::library_paths::imported_std_summary_path(&module_name, Path::new(path))
                .and_then(|summary| {
                    let source = std::fs::read_to_string(summary).ok()?;
                    parse_interface_text(&source).filter(|(name, _)| name == &module_name)
                });
        let parsed = shipped.or_else(|| {
            let entry = EMBEDDED_STD_INTERFACES
                .iter()
                .find(|entry| entry.module == module_name && !entry.namespace)?;
            cached_embedded_std_interface(entry.summary)
        });
        if let Some((name, interface)) = parsed {
            required.extend(interface.dependency_modules.iter().cloned());
            interfaces.insert(name, interface);
        } else if let Some(interface) = interfaces.get(&module_name) {
            // A reachable cached project provider can reference shipped atom
            // aliases even when the importing source never imports their owner.
            required.extend(interface.dependency_modules.iter().cloned());
        }
    }
}

/// Source-declared dependencies shared with interface extraction.
pub(crate) fn source_dependency_modules(module: &SyntaxModuleOutput) -> BTreeSet<String> {
    crate::terlan_syntax::syntax_dependency_modules(module)
}

/// Loads interfaces adjacent to the source and compiler-cache summaries.
fn load_adjacent_and_cached_interfaces(
    path: &str,
    cache_dir: Option<&Path>,
) -> HashMap<String, ModuleInterface> {
    let mut interfaces = HashMap::new();
    let current = Path::new(path);
    if let Some(cache_dir) = cache_dir {
        load_interfaces_from_dir(cache_dir, &mut interfaces);
    }
    load_interfaces_from_dir(
        current
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new(".")),
        &mut interfaces,
    );
    interfaces
}

/// Loads every compiler-embedded stdlib summary once per compiler process.
pub(crate) fn load_embedded_std_interfaces(interfaces: &mut HashMap<String, ModuleInterface>) {
    static EMBEDDED_INTERFACES: OnceLock<HashMap<String, ModuleInterface>> = OnceLock::new();
    let embedded = EMBEDDED_INTERFACES.get_or_init(|| {
        EMBEDDED_STD_INTERFACES
            .iter()
            .filter(|entry| !entry.namespace)
            .filter_map(|entry| cached_embedded_std_interface(entry.summary))
            .collect()
    });
    for (module_name, interface) in embedded {
        interfaces
            .entry(module_name.clone())
            .or_insert_with(|| interface.clone());
    }
}

/// Uses the same content-keyed, synchronized parsing as file-backed interfaces.
fn cached_embedded_std_interface(summary: &'static str) -> Option<(String, ModuleInterface)> {
    parse_interface_text(summary)
}

#[cfg(test)]
#[path = "interface_loading_test.rs"]
mod tests;
