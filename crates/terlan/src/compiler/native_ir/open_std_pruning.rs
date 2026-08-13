//! Reachability pruning for uninstantiated generic standard-library exports.
//!
//! A source-level generic library declaration is a template, not an executable
//! native ABI. The application image retains it only when a concrete native
//! function reaches it, in which case ordinary admission emits the loud
//! diagnostic instead of silently manufacturing a dynamic compatibility ABI.

use std::collections::{HashMap, HashSet, VecDeque};

use crate::terlan_typeck::{CoreExportKind, CoreExpr, CoreIntrinsicId, CoreModule, CoreType};

#[cfg(test)]
#[path = "open_std_pruning_test.rs"]
#[cfg(test)]
mod open_std_pruning_test;

type FunctionKey = (String, String, usize);

/// Removes source router builders after the frontend has extracted their static plan.
///
/// Router values are compiler-owned metadata, not runtime values. Keeping a
/// `router/0` declaration or a grouped `Router -> Router` helper in the direct
/// AOT application closure incorrectly requires executable implementations of
/// `std.http.Router.new/get/post/...`. Route handlers, middleware, and error
/// callbacks have non-Router result types and remain ordinary native exports.
pub(super) fn prune_compile_time_router_builders(cores: &mut [CoreModule]) {
    for core in cores {
        let imports_router = core.imports.iter().any(|import| {
            import.module == "std.http.Router" || import.module.starts_with("std.http.Router.")
        });
        if !imports_router {
            continue;
        }
        let builders = core
            .functions
            .iter()
            .filter(|function| {
                router_result(function.core_return_type.as_ref(), &function.return_type)
            })
            .map(|function| (function.name.clone(), function.arity))
            .collect::<HashSet<_>>();
        core.functions
            .retain(|function| !builders.contains(&(function.name.clone(), function.arity)));
        core.exports.retain(|export| {
            let CoreExportKind::Function { arity } = export.kind else {
                return true;
            };
            !builders.contains(&(export.name.clone(), arity))
        });
    }
}

fn router_result(core: Option<&CoreType>, source: &str) -> bool {
    matches!(core, Some(CoreType::Named(name)) if name.rsplit('.').next() == Some("Router"))
        || source
            .split(['[', '<'])
            .next()
            .unwrap_or(source)
            .trim()
            .rsplit('.')
            .next()
            == Some("Router")
}

/// Retains the same-module function closure reachable from selected entrypoints.
///
/// Test compilation type-checks the complete source module, but its native
/// image only needs selected `@test` exports and the local helpers they call.
/// Compiler-only declarations such as router manifest builders therefore stay
/// visible to the frontend without becoming fake runtime dependencies.
pub(crate) fn prune_module_to_function_roots(core: &mut CoreModule, roots: &[&str]) {
    let providers = providers(std::slice::from_ref(core));
    let mut edges = HashMap::<FunctionKey, HashSet<FunctionKey>>::new();
    for function in &core.functions {
        let caller = (core.module.clone(), function.name.clone(), function.arity);
        let mut calls = HashSet::new();
        for clause in &function.clauses {
            if let Some(guard) = clause
                .guard
                .as_ref()
                .and_then(|guard| guard.core_expr.as_ref())
            {
                collect_calls(guard, core, &providers, &mut calls);
            }
            if let Some(body) = &clause.body.core_expr {
                collect_calls(body, core, &providers, &mut calls);
            }
        }
        edges.insert(caller, calls);
    }
    let root_names = roots.iter().copied().collect::<HashSet<_>>();
    let mut reachable = edges
        .keys()
        .filter(|(_, name, arity)| *arity == 0 && root_names.contains(name.as_str()))
        .cloned()
        .collect::<HashSet<_>>();
    let mut queue = reachable.iter().cloned().collect::<VecDeque<_>>();
    while let Some(caller) = queue.pop_front() {
        for callee in edges.get(&caller).into_iter().flatten() {
            if reachable.insert(callee.clone()) {
                queue.push_back(callee.clone());
            }
        }
    }
    core.functions.retain(|function| {
        reachable.contains(&(core.module.clone(), function.name.clone(), function.arity))
    });
    core.exports.retain(|export| {
        let CoreExportKind::Function { arity } = export.kind else {
            return true;
        };
        reachable.contains(&(core.module.clone(), export.name.clone(), arity))
    });
}

