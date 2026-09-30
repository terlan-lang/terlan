//! Reachable standard-library implementation closure for native applications.

use std::collections::{BTreeSet, VecDeque};
use std::path::Path;

use crate::commands::source_libraries::imported_std_source_path;

use crate::terlan_typeck::core_intrinsic_lowering::core_primitive_intrinsic;
use crate::terlan_typeck::{CoreImportKind, CoreModule};
use crate::CliState;

use super::super::BuildOneError;
use super::compile::{compile_vm_module, CompiledVmModule};

/// Compiles every reachable checked-in standard-library implementation.
///
/// Interface summaries establish types, but application images also need the
/// bodies of non-intrinsic helpers such as `std.system.Process.command/1`.
/// This traversal follows value and type imports transitively: type-only
/// providers still own the canonical schemas used at managed boundaries.
/// It removes only
/// compiler-owned primitive declarations whose executable behavior is
/// supplied directly by lowering.
pub(super) fn compile_imported_std_source_modules(
    roots: &[&CoreModule],
    active_path: &Path,
    state: &CliState,
) -> Result<Vec<CompiledVmModule>, BuildOneError> {
    let mut modules = Vec::new();
    let mut seen = BTreeSet::new();
    let mut pending = roots
        .iter()
        .flat_map(|core| &core.imports)
        .filter(|import| {
            matches!(
                import.kind,
                CoreImportKind::Module | CoreImportKind::TypeModule
            )
        })
        .map(|import| import.module.clone())
        .collect::<VecDeque<_>>();
    let active_file = std::fs::canonicalize(active_path).ok();

    while let Some(module_name) = pending.pop_front() {
        if !seen.insert(module_name.clone()) {
            continue;
        }
        let Some(path) = imported_std_source_path(&module_name, active_path) else {
            continue;
        };
        if active_file.as_ref().is_some_and(|active| {
            std::fs::canonicalize(&path)
                .ok()
                .as_ref()
                .is_some_and(|candidate| candidate == active)
        }) {
            continue;
        }

        let path_text = path.to_string_lossy().into_owned();
        let mut compiled = compile_vm_module(&path_text, state)?;
        pending.extend(
            compiled
                .compiled
                .core
                .imports
                .iter()
                .filter(|import| {
                    matches!(
                        import.kind,
                        CoreImportKind::Module | CoreImportKind::TypeModule
                    )
                })
                .map(|import| import.module.clone()),
        );
        remove_compiler_intrinsic_functions(&mut compiled.compiled.core);
        // Intrinsic-only modules still own aliases and constructor signatures
        // needed by application-wide type normalization.
        modules.push(compiled);
    }
    modules.sort_by(|left, right| left.compiled.core.module.cmp(&right.compiled.core.module));
    Ok(modules)
}

fn remove_compiler_intrinsic_functions(module: &mut CoreModule) {
    let module_name = module.module.clone();
    module.functions.retain(|function| {
        core_primitive_intrinsic(&module_name, &function.name, function.arity).is_none()
    });
}

#[cfg(test)]
#[path = "std_source_test.rs"]
mod tests;
