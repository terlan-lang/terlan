//! Project-wide checking with authoritative handwritten module interfaces.

use std::collections::HashSet;
use std::path::Path;
use std::process::ExitCode;

use crate::commands::artifacts::{
    collect_syntax_dependency_hashes, fingerprint, DependencyManifest,
};
use crate::commands::source_layout::validate_module_layout;
use crate::formal_pipeline::{
    compile_syntax_module_with_interfaces, parse_source_as_syntax_output,
    terlan_check_sources_in_dir,
};
use crate::terlan_syntax::SyntaxSourceKind;
use crate::validation::target_profile::{TargetInferenceInput, TargetProfileCheckOptions};
use crate::CliState;

use super::command::{effective_check_target_profile_from_input, finish_check};

/// Checks contracts before clients and implementations. In interface-only mode,
/// paired implementation files are not read, parsed, or required to exist.
pub(super) fn run(
    path: &str,
    state: CliState,
    manifests: Option<&Path>,
    interfaces_only: bool,
) -> ExitCode {
    let root = Path::new(path);
    let cache_dir = state
        .cache_dir
        .clone()
        .unwrap_or_else(|| root.join(".terlan"));
    let files = match terlan_check_sources_in_dir(root) {
        Ok(files) => files,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    if !interfaces_only
        && !files.iter().any(|file| {
            file.extension()
                .is_some_and(|extension| extension == "terli")
        })
        && !std::fs::read_dir(&cache_dir).is_ok_and(|entries| {
            entries.flatten().any(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "terli")
            })
        })
    {
        return super::directory::run_check_dir(path, state, manifests);
    }
    let mut inputs = Vec::new();
    for file in files {
        if interfaces_only
            && file
                .extension()
                .is_some_and(|extension| extension == "terl")
            && file.with_extension("terli").is_file()
        {
            continue;
        }
        let source = match crate::support::read_file(&file.to_string_lossy()) {
            Ok(source) => source,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        };
        let syntax = match parse_source_as_syntax_output(&file.to_string_lossy(), &source) {
            Ok(syntax) => syntax,
            Err(_) => {
                let result = compile_syntax_module_with_interfaces(
                    &file.to_string_lossy(),
                    &source,
                    state.diagnostic_format,
                    crate::formal_pipeline::InterfaceInputs {
                        cache_dir: None,
                        source_graph: None,
                    },
                    state.native_policy,
                    state.target_profile,
                    TargetProfileCheckOptions::default(),
                );
                let module = crate::commands::source_layout::expected_module_name_for_source_path(
                    root, &file,
                )
                .unwrap_or_else(|_| "unparsed".to_string());
                let manifest = manifests
                    .map(|root| super::directory::check_dir_phase_manifest_path(root, &module));
                return finish_check(
                    &file.to_string_lossy(),
                    fingerprint(source.as_bytes()),
                    &state,
                    manifest.as_deref(),
                    result,
                );
            }
        };
        if let Err(message) = validate_module_layout(root, &file, &syntax.module_name) {
            crate::support::emit_diagnostic(
                "module_layout_error",
                &message,
                &file.to_string_lossy(),
                0,
                0,
                state.diagnostic_format,
            );
            return ExitCode::from(1);
        }
        inputs.push((file, source, syntax));
    }
    let dependencies = match super::dependencies::source_interfaces(root) {
        Ok(dependencies) => dependencies,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    let source_graph = crate::formal_pipeline::interface_graph::SourceInterfaceGraph::collect(
        inputs
            .iter()
            .map(|(path, _, syntax)| (path.as_path(), syntax))
            .chain(
                dependencies
                    .iter()
                    .map(|(path, syntax)| (path.as_path(), syntax)),
            ),
        Some(&cache_dir),
    );
    let target = match effective_check_target_profile_from_input(
        TargetInferenceInput::from_syntax_modules(inputs.iter().map(|(_, _, syntax)| syntax)),
        state.target_profile,
    ) {
        Ok(target) => target,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    if let Some(directory) = manifests.filter(|path| path.extension().is_none()) {
        if let Err(error) = std::fs::create_dir_all(directory) {
            eprintln!("cannot create phase manifest directory: {error}");
            return ExitCode::from(1);
        }
    }
    // Check declarations before trusting them to typecheck clients. No generated
    // implementation cache is published by this project-design operation.
    inputs.sort_by_key(|(path, _, syntax)| {
        (
            syntax.source_kind != SyntaxSourceKind::Interface,
            path.clone(),
        )
    });
    let mut failed = false;
    let mut publications = Vec::new();
    for (file, source, syntax) in inputs {
        let result = compile_syntax_module_with_interfaces(
            &file.to_string_lossy(),
            &source,
            state.diagnostic_format,
            crate::formal_pipeline::InterfaceInputs {
                cache_dir: Some(&cache_dir),
                source_graph: Some(&source_graph),
            },
            state.native_policy,
            target,
            TargetProfileCheckOptions::default(),
        );
        if !(interfaces_only
            || syntax.source_kind == SyntaxSourceKind::Module
                && file.with_extension("terli").is_file())
        {
            let interface = result
                .checked_interface
                .as_ref()
                .map(|checked| &checked.interface)
                .or_else(|| {
                    result
                        .artifacts
                        .as_ref()
                        .map(|checked| &checked.core.interface)
                });
            if let Some(interface) = interface {
                let manifest = DependencyManifest {
                    module: syntax.module_name.clone(),
                    syntax_contract_identity: syntax.syntax_contract.clone(),
                    source_hash: fingerprint(source.as_bytes()),
                    interface_hash: fingerprint(
                        interface.to_terlan_interface_type_text().as_bytes(),
                    ),
                    interface_doc_hash: fingerprint(
                        interface.to_terlan_interface_doc_text().as_bytes(),
                    ),
                    dependencies: collect_syntax_dependency_hashes(
                        &syntax,
                        &source_graph.interfaces,
                        Some(&file),
                        None,
                    ),
                };
                publications.push((
                    syntax.module_name.clone(),
                    interface.to_terlan_interface_text(),
                    manifest.encode(),
                ));
            }
        }
        let manifest = manifests.map(|root| {
            let name = if syntax.source_kind == SyntaxSourceKind::Interface {
                format!("{}.interface", syntax.module_name)
            } else {
                syntax.module_name.clone()
            };
            super::directory::check_dir_phase_manifest_path(root, &name)
        });
        if finish_check(
            &file.to_string_lossy(),
            fingerprint(source.as_bytes()),
            &state,
            manifest.as_deref(),
            result,
        ) != ExitCode::SUCCESS
        {
            failed = true;
        }
    }
    if failed {
        ExitCode::from(1)
    } else {
        if !interfaces_only {
            let cache = state
                .cache_dir
                .as_ref()
                .cloned()
                .unwrap_or_else(|| root.join(".terlan"));
            if let Err(error) = std::fs::create_dir_all(&cache) {
                eprintln!("cannot create interface cache: {error}");
                return ExitCode::from(1);
            }
            for (module, interface, manifest) in publications {
                for (suffix, contents) in [("typi", interface), ("typi.deps", manifest)] {
                    if let Err(error) = crate::support::write_if_changed_or_forced(
                        &cache.join(format!("{module}.{suffix}")),
                        contents.as_bytes(),
                        state.incremental,
                    ) {
                        eprintln!("cannot publish checked interface: {error}");
                        return ExitCode::from(1);
                    }
                }
            }
        }
        ExitCode::SUCCESS
    }
}

/// Checks selected project-owned files against one shared project source graph.
///
/// All configured source roots are parsed once to recover authoritative import
/// interfaces. Only the selected changed modules and scripts enter type checking,
/// Core lowering, and phase-manifest emission.
pub(super) fn run_selected(
    project_root: &Path,
    selected_paths: &[String],
    state: CliState,
    manifests: Option<&Path>,
) -> ExitCode {
    let manifest_path = project_root.join("terlan.toml");
    let manifest =
        match crate::commands::build::project_manifest::read_project_manifest(&manifest_path) {
            Ok(manifest) => manifest,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        };
    let cache_dir = state
        .cache_dir
        .clone()
        .unwrap_or_else(|| project_root.join(".terlan"));
    let selected = match selected_paths
        .iter()
        .map(|path| {
            Path::new(path)
                .canonicalize()
                .map_err(|error| format!("cannot resolve selected check path {path}: {error}"))
        })
        .collect::<Result<HashSet<_>, _>>()
    {
        Ok(selected) => selected,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };

    let mut inputs = Vec::new();
    let mut source_roots = Vec::new();
    for relative_root in &manifest.source_roots {
        let root = project_root.join(relative_root);
        let root = match root.canonicalize() {
            Ok(root) => root,
            Err(error) => {
                eprintln!(
                    "cannot resolve project source root {}: {error}",
                    root.display()
                );
                return ExitCode::from(1);
            }
        };
        let files = match terlan_check_sources_in_dir(&root) {
            Ok(files) => files,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        };
        for file in files {
            let source = match crate::support::read_file(&file.to_string_lossy()) {
                Ok(source) => source,
                Err(message) => {
                    eprintln!("{message}");
                    return ExitCode::from(1);
                }
            };
            let syntax = match parse_source_as_syntax_output(&file.to_string_lossy(), &source) {
                Ok(syntax) => syntax,
                Err(error) => {
                    eprintln!("cannot parse project source {}: {error:?}", file.display());
                    return ExitCode::from(1);
                }
            };
            if let Err(message) = validate_module_layout(&root, &file, &syntax.module_name) {
                crate::support::emit_diagnostic(
                    "module_layout_error",
                    &message,
                    &file.to_string_lossy(),
                    0,
                    0,
                    state.diagnostic_format,
                );
                return ExitCode::from(1);
            }
            inputs.push((file, source, syntax));
        }
        source_roots.push(root);
    }

    let known_paths = inputs
        .iter()
        .map(|(path, _, _)| path.clone())
        .collect::<HashSet<_>>();
    for selected_path in selected.iter().filter(|path| !known_paths.contains(*path)) {
        let source = match crate::support::read_file(&selected_path.to_string_lossy()) {
            Ok(source) => source,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        };
        let syntax = match parse_source_as_syntax_output(&selected_path.to_string_lossy(), &source)
        {
            Ok(syntax) => syntax,
            Err(error) => {
                eprintln!(
                    "cannot parse selected source {}: {error:?}",
                    selected_path.display()
                );
                return ExitCode::from(1);
            }
        };
        inputs.push((selected_path.clone(), source, syntax));
    }

    let dependencies = match source_roots.first() {
        Some(root) => match super::dependencies::source_interfaces(root) {
            Ok(dependencies) => dependencies,
            Err(message) => {
                eprintln!("{message}");
                return ExitCode::from(1);
            }
        },
        None => Vec::new(),
    };
    let source_graph = crate::formal_pipeline::interface_graph::SourceInterfaceGraph::collect(
        inputs
            .iter()
            .map(|(path, _, syntax)| (path.as_path(), syntax))
            .chain(
                dependencies
                    .iter()
                    .map(|(path, syntax)| (path.as_path(), syntax)),
            ),
        Some(&cache_dir),
    );
    let selected_inputs = inputs
        .iter()
        .filter(|(path, _, _)| selected.contains(path))
        .collect::<Vec<_>>();
    if selected_inputs.len() != selected.len() {
        eprintln!("not every selected check path belongs to the prepared project graph");
        return ExitCode::from(1);
    }
    let target = match effective_check_target_profile_from_input(
        TargetInferenceInput::from_syntax_modules(
            selected_inputs.iter().map(|(_, _, syntax)| syntax),
        ),
        state.target_profile,
    ) {
        Ok(target) => target,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(1);
        }
    };
    if let Some(directory) = manifests {
        if let Err(error) = std::fs::create_dir_all(directory) {
            eprintln!("cannot create phase manifest directory: {error}");
            return ExitCode::from(1);
        }
    }

    let mut selected_inputs = selected_inputs;
    selected_inputs.sort_by_key(|(path, _, syntax)| {
        (
            syntax.source_kind != SyntaxSourceKind::Interface,
            (*path).clone(),
        )
    });
    let worker_count = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .min(4)
        .min(selected_inputs.len().max(1));
    let compiled = std::thread::scope(|scope| {
        let mut workers = Vec::new();
        for worker in 0..worker_count {
            let selected_inputs = &selected_inputs;
            let source_graph = &source_graph;
            let cache_dir = &cache_dir;
            let state = &state;
            workers.push(scope.spawn(move || {
                let mut compiled = Vec::new();
                for index in (worker..selected_inputs.len()).step_by(worker_count) {
                    let (file, source, _) = selected_inputs[index];
                    compiled.push((
                        index,
                        compile_syntax_module_with_interfaces(
                            &file.to_string_lossy(),
                            source,
                            state.diagnostic_format,
                            crate::formal_pipeline::InterfaceInputs {
                                cache_dir: Some(cache_dir),
                                source_graph: Some(source_graph),
                            },
                            state.native_policy,
                            target,
                            TargetProfileCheckOptions::default(),
                        ),
                    ));
                }
                compiled
            }));
        }
        let mut compiled = Vec::with_capacity(selected_inputs.len());
        for worker in workers {
            compiled.extend(worker.join().expect("selected check worker"));
        }
        compiled.sort_by_key(|(index, _)| *index);
        compiled
    });
    for ((file, source, syntax), (_, result)) in selected_inputs.into_iter().zip(compiled) {
        let manifest = manifests.map(|root| {
            let name = if file
                .extension()
                .is_some_and(|extension| extension == "terls")
            {
                file.file_stem()
                    .and_then(|stem| stem.to_str())
                    .unwrap_or("script")
                    .to_string()
            } else if syntax.source_kind == SyntaxSourceKind::Interface {
                format!("{}.interface", syntax.module_name)
            } else {
                syntax.module_name.clone()
            };
            super::directory::check_dir_phase_manifest_path(root, &name)
        });
        if finish_check(
            &file.to_string_lossy(),
            fingerprint(source.as_bytes()),
            &state,
            manifest.as_deref(),
            result,
        ) != ExitCode::SUCCESS
        {
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}