/// Retains one closed application function graph from exact executable roots.
///
/// All modules remain available for nominal types and constructor layouts, but
/// dead declarations are not manufactured into runtime exports. Frontend
/// checking has already validated the complete source set before this native
/// linker step.
pub(crate) fn prune_application_to_function_roots(
    cores: &mut [CoreModule],
    roots: &[(String, String, usize)],
) -> super::NativeIrResult<()> {
    let providers = providers(cores);
    for root in roots {
        if !providers.contains(root) {
            return Err(format!(
                "error[native_ir.application_root]: missing executable root `{}.{}/{}`",
                root.0, root.1, root.2
            )
            .into());
        }
    }
    let mut edges = HashMap::<FunctionKey, HashSet<FunctionKey>>::new();
    for core in cores.iter() {
        for function in &core.functions {
            let caller = (core.module.clone(), function.name.clone(), function.arity);
            let mut calls = HashSet::new();
            for clause in &function.clauses {
                if let Some(guard) = clause
                    .guard
                    .as_ref()
                    .and_then(|guard| guard.core_expr.as_ref())
                {
                    collect_calls(guard, core, &providers, &mut calls);
                }
                if let Some(body) = &clause.body.core_expr {
                    collect_calls(body, core, &providers, &mut calls);
                }
            }
            edges.insert(caller, calls);
        }
    }
    let mut reachable = roots.iter().cloned().collect::<HashSet<_>>();
    let mut queue = roots.iter().cloned().collect::<VecDeque<_>>();
    while let Some(caller) = queue.pop_front() {
        for callee in edges.get(&caller).into_iter().flatten() {
            if reachable.insert(callee.clone()) {
                queue.push_back(callee.clone());
            }
        }
    }
    for core in cores {
        core.functions.retain(|function| {
            reachable.contains(&(core.module.clone(), function.name.clone(), function.arity))
        });
        core.exports.retain(|export| {
            let CoreExportKind::Function { arity } = export.kind else {
                return true;
            };
            reachable.contains(&(core.module.clone(), export.name.clone(), arity))
        });
    }
    Ok(())
}

/// Removes `std.*` functions that have no path from an executable application
/// function. Reachable open generic functions remain present so normal native
/// admission rejects them loudly.
pub(super) fn prune_unreachable_open_std_functions(cores: &mut [CoreModule]) {
    let standard = cores
        .iter()
        .flat_map(|core| {
            core.functions
                .iter()
                .filter(|&_function| core.module.starts_with("std."))
                .map(|function| (core.module.clone(), function.name.clone(), function.arity))
        })
        .collect::<HashSet<_>>();
    let providers = providers(cores);
    let mut edges = HashMap::<FunctionKey, HashSet<FunctionKey>>::new();
    for core in cores.iter() {
        for function in &core.functions {
            let caller = (core.module.clone(), function.name.clone(), function.arity);
            let mut calls = HashSet::new();
            for clause in &function.clauses {
                if let Some(guard) = clause
                    .guard
                    .as_ref()
                    .and_then(|guard| guard.core_expr.as_ref())
                {
                    collect_calls(guard, core, &providers, &mut calls);
                }
                if let Some(body) = &clause.body.core_expr {
                    collect_calls(body, core, &providers, &mut calls);
                }
            }
            edges.insert(caller, calls);
        }
    }

    let executable_root_modules = cores
        .iter()
        .filter(|core| {
            core.functions
                .iter()
                .any(|function| function.name == "main" && function.arity == 0)
        })
        .map(|core| core.module.as_str())
        .collect::<HashSet<_>>();
    let explicit_root_modules = cores
        .iter()
        .filter(|core| {
            core.module != "std.test.Test"
                && (core.module.ends_with("Test")
                    || core.source.source_path.as_deref().is_some_and(|path| {
                        path.ends_with("Test.terl")
                            || path.starts_with("tests/")
                            || path.contains("/tests/")
                    }))
        })
        .map(|core| core.module.as_str())
        .collect::<HashSet<_>>();
    let root_modules = if !executable_root_modules.is_empty() {
        executable_root_modules
    } else if explicit_root_modules.is_empty() {
        let application_roots = cores
            .iter()
            .filter(|core| !core.module.starts_with("std.") || core.module.ends_with(".Main"))
            .map(|core| core.module.as_str())
            .collect::<HashSet<_>>();
        if application_roots.is_empty() {
            cores
                .iter()
                .map(|core| core.module.as_str())
                .collect::<HashSet<_>>()
        } else {
            application_roots
        }
    } else {
        explicit_root_modules
    };
    let mut reachable = edges
        .keys()
        .filter(|key| root_modules.contains(key.0.as_str()))
        .cloned()
        .collect::<HashSet<_>>();
    let mut queue = reachable.iter().cloned().collect::<VecDeque<_>>();
    while let Some(caller) = queue.pop_front() {
        for callee in edges.get(&caller).into_iter().flatten() {
            if reachable.insert(callee.clone()) {
                queue.push_back(callee.clone());
            }
        }
    }

    let prunable = if root_modules.iter().any(|module| module.ends_with("Test")) {
        edges.keys().cloned().collect::<HashSet<_>>()
    } else {
        standard
    };
    let pruned = prunable
        .difference(&reachable)
        .cloned()
        .collect::<HashSet<FunctionKey>>();
    for core in cores {
        core.functions.retain(|function| {
            !pruned.contains(&(core.module.clone(), function.name.clone(), function.arity))
        });
        core.exports.retain(|export| {
            let CoreExportKind::Function { arity } = export.kind else {
                return true;
            };
            !pruned.contains(&(core.module.clone(), export.name.clone(), arity))
        });
    }
}

