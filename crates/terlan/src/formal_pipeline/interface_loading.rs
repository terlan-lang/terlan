//! Source-scoped loading for adjacent, cached, and embedded interfaces.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::OnceLock;

use crate::terlan_hir::{
    load_interfaces_from_dir, parse_interface_dependency_entries, parse_interface_text,
    ModuleInterface,
};
use crate::terlan_syntax::{
    syntax_module_import_identities, SyntaxDeclarationPayload, SyntaxExprOutput,
    SyntaxFunctionClauseOutput, SyntaxImportKind, SyntaxModuleOutput, SyntaxParamOutput,
};

use super::EMBEDDED_STD_INTERFACES;

/// Loads the full visible interface inventory for audits and compatibility
/// callers that do not yet have parsed module evidence.
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
    required.extend(
        EMBEDDED_STD_INTERFACES
            .iter()
            .filter(|entry| entry.module == "std.core" || entry.module.starts_with("std.core."))
            .map(|entry| entry.module.to_string()),
    );
    let mut visited = BTreeSet::new();
    while let Some(module_name) = required.pop_first() {
        if !visited.insert(module_name.clone()) {
            continue;
        }
        let Some(entry) = EMBEDDED_STD_INTERFACES
            .iter()
            .find(|entry| entry.module == module_name)
        else {
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
    interfaces
}

/// Adds fully qualified remote modules referenced without an import alias.
fn collect_remote_modules(module: &SyntaxModuleOutput, modules: &mut BTreeSet<String>) {
    for declaration in &module.declarations {
        match &declaration.payload {
            SyntaxDeclarationPayload::Constant { value, .. } => {
                collect_expr_modules(value, modules)
            }
            SyntaxDeclarationPayload::ConstFunction { params, body, .. } => {
                collect_param_modules(params, modules);
                collect_expr_modules(body, modules);
            }
            SyntaxDeclarationPayload::Type { valued_arms, .. } => valued_arms
                .iter()
                .for_each(|arm| collect_expr_modules(&arm.value, modules)),
            SyntaxDeclarationPayload::Struct { fields, .. } => fields
                .iter()
                .filter_map(|field| field.default.as_ref())
                .for_each(|value| collect_expr_modules(value, modules)),
            SyntaxDeclarationPayload::Constructor { clauses, .. } => {
                for clause in clauses {
                    clause
                        .params
                        .iter()
                        .filter_map(|param| param.default.as_ref())
                        .for_each(|value| collect_expr_modules(value, modules));
                    collect_expr_modules(&clause.body, modules);
                }
            }
            SyntaxDeclarationPayload::Function {
                params, clauses, ..
            }
            | SyntaxDeclarationPayload::Method {
                params, clauses, ..
            } => {
                collect_param_modules(params, modules);
                collect_clause_modules(clauses, modules);
            }
            SyntaxDeclarationPayload::Trait {
                methods, constants, ..
            } => {
                for method in methods {
                    collect_param_modules(&method.params, modules);
                    if let Some(body) = method.default_body.as_ref() {
                        collect_expr_modules(body, modules);
                    }
                }
                constants
                    .iter()
                    .filter_map(|constant| constant.default.as_ref())
                    .for_each(|value| collect_expr_modules(value, modules));
            }
            SyntaxDeclarationPayload::TraitImpl {
                methods, constants, ..
            } => {
                for method in methods {
                    collect_param_modules(&method.params, modules);
                    collect_clause_modules(&method.clauses, modules);
                }
                constants
                    .iter()
                    .for_each(|constant| collect_expr_modules(&constant.value, modules));
            }
            SyntaxDeclarationPayload::Template { props, .. } => props
                .iter()
                .filter_map(|prop| prop.default.as_ref())
                .for_each(|value| collect_expr_modules(value, modules)),
            _ => {}
        }
    }
}

fn collect_param_modules(params: &[SyntaxParamOutput], modules: &mut BTreeSet<String>) {
    params
        .iter()
        .filter_map(|param| param.default.as_ref())
        .for_each(|value| collect_expr_modules(value, modules));
}

fn collect_clause_modules(clauses: &[SyntaxFunctionClauseOutput], modules: &mut BTreeSet<String>) {
    for clause in clauses {
        if let Some(guard) = clause.guard.as_ref() {
            collect_expr_modules(guard, modules);
        }
        collect_expr_modules(&clause.body, modules);
    }
}

fn collect_expr_modules(expr: &SyntaxExprOutput, modules: &mut BTreeSet<String>) {
    if let Some(module) = expr
        .remote
        .as_ref()
        .filter(|module| module.starts_with("std."))
    {
        modules.insert(module.clone());
    }
    expr.children
        .iter()
        .for_each(|child| collect_expr_modules(child, modules));
    expr.let_guards
        .iter()
        .flatten()
        .for_each(|guard| collect_expr_modules(guard, modules));
    expr.fields
        .iter()
        .for_each(|field| collect_expr_modules(&field.value, modules));
    expr.clauses.iter().for_each(|clause| {
        if let Some(guard) = clause.guard.as_ref() {
            collect_expr_modules(guard, modules);
        }
        collect_expr_modules(&clause.body, modules);
    });
    expr.catch_clauses.iter().for_each(|clause| {
        if let Some(guard) = clause.guard.as_ref() {
            collect_expr_modules(guard, modules);
        }
        collect_expr_modules(&clause.body, modules);
    });
    if let Some(after) = expr.try_after.as_ref() {
        collect_expr_modules(&after.trigger, modules);
        collect_expr_modules(&after.body, modules);
    }
}

/// Loads interfaces adjacent to the source and compiler-cache summaries.
fn load_adjacent_and_cached_interfaces(
    path: &str,
    cache_dir: Option<&Path>,
) -> HashMap<String, ModuleInterface> {
    let mut interfaces = HashMap::new();
    let current = Path::new(path);
    load_interfaces_from_dir(current.parent().unwrap_or(Path::new(".")), &mut interfaces);
    if let Some(cache_dir) = cache_dir {
        load_interfaces_from_dir(cache_dir, &mut interfaces);
    }
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
