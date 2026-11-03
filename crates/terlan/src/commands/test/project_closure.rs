//! Reachable project-source compilation for focused VM tests.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::terlan_typeck::{CoreImportKind, CoreModule};
use crate::validation::target_profile::TargetProfile;
use crate::CliState;

use super::execution::{TestCompiledModule, TestProjectContext};
use super::project_context::test_target_profile_options;

/// Compiles every project module for directory and coverage test suites.
pub(super) fn compile_project_source_core_modules(
    context: &TestProjectContext,
    state: &CliState,
    active_tests: &[PathBuf],
) -> Result<Vec<TestCompiledModule>, ExitCode> {
    let mut compiled_paths = BTreeSet::new();
    for active_test in active_tests {
        let active_test = active_test.canonicalize().map_err(|error| {
            eprintln!(
                "cannot resolve active test {}: {error}",
                active_test.display()
            );
            ExitCode::from(1)
        })?;
        compiled_paths.insert(active_test);
    }
    let mut modules = Vec::new();
    for root in &context.source_roots {
        let files = match crate::formal_pipeline::terlan_sources_in_dir(root) {
            Ok(files) => files,
            Err(message) => {
                eprintln!("{message}");
                return Err(ExitCode::from(1));
            }
        };
        for file in files {
            let canonical = file.canonicalize().map_err(|error| {
                eprintln!("cannot resolve project source {}: {error}", file.display());
                ExitCode::from(1)
            })?;
            if !compiled_paths.insert(canonical) {
                continue;
            }
            modules.push(compile_project_source_core_module(&file, state)?);
        }
    }
    Ok(modules)
}

/// Compiles only project modules reachable from focused test imports.
pub(super) fn compile_project_source_core_closure(
    context: &TestProjectContext,
    state: &CliState,
    active_tests: &[PathBuf],
    roots: &[&CoreModule],
) -> Result<Vec<TestCompiledModule>, ExitCode> {
    let mut active_paths = BTreeSet::new();
    for active_test in active_tests {
        active_paths.insert(active_test.canonicalize().map_err(|error| {
            eprintln!("cannot resolve active test: {error}");
            ExitCode::from(1)
        })?);
    }

    let mut sources = BTreeMap::new();
    for root in &context.source_roots {
        let files = crate::formal_pipeline::terlan_sources_in_dir(root).map_err(|message| {
            eprintln!("{message}");
            ExitCode::from(1)
        })?;
        for file in files {
            let module =
                crate::commands::source_layout::expected_module_name_for_source_path(root, &file)
                    .map_err(|message| {
                    eprintln!("{message}");
                    ExitCode::from(1)
                })?;
            if let Some(previous) = sources.insert(module.clone(), file.clone()) {
                if previous.canonicalize().ok() != file.canonicalize().ok() {
                    eprintln!(
                        "duplicate project source module `{module}` at {} and {}",
                        previous.display(),
                        file.display()
                    );
                    return Err(ExitCode::from(1));
                }
            }
        }
    }

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
    let mut seen = BTreeSet::new();
    let mut modules = Vec::new();
    while let Some(module) = pending.pop_front() {
        if !seen.insert(module.clone()) {
            continue;
        }
        let Some(file) = sources.get(&module) else {
            continue;
        };
        if file
            .canonicalize()
            .ok()
            .is_some_and(|path| active_paths.contains(&path))
        {
            continue;
        }
        let compiled = compile_project_source_core_module(file, state)?;
        pending.extend(
            compiled
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
        modules.push(compiled);
    }
    modules.sort_by(|left, right| left.core.module.cmp(&right.core.module));
    Ok(modules)
}

fn compile_project_source_core_module(
    file: &Path,
    state: &CliState,
) -> Result<TestCompiledModule, ExitCode> {
    let path = file.to_string_lossy().into_owned();
    let source = match crate::support::read_file(&path) {
        Ok(source) => source,
        Err(message) => {
            eprintln!("{message}");
            return Err(ExitCode::from(1));
        }
    };
    let compiled =
        crate::formal_pipeline::compile_syntax_module_through_phases_with_profile_options(
            &path,
            &source,
            state.diagnostic_format,
            state.cache_dir.as_deref(),
            state.native_policy,
            TargetProfile::Vm,
            test_target_profile_options(state, true),
        )?;
    Ok(TestCompiledModule {
        source_path: path,
        source_text: source,
        syntax: compiled.syntax_output,
        core: compiled.core,
    })
}