/// Inventories every local and public imported provider used by call lookup.
fn providers(cores: &[CoreModule]) -> Vec<FunctionKey> {
    cores
        .iter()
        .flat_map(|core| {
            core.functions
                .iter()
                .map(|function| (core.module.clone(), function.name.clone(), function.arity))
        })
        .collect()
}

fn resolve_call(
    caller: &CoreModule,
    name: &str,
    arity: usize,
    providers: &[FunctionKey],
) -> Option<FunctionKey> {
    let imported = caller
        .imports
        .iter()
        .map(|import| import.module.as_str())
        .collect::<HashSet<_>>();
    resolve_scoped_call(&caller.module, &imported, name, arity, providers)
}

fn resolve_scoped_call(
    caller_module: &str,
    imported: &HashSet<&str>,
    name: &str,
    arity: usize,
    providers: &[FunctionKey],
) -> Option<FunctionKey> {
    if let Some(local) = providers
        .iter()
        .find(|(module, function, candidate_arity)| {
            module == caller_module && function == name && *candidate_arity == arity
        })
    {
        return Some(local.clone());
    }

    // A qualified call names exactly one provider. An unqualified call must
    // remain scoped to explicit imports; resolving it against the first
    // same-named function in the whole application makes reachability depend
    // on module ordering and can prune the real imported provider.
    if name.contains('.') {
        return providers
            .iter()
            .find(|(module, function, candidate_arity)| {
                format!("{module}.{function}") == name && *candidate_arity == arity
            })
            .cloned();
    }

    let mut matches = providers
        .iter()
        .filter(|(module, function, candidate_arity)| {
            (imported.contains(module.as_str())
                || imported.contains(format!("{module}.{function}").as_str()))
                && function == name
                && *candidate_arity == arity
        });
    let first = matches.next()?.clone();
    matches.next().is_none().then_some(first)
}

fn resolve_remote(
    module: &str,
    function: &str,
    arity: usize,
    providers: &[FunctionKey],
) -> Option<FunctionKey> {
    providers
        .iter()
        .find(|(candidate_module, candidate_function, candidate_arity)| {
            candidate_module == module
                && candidate_function == function
                && *candidate_arity == arity
        })
        .cloned()
}

