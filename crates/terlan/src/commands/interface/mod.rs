use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use crate::commands::artifacts::{
    collect_syntax_dependency_hashes, fingerprint, DependencyManifest,
};
use crate::terlan_hir::syntax_module_output_to_interface;
use crate::terlan_syntax::SyntaxSourceKind;
use crate::validation::phase_manifest::current_syntax_contract_identity;
use crate::validation::target_profile::{
    infer_target_profile_from_typed_evidence, TargetInferenceInput, TargetProfile,
    TargetProfileCheckOptions,
};

use crate::{support::write_if_changed_or_forced, CliState};

#[cfg(test)]
#[path = "interface_test.rs"]
mod tests;

/// Executes the `interface` CLI command.
///
/// Inputs:
/// - `args`: command-local arguments after the `interface` verb.
/// - `state`: parsed global CLI state, including output directory,
///   incremental-write mode, and diagnostic format.
///
/// Output:
/// - `ExitCode::SUCCESS` when interface output is written successfully.
/// - `ExitCode::from(2)` when command-local arguments are malformed.
/// - `ExitCode::from(1)` on read, parse, contract validation, directory, or
///   write failures.
///
/// Transformation:
/// - Collects one or more formal sources, validates `.terli` declarations and
///   contracted implementations, and writes `<module>.typi` plus
///   `<module>.typi.deps`. Batches of uncontracted implementations retain
///   syntax-based extraction for standard-library bootstrapping.
pub(crate) fn run(args: &[String], state: &CliState) -> ExitCode {
    if args.is_empty() {
        eprintln!("missing path argument");
        crate::print_usage();
        return ExitCode::from(2);
    }

    let mut inputs = Vec::with_capacity(args.len());
    let mut module_names = HashSet::new();
    let mut contracts = HashMap::new();
    for path in args {
        let source = match crate::support::read_file(path) {
            Ok(source) => source,
            Err(message) => {
                eprintln!("{}", message);
                return ExitCode::from(1);
            }
        };
        let syntax_output =
            match crate::formal_pipeline::parse_source_as_syntax_output(path, &source) {
                Ok(output) => output,
                Err(crate::terlan_syntax::ebnf::EbnfCompileError::Parse(message, span)) => {
                    crate::support::emit_diagnostic(
                        "parse_error",
                        &message,
                        path,
                        span.start,
                        span.end,
                        state.diagnostic_format,
                    );
                    return ExitCode::from(1);
                }
                Err(crate::terlan_syntax::ebnf::EbnfCompileError::Serialize(message)) => {
                    eprintln!("{}", message);
                    return ExitCode::from(1);
                }
            };
        if !module_names.insert(syntax_output.module_name.clone()) {
            eprintln!(
                "duplicate interface module `{}` in batch",
                syntax_output.module_name
            );
            return ExitCode::from(1);
        }
        let interface = syntax_module_output_to_interface(&syntax_output);
        if syntax_output.source_kind == SyntaxSourceKind::Interface {
            contracts.insert(syntax_output.module_name.clone(), syntax_output.clone());
        } else if Path::new(path)
            .extension()
            .is_some_and(|extension| extension == "terl")
        {
            match crate::formal_pipeline::interface_contract::read_declared_interface(Path::new(
                path,
            )) {
                Ok(Some(contract)) => {
                    if contract.module_name != syntax_output.module_name {
                        eprintln!(
                            "interface_contract: {} declares module `{}`, expected `{}`",
                            Path::new(path).with_extension("terli").display(),
                            contract.module_name,
                            syntax_output.module_name
                        );
                        return ExitCode::from(1);
                    }
                    contracts.insert(syntax_output.module_name.clone(), contract);
                }
                Ok(None) => {}
                Err(message) => {
                    eprintln!("{message}");
                    return ExitCode::from(1);
                }
            }
        }
        inputs.push((path, source, syntax_output, interface));
    }
    if let Err(err) = fs::create_dir_all(&state.out_dir) {
        eprintln!("cannot create output directory: {}", err);
        return ExitCode::from(1);
    }

    let syntax_contract_identity = match current_syntax_contract_identity() {
        Ok(identity) => identity,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    let cache_dir = state.cache_dir.as_deref().unwrap_or(&state.out_dir);
    let source_graph = crate::formal_pipeline::interface_graph::SourceInterfaceGraph::collect(
        inputs.iter().map(|(path, _, syntax, _)| {
            (
                Path::new(path.as_str()),
                contracts.get(&syntax.module_name).unwrap_or(syntax),
            )
        }),
        Some(cache_dir),
    );
    if contracts.is_empty() {
        for (_, _, syntax, interface) in &mut inputs {
            *interface = match expanded_interface(syntax, &source_graph.interfaces) {
                Ok(interface) => interface,
                Err(diagnostics) => {
                    for diagnostic in diagnostics {
                        eprintln!("{}", diagnostic.message);
                    }
                    return ExitCode::from(1);
                }
            };
        }
    }
    // A checked declaration may depend on any source in the batch. Validate
    // the complete graph before publishing. Batches with no handwritten
    // contracts retain extraction for standard-library bootstrapping.
    for (path, source, syntax, interface) in &mut inputs {
        if contracts.is_empty() {
            break;
        }
        let target = if syntax.source_kind != SyntaxSourceKind::Interface
            && state.target_profile == TargetProfile::Vm
        {
            match infer_target_profile_from_typed_evidence(
                &TargetInferenceInput::from_syntax_modules([&*syntax]),
            ) {
                Ok(inference) => inference.profile,
                Err(conflict) => {
                    eprintln!(
                        "terlc interface target inference error: {}",
                        conflict.message
                    );
                    return ExitCode::from(1);
                }
            }
        } else {
            state.target_profile
        };
        let result = crate::formal_pipeline::compile_syntax_module_with_interfaces(
            path,
            source,
            state.diagnostic_format,
            crate::formal_pipeline::InterfaceInputs {
                cache_dir: Some(cache_dir),
                source_graph: Some(&source_graph),
            },
            state.native_policy,
            target,
            TargetProfileCheckOptions::default(),
        );
        if result.exit_code != ExitCode::SUCCESS {
            return result.exit_code;
        }
        if let Some(checked) = result.checked_interface {
            *syntax = checked.syntax_output;
            *interface = checked.interface;
        } else if let Some(checked) = result.artifacts {
            *syntax = checked.syntax_output;
            *interface = checked.core.interface;
        }
    }
    let mut interfaces = source_graph.interfaces;
    for (_, _, syntax, interface) in &inputs {
        interfaces.insert(syntax.module_name.clone(), interface.clone());
    }
    for (path, source, syntax_output, interface) in inputs {
        if let Err(code) = write_interface_input(
            path,
            &source,
            &syntax_output,
            &interface,
            &interfaces,
            &syntax_contract_identity,
            state,
        ) {
            return code;
        }
    }

    ExitCode::SUCCESS
}

fn expanded_interface(
    syntax: &crate::terlan_syntax::SyntaxModuleOutput,
    interfaces: &HashMap<String, crate::terlan_hir::ModuleInterface>,
) -> Result<crate::terlan_hir::ModuleInterface, Vec<crate::terlan_typeck::Diagnostic>> {
    let resolved =
        crate::terlan_hir::resolve_syntax_module_output_with_interfaces(syntax, interfaces);
    let (expanded, diagnostics) =
        crate::terlan_typeck::expand_syntax_includes(syntax.clone(), &resolved.module);
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    let diagnostics = crate::terlan_typeck::check_public_atom_aliases(&expanded, interfaces);
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }
    Ok(syntax_module_output_to_interface(&expanded))
}

