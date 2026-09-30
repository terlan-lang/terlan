use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use crate::commands::artifacts::{
    collect_syntax_dependency_hashes, fingerprint, DependencyManifest,
};
use crate::terlan_hir::syntax_module_output_to_interface;
use crate::validation::phase_manifest::current_syntax_contract_identity;

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
/// - `ExitCode::from(1)` on read, parse, serialization, directory, or write
///   failures.
///
/// Transformation:
/// - Reads one `.terl` or `.terli` source, parses it through the formal
///   syntax-output path, converts that output to Terlan interface text, and
///   writes `<module>.typi` plus `<module>.typi.deps` into the configured
///   output directory.
pub(crate) fn run(args: &[String], state: &CliState) -> ExitCode {
    if args.is_empty() {
        eprintln!("missing path argument");
        crate::print_usage();
        return ExitCode::from(2);
    }

    let mut inputs = Vec::with_capacity(args.len());
    let mut module_names = HashSet::new();
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
    let mut interfaces = crate::formal_pipeline::load_external_interfaces(
        args.first().expect("non-empty interface input"),
        Some(state.out_dir.as_path()),
    );
    interfaces.extend(
        inputs
            .iter()
            .map(|(_, _, syntax, interface)| (syntax.module_name.clone(), interface.clone())),
    );
    // Batch inputs can arrive before their parents. Resolve inherited fields
    // to a fixed point before writing any interface or dependency fingerprint.
    for _ in 0..inputs.len() {
        let mut changed = false;
        for (_, _, syntax, interface) in &mut inputs {
            let expanded = match expanded_interface(syntax, &interfaces) {
                Ok(expanded) => expanded,
                Err(diagnostics) => {
                    for diagnostic in diagnostics {
                        eprintln!("{}", diagnostic.message);
                    }
                    return ExitCode::from(1);
                }
            };
            changed |= interface.struct_fields != expanded.struct_fields;
            *interface = expanded;
            interfaces.insert(syntax.module_name.clone(), interface.clone());
        }
        if !changed {
            break;
        }
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
    let interface_text = interface.to_terlan_interface_text();
    let target = state
        .out_dir
        .join(format!("{}.typi", syntax_output.module_name));
    if let Err(err) =
        write_if_changed_or_forced(&target, interface_text.as_bytes(), state.incremental)
    {
        eprintln!("failed to write interface output: {}", err);
        return Err(ExitCode::from(1));
    }

    let dependency_hashes =
        collect_syntax_dependency_hashes(syntax_output, interfaces, Some(Path::new(path)), None);
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