fn collect_calls(
    expr: &CoreExpr,
    caller: &CoreModule,
    providers: &[FunctionKey],
    calls: &mut HashSet<FunctionKey>,
) {
    match expr {
        CoreExpr::Call { function, args } => {
            if let Some(target) = resolve_call(caller, function, args.len(), providers) {
                calls.insert(target);
            }
            collect_many(args, caller, providers, calls);
        }
        CoreExpr::RemoteCall {
            module,
            function,
            args,
        } => {
            let target = if module == "__receiver__" {
                resolve_call(caller, function, args.len(), providers)
            } else {
                resolve_remote(module, function, args.len(), providers)
            };
            if let Some(target) = target {
                calls.insert(target);
            }
            collect_many(args, caller, providers, calls);
        }
        CoreExpr::RemoteFunRef {
            module,
            function,
            arity,
        } => {
            if let Some(target) = resolve_remote(module, function, *arity, providers) {
                calls.insert(target);
            }
        }
        CoreExpr::Tuple(items) | CoreExpr::List(items) | CoreExpr::FixedArray(items) => {
            collect_many(items, caller, providers, calls)
        }
        CoreExpr::ListCons { head, tail }
        | CoreExpr::Index {
            base: head,
            index: tail,
        } => {
            collect_calls(head, caller, providers, calls);
            collect_calls(tail, caller, providers, calls);
        }
        CoreExpr::ListComprehension {
            expr,
            generators,
            guards,
            ..
        } => {
            collect_calls(expr, caller, providers, calls);
            for generator in generators {
                collect_calls(&generator.source, caller, providers, calls);
            }
            collect_many(guards, caller, providers, calls);
        }
        CoreExpr::Let { bindings, body } => {
            for binding in bindings {
                collect_calls(&binding.value, caller, providers, calls);
            }
            collect_calls(body, caller, providers, calls);
        }
        CoreExpr::Map(fields) => {
            for field in fields {
                collect_calls(&field.value, caller, providers, calls);
            }
        }
        CoreExpr::RecordConstruct { fields, .. } | CoreExpr::TemplateInstantiate { fields, .. } => {
            for field in fields {
                collect_calls(&field.value, caller, providers, calls);
            }
        }
        CoreExpr::FieldAccess { base, .. } | CoreExpr::RecordAccess { base, .. } => {
            collect_calls(base, caller, providers, calls)
        }
        CoreExpr::RecordUpdate { base, fields, .. } => {
            collect_calls(base, caller, providers, calls);
            for field in fields {
                collect_calls(&field.value, caller, providers, calls);
            }
        }
        CoreExpr::ConstructorChain { args, record, .. } => {
            collect_many(args, caller, providers, calls);
            collect_calls(record, caller, providers, calls);
        }
        CoreExpr::ConstructorCall { args, .. } => collect_many(args, caller, providers, calls),
        CoreExpr::MutableReceiverCall { receiver, args, .. } => {
            collect_calls(receiver, caller, providers, calls);
            collect_many(args, caller, providers, calls);
        }
        CoreExpr::FunctionCall { callee, args } => {
            collect_calls(callee, caller, providers, calls);
            collect_many(args, caller, providers, calls);
        }
        CoreExpr::Cast { expr, .. } => collect_calls(expr, caller, providers, calls),
        CoreExpr::Intrinsic(intrinsic) => {
            if matches!(intrinsic.id, CoreIntrinsicId::VmProcessEntry(_)) {
                if let [CoreExpr::Int(tag)] = intrinsic.args.as_slice() {
                    if *tag > 0 {
                        let function = format!("spawn_{tag}");
                        if let Some(target) = resolve_call(caller, &function, 0, providers) {
                            calls.insert(target);
                        }
                    }
                }
            }
            collect_many(&intrinsic.args, caller, providers, calls);
        }
        CoreExpr::SqlQuery { parameters, .. } => collect_many(parameters, caller, providers, calls),
        CoreExpr::Case { scrutinee, clauses } => {
            collect_calls(scrutinee, caller, providers, calls);
            collect_clauses(clauses, caller, providers, calls);
        }
        CoreExpr::Try {
            body,
            of_clauses,
            catch_clauses,
            after_clause,
        } => {
            collect_calls(body, caller, providers, calls);
            collect_clauses(of_clauses, caller, providers, calls);
            collect_clauses(catch_clauses, caller, providers, calls);
            if let Some(after) = after_clause {
                collect_calls(&after.trigger, caller, providers, calls);
                collect_calls(&after.body, caller, providers, calls);
            }
        }
        CoreExpr::If { clauses } => {
            for clause in clauses {
                collect_calls(&clause.condition, caller, providers, calls);
                collect_calls(&clause.body, caller, providers, calls);
            }
        }
        CoreExpr::Lam { body, .. } | CoreExpr::UnaryOp { operand: body, .. } => {
            collect_calls(body, caller, providers, calls)
        }
        CoreExpr::BinaryOp { left, right, .. } => {
            collect_calls(left, caller, providers, calls);
            collect_calls(right, caller, providers, calls);
        }
        CoreExpr::Int(_)
        | CoreExpr::Float(_)
        | CoreExpr::Binary(_)
        | CoreExpr::Atom(_)
        | CoreExpr::Var(_) => {}
    }
}

fn collect_many(
    expressions: &[CoreExpr],
    caller: &CoreModule,
    providers: &[FunctionKey],
    calls: &mut HashSet<FunctionKey>,
) {
    for expression in expressions {
        collect_calls(expression, caller, providers, calls);
    }
}

fn collect_clauses(
    clauses: &[crate::terlan_typeck::CoreCaseClause],
    caller: &CoreModule,
    providers: &[FunctionKey],
    calls: &mut HashSet<FunctionKey>,
) {
    for clause in clauses {
        if let Some(guard) = &clause.guard {
            collect_calls(guard, caller, providers, calls);
        }
        collect_calls(&clause.body, caller, providers, calls);
    }
}
