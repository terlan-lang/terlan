//! Source-scoped loading for adjacent, cached, and embedded interfaces.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::OnceLock;

use crate::terlan_hir::{
    load_interfaces_from_dir, parse_interface_dependency_entries, parse_interface_text,
    ModuleInterface,
};
use crate::terlan_syntax::{
    collect_remote_modules, collect_type_modules, syntax_module_import_identities,
    SyntaxDeclarationPayload, SyntaxImportKind, SyntaxModuleOutput,
};

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

/// Loads imported embedded modules and their transitive manifest dependencies.
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
        if let Some(summary) = crate::support::library_paths::imported_std_summary_path(
            &module_name,
            std::path::Path::new(path),
        ) {
            if let Ok(source) = std::fs::read_to_string(&summary) {
                if let Some((name, interface)) = parse_interface_text(&source) {
                    if name == module_name {
                        if let Ok(manifest) =
                            std::fs::read_to_string(summary.with_extension("typi.deps"))
                        {
                            if let Some(dependencies) =
                                parse_interface_dependency_entries(&manifest)
                            {
                                required.extend(dependencies.into_iter().map(|(name, _)| name));
                            }
                        }
                        interfaces.insert(name, interface);
                        continue;
                    }
                }
            }
        }
        let entry = EMBEDDED_STD_INTERFACES
            .iter()
            .find(|entry| entry.module == module_name);
        let Some(entry) = entry else {
            if let Some(summary) = crate::support::library_paths::imported_std_summary_path(
                &module_name,
                Path::new(path),
            ) {
                if let Ok(source) = std::fs::read_to_string(&summary) {
                    if let Some((name, interface)) = parse_interface_text(&source) {
                        if name == module_name {
                            if let Ok(manifest) =
                                std::fs::read_to_string(summary.with_extension("typi.deps"))
                            {
                                if let Some(dependencies) =
                                    parse_interface_dependency_entries(&manifest)
                                {
                                    required.extend(dependencies.into_iter().map(|(name, _)| name));
                                }
                            }
                            interfaces.entry(name).or_insert(interface);
                        }
                    }
                }
            }
            continue;
        };
        let Some(manifest) = entry.dependencies else {
            continue; // Namespace indexes are not callable module interfaces.
        };
        let dependencies = parse_interface_dependency_entries(manifest)
            .expect("embedded standard dependency manifest is valid");
        required.extend(dependencies.into_iter().map(|(dependency, _)| dependency));
        if !interfaces.contains_key(entry.module) {
            if let Some((module_name, interface)) = cached_embedded_std_interface(entry.summary) {
                interfaces.insert(module_name, interface);
            }
        }
    }
}

/// Source-declared dependency identities, including default imports and remote calls.
pub(crate) fn source_dependency_modules(module: &SyntaxModuleOutput) -> BTreeSet<String> {
    let mut required = syntax_module_import_identities(module)
        .into_iter()
        .collect::<BTreeSet<_>>();
    for declaration in &module.declarations {
        let SyntaxDeclarationPayload::Import {
            import_kind: SyntaxImportKind::Module,
            module_name,
            items,
            ..
        } = &declaration.payload
        else {
            continue;
        };
        required.extend(
            items
                .iter()
                .map(|item| format!("{module_name}.{}", item.name)),
        );
    }
    collect_remote_modules(module, &mut required);
    collect_annotation_modules(module, &mut required);
    required
}

/// Qualified annotation references are dependencies even without an import.
fn collect_annotation_modules(module: &SyntaxModuleOutput, modules: &mut BTreeSet<String>) {
    for declaration in &module.declarations {
        let mut texts = Vec::new();
        match &declaration.payload {
            SyntaxDeclarationPayload::Constant { annotation, .. } => {
                texts.push(annotation.text.as_str())
            }
            SyntaxDeclarationPayload::ConstFunction { return_type, .. } => {
                texts.push(return_type.text.as_str())
            }
            SyntaxDeclarationPayload::Type {
                params,
                variants,
                representation,
                implements,
                ..
            } => {
                texts.extend(params.iter().map(String::as_str));
                texts.extend(
                    variants
                        .iter()
                        .chain(representation)
                        .chain(implements)
                        .map(|ty| ty.text.as_str()),
                );
            }
            SyntaxDeclarationPayload::Struct {
                generic_params,
                includes,
                implements,
                fields,
                ..
            } => {
                texts.extend(generic_params.iter().chain(includes).map(String::as_str));
                texts.extend(implements.iter().map(|ty| ty.text.as_str()));
                texts.extend(fields.iter().map(|field| field.annotation.text.as_str()));
            }
            SyntaxDeclarationPayload::Constructor {
                params, clauses, ..
            } => {
                texts.extend(params.iter().map(String::as_str));
                for clause in clauses {
                    texts.push(clause.return_type.text.as_str());
                    texts.extend(
                        clause
                            .params
                            .iter()
                            .map(|param| param.annotation.text.as_str()),
                    );
                }
            }
            SyntaxDeclarationPayload::Function {
                generic_params,
                generic_bounds,
                return_type,
                ..
            }
            | SyntaxDeclarationPayload::Method {
                generic_params,
                generic_bounds,
                return_type,
                ..
            } => {
                texts.extend(
                    generic_params
                        .iter()
                        .chain(generic_bounds)
                        .map(String::as_str),
                );
                texts.push(return_type.text.as_str());
                if let SyntaxDeclarationPayload::Method { receiver, .. } = &declaration.payload {
                    texts.push(receiver.annotation.text.as_str());
                }
            }
            SyntaxDeclarationPayload::Trait {
                params,
                super_traits,
                methods,
                constants,
                ..
            } => {
                texts.extend(params.iter().chain(super_traits).map(String::as_str));
                for method in methods {
                    texts.extend(method.generic_params.iter().map(String::as_str));
                    texts.push(method.return_type.text.as_str());
                }
                texts.extend(
                    constants
                        .iter()
                        .map(|constant| constant.annotation.text.as_str()),
                );
            }
            SyntaxDeclarationPayload::TraitImpl {
                generic_params,
                trait_ref,
                for_type,
                methods,
                constants,
                ..
            } => {
                texts.extend(generic_params.iter().map(String::as_str));
                texts.extend([trait_ref.text.as_str(), for_type.text.as_str()]);
                texts.extend(
                    methods
                        .iter()
                        .map(|method| method.return_type.text.as_str()),
                );
                texts.extend(
                    constants
                        .iter()
                        .filter_map(|constant| constant.annotation.as_ref())
                        .map(|annotation| annotation.text.as_str()),
                );
            }
            SyntaxDeclarationPayload::Template { props, .. } => {
                texts.extend(props.iter().map(|prop| prop.annotation.text.as_str()))
            }
            _ => {}
        }
        for text in texts {
            collect_type_modules(text, modules);
        }
    }
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
            .filter(|entry| entry.dependencies.is_some())
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
