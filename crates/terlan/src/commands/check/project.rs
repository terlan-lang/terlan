//! Project-wide checking with authoritative handwritten module interfaces.

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
    let source_graph = crate::formal_pipeline::interface_graph::SourceInterfaceGraph::collect(
        inputs
            .iter()
            .map(|(path, _, syntax)| (path.as_path(), syntax)),
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