fn write_interface_input(
    path: &str,
    source: &str,
    syntax_output: &crate::terlan_syntax::SyntaxModuleOutput,
    interface: &crate::terlan_hir::ModuleInterface,
    interfaces: &HashMap<String, crate::terlan_hir::ModuleInterface>,
    syntax_contract_identity: &crate::terlan_syntax::syntax_contract::SyntaxContractIdentity,
    state: &CliState,
) -> Result<(), ExitCode> {
    let dependency_hashes =
        collect_syntax_dependency_hashes(syntax_output, interfaces, Some(Path::new(path)), None);
    let rendered = interface.to_terlan_interface_text();
    let declaration = format!("module {}.", syntax_output.module_name);
    let mut interface_text = String::new();
    for line in rendered.split_inclusive('\n') {
        interface_text.push_str(line);
        if line.trim_end() == declaration {
            for (dependency, _) in &dependency_hashes {
                if interfaces.contains_key(dependency) {
                    interface_text.push_str(&format!("import {dependency}.{{*}}.\n"));
                }
            }
        }
    }
    let target = state
        .out_dir
        .join(format!("{}.typi", syntax_output.module_name));
    if let Err(err) =
        write_if_changed_or_forced(&target, interface_text.as_bytes(), state.incremental)
    {
        eprintln!("failed to write interface output: {}", err);
        return Err(ExitCode::from(1));
    }

    let manifest = DependencyManifest {
        module: syntax_output.module_name.clone(),
        syntax_contract_identity: syntax_contract_identity.clone(),
        source_hash: fingerprint(source.as_bytes()),
        interface_hash: fingerprint(interface.to_terlan_interface_type_text().as_bytes()),
        interface_doc_hash: fingerprint(interface.to_terlan_interface_doc_text().as_bytes()),
        dependencies: dependency_hashes,
    };
    let manifest_target = state
        .out_dir
        .join(format!("{}.typi.deps", syntax_output.module_name));
    write_if_changed_or_forced(
        &manifest_target,
        manifest.encode().as_bytes(),
        state.incremental,
    )
    .map_err(|err| {
        eprintln!("failed to write interface dependency output: {}", err);
        ExitCode::from(1)
    })
}
