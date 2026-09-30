//! Source dependencies for calls whose interfaces were visible without imports.

use std::collections::BTreeSet;

use super::{
    merge_core_imports, parse_trait_instance_from_text, CoreExpr, CoreImport, CoreImportKind,
    CoreModule, ResolvedModule,
};

/// Retains callable providers, including candidates resolved after specialization.
/// Interface visibility alone supplies no executable body. Record dependencies
/// from checked interface metadata without a standard-module dispatch table.
pub(super) fn retain(core: &mut CoreModule, resolved: &ResolvedModule) {
    let mut providers = BTreeSet::new();
    let mut receivers = BTreeSet::new();
    let imported_modules = core
        .imports
        .iter()
        .map(|import| import.module.as_str())
        .collect::<BTreeSet<_>>();
    for function in &mut core.functions {
        for clause in &mut function.clauses {
            for summary in clause
                .guard
                .iter_mut()
                .chain(std::iter::once(&mut clause.body))
            {
                if let Some(expr) = &mut summary.core_expr {
                    crate::terlan_typeck::visit_core_expr_mut(expr, &mut |expr| match expr {
                        CoreExpr::RemoteCall {
                            module,
                            function,
                            args,
                            ..
                        } if module == "__receiver__" => {
                            receivers.insert((function.clone(), args.len()));
                        }
                        CoreExpr::RemoteCall { module, .. }
                            if resolved.interface_map.contains_key(module) =>
                        {
                            providers.insert(module.clone());
                        }
                        CoreExpr::MutableReceiverCall { method, args, .. } => {
                            receivers.insert((method.clone(), args.len() + 1));
                        }
                        _ => {}
                    });
                }
            }
        }
    }
    if !receivers.is_empty() {
        for interface in resolved.interface_map.values() {
            // Declared receiver methods follow the same import scope as
            // receiver dispatch. Unrelated visible interfaces are not runtime
            // dependencies merely because a method name and arity coincide.
            let declared_method = (imported_modules.contains(interface.module.as_str())
                || interface.module.starts_with("std.core."))
                && interface
                    .functions
                    .values()
                    .chain(interface.function_overloads.values().flatten())
                    .any(|function| {
                        function.public
                            && function.receiver_method
                            && receivers.contains(&(function.name.clone(), function.params.len()))
                    });
            let trait_method = interface
                .trait_conformances
                .iter()
                .filter(|conformance| conformance.public && !conformance.is_negative)
                .filter_map(|conformance| parse_trait_instance_from_text(&conformance.trait_ref))
                .filter_map(|instance| {
                    let (owner, name) = instance
                        .name
                        .rsplit_once('.')
                        .unwrap_or((&interface.module, &instance.name));
                    resolved.interface_map.get(owner)?.traits.get(name)
                })
                .any(|signature| {
                    signature.methods.iter().any(|(name, method)| {
                        receivers.contains(&(name.clone(), method.params.len()))
                    })
                });
            if declared_method || trait_method {
                providers.insert(interface.module.clone());
            }
        }
    }
    providers.remove(&core.module);
    merge_core_imports(
        &mut core.imports,
        providers
            .into_iter()
            .map(|module| CoreImport {
                module,
                kind: CoreImportKind::Module,
            })
            .collect(),
    );
}

#[cfg(test)]
#[path = "implementation_dependencies_test.rs"]
mod tests;
