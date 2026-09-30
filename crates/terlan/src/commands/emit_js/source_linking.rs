//! Links reachable source bodies before JavaScript emission, without API substitutions.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

use crate::compiler::native_ir::{
    prune_application_to_function_roots, resolve_selected_imports,
    resolve_typed_mutable_receiver_calls,
};
use crate::terlan_typeck::{visit_core_expr_mut, CoreExpr, CoreModule};

#[path = "source_linking_error.rs"]
mod error;
use error::SourceLinkError;

pub(super) fn link_libraries(
    root: &CoreModule,
    source_path: &Path,
) -> Result<CoreModule, SourceLinkError> {
    let mut cores = vec![root.clone()];
    let mut seen = BTreeSet::from([root.module.clone()]);
    let mut pending = root
        .imports
        .iter()
        .map(|import| import.module.clone())
        .collect::<VecDeque<_>>();
    while let Some(module) = pending.pop_front() {
        if !seen.insert(module.clone()) {
            continue;
        }
        let Some(path) =
            crate::commands::source_libraries::imported_std_source_path(&module, source_path)
        else {
            continue;
        };
        let source = std::fs::read_to_string(&path).map_err(|source| SourceLinkError::Read {
            path: path.clone(),
            source,
        })?;
        let compiled = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
            &path.to_string_lossy(),
            &source,
            crate::DiagnosticFormat::default(),
            None,
            crate::validation::native_policy::NativePolicy::default(),
            crate::validation::target_profile::TargetProfile::JsShared,
        )
        .map_err(|_| SourceLinkError::Compile {
            module: module.clone(),
        })?;
        if compiled.core.module != module {
            return Err(SourceLinkError::ModuleMismatch {
                expected: module,
                actual: compiled.core.module,
            });
        }
        pending.extend(
            compiled
                .core
                .imports
                .iter()
                .map(|import| import.module.clone()),
        );
        cores.push(compiled.core);
    }
    link_cores(cores)
}

fn link_cores(mut cores: Vec<CoreModule>) -> Result<CoreModule, SourceLinkError> {
    let roots = cores[0]
        .functions
        .iter()
        .filter(|function| function.public)
        .map(|function| {
            (
                cores[0].module.clone(),
                function.name.clone(),
                function.arity,
            )
        })
        .collect::<Vec<_>>();
    resolve_selected_imports(&mut cores)?;
    resolve_typed_mutable_receiver_calls(&mut cores)?;
    prune_application_to_function_roots(&mut cores, &roots)?;
    let root_name = cores[0].module.clone();
    let symbols = cores.iter().flat_map(|core| {
        let is_root = core.module == root_name;
        core.functions.iter().map(move |function| {
            let qualified = format!("{}.{}", core.module, function.name);
            let name = if is_root {
                function.name.clone()
            } else {
                format!(
                    "$library${}${}",
                    qualified.replace('.', "$"),
                    function.arity
                )
            };
            ((qualified, function.arity), name)
        })
    });
    let mut names = BTreeMap::new();
    let mut emitted_names = BTreeSet::new();
    for (identity, name) in symbols {
        if names.contains_key(&identity) || !emitted_names.insert(name.clone()) {
            return Err(SourceLinkError::DuplicateSymbol {
                function: identity.0,
                arity: identity.1,
            });
        }
        names.insert(identity, name);
    }
    for core in &mut cores {
        for function in &mut core.functions {
            for clause in &mut function.clauses {
                for summary in clause
                    .guard
                    .iter_mut()
                    .chain(std::iter::once(&mut clause.body))
                {
                    if let Some(expr) = &mut summary.core_expr {
                        visit_core_expr_mut(expr, &mut |expr| match expr {
                            CoreExpr::RemoteCall {
                                module,
                                function,
                                args,
                                type_args,
                            } => {
                                if let Some(name) =
                                    names.get(&(format!("{module}.{function}"), args.len()))
                                {
                                    *expr = CoreExpr::Call {
                                        function: name.clone(),
                                        args: std::mem::take(args),
                                        type_args: std::mem::take(type_args),
                                    };
                                }
                            }
                            CoreExpr::Call { function, args, .. } => {
                                let qualified = if function.contains('.') {
                                    function.clone()
                                } else {
                                    format!("{}.{}", core.module, function)
                                };
                                if let Some(name) = names.get(&(qualified, args.len())) {
                                    *function = name.clone();
                                }
                            }
                            _ => {}
                        });
                    }
                }
            }
            function.name =
                names[&(format!("{}.{}", core.module, function.name), function.arity)].clone();
            if core.module != root_name {
                function.public = false;
            }
        }
    }
    let mut linked = cores.remove(0);
    for provider in cores {
        linked.functions.extend(provider.functions);
    }
    Ok(linked)
}

#[cfg(test)]
#[path = "source_linking_test.rs"]
mod tests;
