//! Native source evaluation follows resolved imports, not every project target.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;

use crate::CliState;

use super::super::BuildOneError;
use super::compile::{compile_vm_module, CompiledVmModule};

/// Compiles the selected application entry and its project-local import closure.
///
/// Interfaces for all project roots must already be prepared. Dependencies come
/// from checked CoreIR, including resolved grouped and type-only imports. Other
/// backends' modules may coexist in the project without entering this image.
pub(super) fn compile_application_closure(
    paths: &[PathBuf],
    entry: &str,
    state: &CliState,
) -> Result<Vec<CompiledVmModule>, BuildOneError> {
    let mut sources = BTreeMap::new();
    for path in paths {
        let path_text = path.to_string_lossy();
        let text = crate::support::read_file(&path_text)
            .map_err(|error| BuildOneError::Message(error.to_string()))?;
        let syntax = crate::formal_pipeline::parse_source_as_syntax_output(&path_text, &text)
            .map_err(|error| {
                BuildOneError::Message(format!("invalid application source: {error:?}"))
            })?;
        if sources.insert(syntax.module_name.clone(), path).is_some() {
            return Err(BuildOneError::Message(format!(
                "duplicate application module `{}`",
                syntax.module_name
            )));
        }
    }
    if !sources.contains_key(entry) {
        return Err(BuildOneError::Message(format!(
            "application entry module `{entry}` was not found"
        )));
    }
    let mut pending = VecDeque::from([entry.to_owned()]);
    let mut seen = BTreeSet::new();
    let mut modules = Vec::new();
    while let Some(name) = pending.pop_front() {
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(path) = sources.get(&name) else {
            // Standard-library implementations are loaded by the shared std closure.
            continue;
        };
        let module = compile_vm_module(&path.to_string_lossy(), state)?;
        // Native target checking has already rejected non-module asset imports.
        pending.extend(
            module
                .compiled
                .core
                .imports
                .iter()
                .map(|import| import.module.clone()),
        );
        modules.push(module);
    }
    modules.sort_by(|left, right| left.compiled.core.module.cmp(&right.compiled.core.module));
    Ok(modules)
}

#[cfg(test)]
#[path = "application_closure_test.rs"]
mod tests;
