//! Source-scoped loading for adjacent, cached, and embedded interfaces.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::{OnceLock, RwLock};

use crate::terlan_hir::{
    load_interfaces_from_dir, syntax_module_output_to_interface, ModuleInterface,
};
use crate::terlan_syntax::{
    parse_interface_module_as_syntax_output, syntax_module_import_identities,
    SyntaxDeclarationPayload, SyntaxExprOutput, SyntaxFunctionClauseOutput, SyntaxImportKind,
    SyntaxModuleOutput, SyntaxParamOutput,
};

use super::EMBEDDED_STD_INTERFACE_SUMMARIES;

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

/// Loads only embedded standard-library modules imported by one parsed module.
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
    for summary in EMBEDDED_STD_INTERFACE_SUMMARIES {
        let Some(module_name) = embedded_summary_module_name(summary) else {
            continue;
        };
        let compiler_prelude = module_name == "std.core" || module_name.starts_with("std.core.");
        if (compiler_prelude || required.contains(module_name))
            && !interfaces.contains_key(module_name)
        {
            if let Some((module_name, interface)) = cached_embedded_std_interface(summary) {
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
        EMBEDDED_STD_INTERFACE_SUMMARIES
            .iter()
            .filter_map(|summary| cached_embedded_std_interface(summary))
            .collect()
    });
    for (module_name, interface) in embedded {
        interfaces
            .entry(module_name.clone())
            .or_insert_with(|| interface.clone());
    }
}

/// Reads a module identity without invoking the interface parser.
fn embedded_summary_module_name(summary: &str) -> Option<&str> {
    summary.lines().find_map(|line| {
        line.strip_prefix("module ")
            .and_then(|name| name.strip_suffix('.'))
    })
}

/// Parses one embedded interface through the canonical syntax and HIR path.
fn parse_embedded_std_interface(summary: &str) -> Option<(String, ModuleInterface)> {
    let parsed = parse_interface_module_as_syntax_output(summary).ok()?;
    let module_name = parsed.module_name.clone();
    Some((module_name, syntax_module_output_to_interface(&parsed)))
}

/// Parses each embedded summary at most once while allowing source-scoped
/// callers to clone only their admitted interface subset.
fn cached_embedded_std_interface(summary: &'static str) -> Option<(String, ModuleInterface)> {
    static CACHE: OnceLock<RwLock<HashMap<&'static str, ModuleInterface>>> = OnceLock::new();
    let module_name = embedded_summary_module_name(summary)?;
    let cache = CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    if let Ok(interfaces) = cache.read() {
        if let Some(interface) = interfaces.get(module_name) {
            return Some((module_name.to_string(), interface.clone()));
        }
    }
    let (_, parsed) = parse_embedded_std_interface(summary)?;
    let interface = match cache.write() {
        Ok(mut interfaces) => interfaces.entry(module_name).or_insert(parsed).clone(),
        Err(_) => parsed,
    };
    Some((module_name.to_string(), interface))
}

#[cfg(test)]
#[path = "interface_loading_test.rs"]
mod tests;
