//! Reachable project-source compilation for focused VM tests.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use crate::commands::build::vm_artifact::parallel_compile::{
    bounded_worker_limit, run_indexed_bounded, ParallelTaskError,
};
use crate::terlan_typeck::{CoreImportKind, CoreModule};
use crate::CliState;
use terlan_native_planning::{SourceModule, SourceModuleIndex};

use super::execution::{TestCompiledModule, TestProjectContext};
use super::project_context::test_target_profile_options;

/// Prepares interfaces only for project modules imported by one focused test.
///
/// Inputs:
/// - The project source roots and cache from `context`.
/// - The active compiler `state` and focused test source.
///
/// Output:
/// - Current interfaces for the test's transitive project import closure, or
///   the first source-layout, duplicate-module, parse, or interface error.
///
/// Transformation:
/// - Indexes module paths without parsing unused sources, prefers an explicit
///   `.terli` sibling, expands grouped imports into possible sibling providers,
///   traverses imports breadth-first, and prepares selected files grouped by
///   their original source-root order.
pub(super) fn prepare_project_source_interface_closure(
    context: &TestProjectContext,
    state: &CliState,
    test_path: &str,
    test_source: &str,
) -> Result<(), String> {
    let started = Instant::now();
    let syntax = crate::formal_pipeline::parse_source_as_syntax_output(test_path, test_source)
        .map_err(|error| format!("cannot parse focused test imports: {error:?}"))?;
    let mut sources = Vec::new();
    let mut discovered = 0_usize;
    for root in &context.compilation_source_roots {
        let files = crate::formal_pipeline::terlan_check_sources_in_dir(root)
            .map_err(|error| error.to_string())?;
        discovered += files.len();
        for file in files {
            let identity =
                crate::commands::source_layout::expected_module_name_for_source_path(root, &file)?;
            sources.push(SourceModule {
                identity,
                root: root.clone(),
                path: file,
            });
        }
    }

    let sources = SourceModuleIndex::new(sources).map_err(|error| error.to_string())?;
    let mut load_imports = |module: &SourceModule| {
        let path = module.path.to_string_lossy();
        let source = crate::support::read_file(&path)?;
        let syntax = crate::formal_pipeline::parse_source_as_syntax_output(&path, &source)
            .map_err(|error| {
                format!(
                    "cannot parse focused import `{}`: {error:?}",
                    module.identity
                )
            })?;
        Ok::<_, String>(syntax_module_imports(&syntax))
    };
    let selected_sources =
        sources.transitive_closure(syntax_module_imports(&syntax), &mut load_imports)?;
    let mut selected = BTreeMap::<PathBuf, Vec<PathBuf>>::new();
    for module in selected_sources {
        selected.entry(module.root).or_default().push(module.path);
    }
    let selected_count = selected.values().map(Vec::len).sum::<usize>();
    for root in &context.compilation_source_roots {
        let Some(files) = selected.remove(root) else {
            continue;
        };
        crate::commands::build::prepare_source_interfaces(root, state, files)?;
    }
    if state.timings {
        eprintln!(
            "terlc timing: test.interface-closure: discovered={discovered} reachable={selected_count} elapsed={}ms",
            started.elapsed().as_millis()
        );
    }
    Ok(())
}

