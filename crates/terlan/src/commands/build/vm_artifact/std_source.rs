//! Reachable standard-library implementation closure for native applications.

use std::collections::{BTreeSet, VecDeque};
use std::path::{Path, PathBuf};

use crate::terlan_typeck::core_intrinsic_lowering::core_primitive_intrinsic;
use crate::terlan_typeck::{CoreImportKind, CoreModule};
use crate::CliState;

use super::super::BuildOneError;
use super::compile::{compile_vm_module, CompiledVmModule};

/// Compiles every reachable checked-in standard-library implementation.
///
/// Interface summaries establish types, but application images also need the
/// bodies of non-intrinsic helpers such as `std.system.Process.command/1`.
/// This traversal follows module imports transitively and removes only
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
        .filter(|import| import.kind == CoreImportKind::Module)
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
                .filter(|import| import.kind == CoreImportKind::Module)
                .map(|import| import.module.clone()),
        );
        remove_compiler_intrinsic_functions(&mut compiled.compiled.core);
        if !compiled.compiled.core.functions.is_empty() {
            modules.push(compiled);
        }
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

fn imported_std_source_path(module: &str, active_path: &Path) -> Option<PathBuf> {
    if !module.starts_with("std.") {
        return None;
    }
    let relative = PathBuf::from(format!("{}.terl", module.replace('.', "/")));
    let mut candidates = Vec::new();
    if let Some(root) = repository_root_from_std_path(active_path) {
        candidates.push(root.join(&relative));
    }
    if let Ok(current_dir) = std::env::current_dir() {
        candidates.push(current_dir.join(&relative));
    }
    if let Some(share_root) = crate::commands::release_layout::installed_share_root() {
        candidates.push(share_root.join(&relative));
    }
    candidates.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(&relative),
    );
    candidates.into_iter().find(|candidate| candidate.is_file())
}

fn repository_root_from_std_path(path: &Path) -> Option<PathBuf> {
    let mut current = path.parent();
    while let Some(directory) = current {
        if directory.file_name().and_then(|name| name.to_str()) == Some("std") {
            return directory.parent().map(Path::to_path_buf);
        }
        current = directory.parent();
    }
    None
}
