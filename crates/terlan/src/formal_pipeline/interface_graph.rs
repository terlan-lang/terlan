//! Shared source and dependency inventory for project and batch compilation.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::terlan_hir::{
    load_interfaces_from_dir, syntax_module_output_to_interface, ModuleInterface,
};
use crate::terlan_syntax::{
    parse_interface_module_as_syntax_output, SyntaxModuleOutput, SyntaxSourceKind,
};

/// The complete visible graph and the source identities its caller validates.
/// External declarations remain subject to reachable dependency validation.
pub(crate) struct SourceInterfaceGraph {
    pub(crate) interfaces: HashMap<String, ModuleInterface>,
    pub(crate) source_modules: BTreeSet<String>,
    pub(crate) directories: BTreeSet<PathBuf>,
}

impl SourceInterfaceGraph {
    /// Collect every source directory before expanding declarations. Sorting
    /// inventories and expanding the complete graph makes batch order irrelevant.
    pub(crate) fn collect<'a>(
        sources: impl IntoIterator<Item = (&'a Path, &'a SyntaxModuleOutput)>,
        cache_dir: Option<&Path>,
    ) -> Self {
        let mut directories = BTreeSet::new();
        let mut active_path = None;
        let mut local_modules: BTreeMap<String, SyntaxModuleOutput> = BTreeMap::new();
        for (path, syntax) in sources {
            active_path.get_or_insert_with(|| path.to_path_buf());
            directories.insert(
                path.parent()
                    .filter(|parent| !parent.as_os_str().is_empty())
                    .unwrap_or(Path::new("."))
                    .to_path_buf(),
            );
            // A sibling declaration is authoritative even when its implementation
            // occurs later in source discovery or batch argument order.
            if local_modules
                .get(&syntax.module_name)
                .is_some_and(|existing| {
                    existing.source_kind == SyntaxSourceKind::Interface
                        && syntax.source_kind != SyntaxSourceKind::Interface
                })
            {
                continue;
            }
            local_modules.insert(syntax.module_name.clone(), syntax.clone());
        }
        let source_modules = local_modules.keys().cloned().collect();
        let mut interfaces = HashMap::new();
        let mut declarations = BTreeMap::new();
        for directory in cache_dir
            .into_iter()
            .chain(directories.iter().map(PathBuf::as_path))
        {
            load_interfaces_from_dir(directory, &mut interfaces);
            let Ok(entries) = std::fs::read_dir(directory) else {
                continue;
            };
            let mut paths = entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .is_some_and(|extension| extension == "terli")
                })
                .collect::<Vec<_>>();
            paths.sort();
            for path in paths {
                let Ok(source) = std::fs::read_to_string(&path) else {
                    continue;
                };
                if let Ok(syntax) = parse_interface_module_as_syntax_output(&source) {
                    declarations.insert(syntax.module_name.clone(), syntax);
                }
            }
        }
        // Handwritten declarations override generated summaries across the
        // inventory; explicit source modules then override cached copies.
        declarations.extend(local_modules);
        if let Some(path) = active_path {
            let required = declarations
                .values()
                .flat_map(super::interface_loading::source_dependency_modules)
                .filter(|module| module.starts_with("std."))
                .collect();
            super::interface_loading::load_required_standard_interfaces(
                &path.to_string_lossy(),
                required,
                &mut interfaces,
            );
        }
        for (name, syntax) in &declarations {
            interfaces.insert(name.clone(), syntax_module_output_to_interface(syntax));
        }
        super::interface_loading::load_embedded_std_interfaces(&mut interfaces);
        let modules = declarations.into_values().collect::<Vec<_>>();
        super::interface_contract::expand_interface_graph(&modules, &mut interfaces);
        Self {
            interfaces,
            source_modules,
            directories,
        }
    }
}

#[cfg(test)]
#[path = "interface_graph_test.rs"]
mod tests;