fn syntax_module_imports(syntax: &crate::terlan_syntax::SyntaxModuleOutput) -> Vec<String> {
    syntax
        .declarations
        .iter()
        .flat_map(|declaration| {
            let crate::terlan_syntax::SyntaxDeclarationPayload::Import {
                import_kind,
                module_name,
                items,
                is_selected,
                ..
            } = &declaration.payload
            else {
                return Vec::new();
            };
            if *import_kind != crate::terlan_syntax::SyntaxImportKind::Module {
                return Vec::new();
            }
            let mut providers = vec![crate::terlan_syntax::syntax_module_import_identity(
                module_name,
                items,
                *is_selected,
            )];
            if *is_selected {
                providers.extend(
                    items
                        .iter()
                        .map(|item| format!("{module_name}.{}", item.name)),
                );
            }
            providers
        })
        .collect()
}

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
    let mut files_to_compile = Vec::new();
    for root in &context.compilation_source_roots {
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
            files_to_compile.push(file);
        }
    }
    let started = Instant::now();
    if state.timings {
        eprintln!(
            "terlc timing: test.project-complete.start: modules={} workers={}",
            files_to_compile.len(),
            bounded_worker_limit()
        );
    }
    let result = run_indexed_bounded(&files_to_compile, bounded_worker_limit(), |file| {
        compile_project_source_core_module(file, state)
    });
    if state.timings {
        eprintln!(
            "terlc timing: test.project-complete.finish: modules={} elapsed={}ms",
            files_to_compile.len(),
            started.elapsed().as_millis()
        );
    }
    match result {
        Ok(modules) => Ok(modules),
        Err(ParallelTaskError::Task(exit_code)) => Err(exit_code),
        Err(ParallelTaskError::WorkerPanicked) => {
            eprintln!("complete test project compilation worker panicked");
            Err(ExitCode::from(1))
        }
    }
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
    for root in &context.compilation_source_roots {
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
            let source = SourceModule {
                identity: module.clone(),
                root: root.clone(),
                path: file.clone(),
            };
            if let Some(previous) = sources.insert(module.clone(), source) {
                if previous.path.canonicalize().ok() != file.canonicalize().ok() {
                    eprintln!(
                        "duplicate project source module `{module}` at {} and {}",
                        previous.path.display(),
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
    while !pending.is_empty() {
        let mut frontier = Vec::new();
        while let Some(module) = pending.pop_front() {
            if !seen.insert(module.clone()) {
                continue;
            }
            let Some(source) = sources.get(&module) else {
                continue;
            };
            if source
                .path
                .canonicalize()
                .ok()
                .is_some_and(|path| active_paths.contains(&path))
            {
                continue;
            }
            frontier.push(source.clone());
        }
        prepare_frontier_interfaces(&frontier, state)?;
        let started = Instant::now();
        if state.timings {
            eprintln!(
                "terlc timing: test.project-frontier.start: modules={} workers={}",
                frontier.len(),
                bounded_worker_limit()
            );
        }
        let result = run_indexed_bounded(&frontier, bounded_worker_limit(), |source| {
            compile_project_source_core_module(&source.path, state)
        });
        if state.timings {
            eprintln!(
                "terlc timing: test.project-frontier.finish: modules={} elapsed={}ms",
                frontier.len(),
                started.elapsed().as_millis()
            );
        }
        let compiled = match result {
            Ok(compiled) => compiled,
            Err(ParallelTaskError::Task(exit_code)) => return Err(exit_code),
            Err(ParallelTaskError::WorkerPanicked) => {
                eprintln!("focused test project compilation worker panicked");
                return Err(ExitCode::from(1));
            }
        };
        for compiled in compiled {
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
    }
    modules.sort_by(|left, right| left.core.module.cmp(&right.core.module));
    Ok(modules)
}

/// Refreshes every provider interface before one parallel Core frontier starts.
///
/// Inputs:
/// - `frontier`: newly resolved source providers grouped by source root.
/// - `state`: the focused-test cache and incremental compiler policy.
///
/// Output:
/// - Success after every current interface is published, or the first
///   deterministic preparation failure.
///
/// Transformation:
/// - Prepares source-root groups serially before worker admission so a consumer
///   can never race a provider replacing an obsolete cached interface.
fn prepare_frontier_interfaces(
    frontier: &[SourceModule],
    state: &CliState,
) -> Result<(), ExitCode> {
    let mut by_root = BTreeMap::<PathBuf, Vec<PathBuf>>::new();
    for source in frontier {
        by_root
            .entry(source.root.clone())
            .or_default()
            .push(source.path.clone());
    }
    for (root, files) in by_root {
        if let Err(message) = crate::commands::build::prepare_source_interfaces(&root, state, files)
        {
            eprintln!("focused test frontier interface preparation failed: {message}");
            return Err(ExitCode::from(1));
        }
    }
    Ok(())
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
    let compiled = crate::commands::build::vm_artifact::compile_checked_vm_artifacts(
        &path,
        &source,
        state,
        test_target_profile_options(state, true),
    )?;
    let native_debug =
        crate::commands::build::vm_artifact::native_debug::NativeDebugInput::capture_owned(
            path,
            source,
            &compiled.core,
            &compiled.syntax_output,
        );
    Ok(TestCompiledModule {
        core: compiled.core,
        native_debug,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn context(root: &Path, cache: &Path) -> TestProjectContext {
        TestProjectContext {
            project_root: root.to_path_buf(),
            cache_dir: cache.to_path_buf(),
            compilation_source_roots: vec![root.to_path_buf()],
            coverage_source_roots: vec![root.to_path_buf()],
            native_helper_environment: Vec::new(),
            native_helper_candidates: Vec::new(),
        }
    }

    fn state(cache: &Path) -> CliState {
        CliState {
            cache_dir: Some(cache.to_path_buf()),
            incremental: true,
            ..CliState::default()
        }
    }

    #[test]
    fn focused_interface_preparation_visits_only_the_import_closure() {
        let fixture = tempfile::tempdir().expect("focused closure fixture");
        let root = fixture.path().join("src");
        let cache = fixture.path().join("cache");
        fs::create_dir_all(root.join("app")).expect("source layout");
        fs::write(
            root.join("app/Reachable.terl"),
            "module app.Reachable.\nimport app.Transitive.\npub value(): Int -> Transitive.value().\n",
        )
        .expect("reachable source");
        fs::write(
            root.join("app/Transitive.terl"),
            "module app.Transitive.\npub value(): Int -> 7.\n",
        )
        .expect("transitive source");
        fs::write(
            root.join("app/Unused.terl"),
            "module app.Unused.\npub broken(: Int -> 0.\n",
        )
        .expect("invalid unused source");
        fs::write(
            root.join("app/Contract.terl"),
            "module app.Contract.\npub broken(: Int -> 0.\n",
        )
        .expect("hidden implementation");
        fs::write(
            root.join("app/Contract.terli"),
            "module app.Contract.\npub value(): Int.\n",
        )
        .expect("public interface");
        let source = concat!(
            "module app.FocusedTest.\n",
            "import app.Reachable.\n",
            "import app.Contract.\n",
            "@test\npub works(): Bool -> Reachable.value() == 7.\n",
        );

        prepare_project_source_interface_closure(
            &context(&root, &cache),
            &state(&cache),
            "FocusedTest.terl",
            source,
        )
        .expect("prepare focused closure");

        assert!(cache.join("app.Reachable.typi").is_file());
        assert!(cache.join("app.Transitive.typi").is_file());
        assert!(cache.join("app.Contract.typi").is_file());
        assert!(!cache.join("app.Unused.typi").exists());
    }

    #[test]
    fn focused_interface_preparation_rejects_duplicate_modules() {
        let fixture = tempfile::tempdir().expect("duplicate closure fixture");
        let left = fixture.path().join("left");
        let right = fixture.path().join("right");
        let cache = fixture.path().join("cache");
        for root in [&left, &right] {
            fs::create_dir_all(root.join("app")).expect("source layout");
            fs::write(
                root.join("app/Shared.terl"),
                "module app.Shared.\npub value(): Int -> 1.\n",
            )
            .expect("duplicate source");
        }
        let mut context = context(&left, &cache);
        context.compilation_source_roots.push(right);
        let error = prepare_project_source_interface_closure(
            &context,
            &state(&cache),
            "FocusedTest.terl",
            "module app.FocusedTest.\nimport app.Shared.\n",
        )
        .expect_err("duplicate module must fail");
        assert!(error.contains("duplicate project source module `app.Shared`"));
    }

    #[test]
    fn focused_interface_preparation_expands_grouped_sibling_imports() {
        let fixture = tempfile::tempdir().expect("grouped import fixture");
        let root = fixture.path().join("src");
        let cache = fixture.path().join("cache");
        fs::create_dir_all(root.join("app")).expect("source layout");
        fs::write(
            root.join("app/Child.terl"),
            "module app.Child.\npub value(): Int -> 7.\n",
        )
        .expect("child source");
        let source = concat!(
            "module app.FocusedTest.\n",
            "import app.{Child}.\n",
            "@test\npub works(): Bool -> Child.value() == 7.\n",
        );

        prepare_project_source_interface_closure(
            &context(&root, &cache),
            &state(&cache),
            "FocusedTest.terl",
            source,
        )
        .expect("prepare grouped sibling closure");

        assert!(cache.join("app.Child.typi").is_file());
    }

    #[test]
    fn frontier_interfaces_refresh_all_roots_before_compilation() {
        let fixture = tempfile::tempdir().expect("frontier fixture");
        let cache = fixture.path().join("cache");
        let left = fixture.path().join("left");
        let right = fixture.path().join("right");
        fs::create_dir_all(left.join("app")).expect("left source layout");
        fs::create_dir_all(right.join("lib")).expect("right source layout");
        let left_file = left.join("app/Provider.terl");
        let right_file = right.join("lib/Provider.terl");
        fs::write(
            &left_file,
            "module app.Provider.\npub current_left(): Int -> 1.\n",
        )
        .expect("left provider");
        fs::write(
            &right_file,
            "module lib.Provider.\npub current_right(): Int -> 2.\n",
        )
        .expect("right provider");
        let frontier = vec![
            SourceModule {
                identity: "app.Provider".to_string(),
                root: left,
                path: left_file,
            },
            SourceModule {
                identity: "lib.Provider".to_string(),
                root: right,
                path: right_file,
            },
        ];

        prepare_frontier_interfaces(&frontier, &state(&cache)).expect("prepare frontier");

        let left_interface =
            fs::read_to_string(cache.join("app.Provider.typi")).expect("left prepared interface");
        let right_interface =
            fs::read_to_string(cache.join("lib.Provider.typi")).expect("right prepared interface");
        assert!(left_interface.contains("current_left"));
        assert!(right_interface.contains("current_right"));
    }

    #[test]
    fn frontier_interface_failure_stops_worker_admission() {
        let fixture = tempfile::tempdir().expect("invalid frontier fixture");
        let cache = fixture.path().join("cache");
        let root = fixture.path().join("src");
        fs::create_dir_all(root.join("app")).expect("source layout");
        let file = root.join("app/Broken.terl");
        fs::write(&file, "module app.Broken.\npub broken(: Int -> 0.\n").expect("invalid source");
        let frontier = vec![SourceModule {
            identity: "app.Broken".to_string(),
            root,
            path: file,
        }];

        assert_eq!(
            prepare_frontier_interfaces(&frontier, &state(&cache)),
            Err(ExitCode::from(1))
        );
    }
}
