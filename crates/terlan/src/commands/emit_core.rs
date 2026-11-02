//! Bootstrap artifact export for Terlan-owned semantic compiler services.
//!
//! This module serializes existing authoritative compiler results. Snapshot
//! identity, indexing, MCP handling, and edit/transaction policy belong in Terlan.

use std::io::{self, Write};
use std::path::Path;
use std::process::ExitCode;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{CliCommand, CliState};

/// Checks a source or explicit source root and writes one versioned artifact.
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    let [path] = cmd.args.as_slice() else {
        eprintln!("terlc emit-core requires one source file or source root");
        return ExitCode::from(2);
    };
    let result = if Path::new(path).is_dir() {
        export_sources(Path::new(path), &state)
    } else {
        crate::support::read_file(path)
            .map_err(|error| error.to_string())
            .and_then(|source| export(path, &source, &state))
    };
    let (document, exit_code) = match result {
        Ok(result) => result,
        Err(message) => {
            eprintln!("error[core_export]: {message}");
            return ExitCode::from(1);
        }
    };
    let stdout = io::stdout();
    let mut output = stdout.lock();
    if let Err(error) = serde_json::to_writer(&mut output, &document)
        .and_then(|()| writeln!(output).map_err(serde_json::Error::io))
    {
        eprintln!("error[core_export]: cannot write checked artifact: {error}");
        return ExitCode::from(1);
    }
    exit_code
}

fn digest(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn dependencies(
    interfaces: &std::collections::HashMap<String, crate::terlan_hir::ModuleInterface>,
) -> Vec<Value> {
    let mut dependencies = interfaces
        .iter()
        .map(|(name, interface)| {
            let text = interface.to_terlan_interface_text();
            json!({"module": name, "interface_digest": digest(&text), "interface_source": text})
        })
        .collect::<Vec<_>>();
    dependencies.sort_by(|a, b| a["module"].as_str().cmp(&b["module"].as_str()));
    dependencies
}

fn export(path: &str, source: &str, state: &CliState) -> Result<(Value, ExitCode), String> {
    export_with_interfaces(path, source, state, None)
}

fn export_with_interfaces(
    path: &str,
    source: &str,
    state: &CliState,
    source_graph: Option<&crate::formal_pipeline::interface_graph::SourceInterfaceGraph>,
) -> Result<(Value, ExitCode), String> {
    let target = super::check::effective_check_target_profile(source, state.target_profile)?;
    let ((((result, calls), binding_types), requirement_observations), coverage_observations) =
        crate::terlan_typeck::coverage_observations::capture(|| {
            crate::terlan_typeck::requirement_observations::capture(|| {
                crate::terlan_typeck::binding_type_observations::capture(|| {
                    crate::terlan_typeck::call_observations::capture(|| {
                        crate::formal_pipeline::compile_syntax_module_with_interfaces(
                            path,
                            source,
                            // The pipeline's JSON diagnostic renderer writes to stdout. Keep
                            // that channel exclusively for this command's single artifact;
                            // the same structured diagnostics are embedded below.
                            crate::DiagnosticFormat::Text {
                                color: crate::ColorChoice::Never,
                            },
                            crate::formal_pipeline::InterfaceInputs {
                                cache_dir: state.cache_dir.as_deref(),
                                source_graph,
                            },
                            state.native_policy,
                            target,
                            crate::validation::target_profile::TargetProfileCheckOptions::default(),
                        )
                    })
                })
            })
        });
    let mut diagnostics = Vec::new();
    for (phase, entries) in [
        ("parse", &result.parse_diagnostics),
        ("macro_expansion", &result.macro_expansion_diagnostics),
        ("include_expansion", &result.include_expansion_diagnostics),
        ("resolve", &result.resolve_diagnostics),
        ("typecheck", &result.typecheck_diagnostics),
        ("core", &result.core_diagnostics),
    ] {
        for entry in entries {
            diagnostics.push(json!({"phase": phase, "diagnostic": entry}));
        }
    }
    let successful = result.exit_code == ExitCode::SUCCESS;
    let (core, interface, dependencies, status) = if successful {
        if let Some(artifacts) = &result.artifacts {
            (
                serde_json::to_value(&artifacts.core).map_err(|error| error.to_string())?,
                json!(artifacts.core.interface_text()),
                dependencies(&artifacts.interfaces),
                "checked",
            )
        } else if let Some(artifacts) = &result.checked_interface {
            (
                Value::Null,
                json!(artifacts.interface.to_terlan_interface_text()),
                dependencies(&artifacts.interfaces),
                "interface_only",
            )
        } else {
            return Err("successful compiler result has no checked artifact".to_string());
        }
    } else {
        (Value::Null, Value::Null, Vec::new(), "invalid")
    };
    let mut declarations = if let Some(artifacts) = &result.artifacts {
        serde_json::to_value(&artifacts.syntax_output.declarations)
    } else if let Some(artifacts) = &result.checked_interface {
        serde_json::to_value(&artifacts.syntax_output.declarations)
    } else {
        Ok(json!([]))
    }
    .map_err(|error| error.to_string())?;
    if successful {
        for declaration in declarations.as_array_mut().into_iter().flatten() {
            if declaration["payload"]["raw_kind"] != "shape" {
                continue;
            }
            let text = declaration["payload"]["text"]
                .as_str()
                .ok_or("checked shape text missing")?;
            let contract = crate::terlan_syntax::describe_shape(text)
                .map_err(|error| format!("cannot serialize checked shape: {error:?}"))?;
            declaration["payload"]["name"] = json!(contract.name);
            declaration["payload"]["shape_contract"] =
                serde_json::to_value(contract).map_err(|error| error.to_string())?;
        }
    }
    let module_name = result
        .artifacts
        .as_ref()
        .map(|artifacts| &artifacts.syntax_output.module_name)
        .or_else(|| {
            result
                .checked_interface
                .as_ref()
                .map(|artifacts| &artifacts.syntax_output.module_name)
        });
    let resolved_imports = successful.then_some(&result.resolved_imports);
    let purity_contracts = if successful {
        result
            .artifacts
            .as_ref()
            .map(|artifacts| {
                crate::formal_pipeline::resolution_metadata::purity_contracts(
                    &artifacts.core.interface,
                )
            })
            .or_else(|| {
                result.checked_interface.as_ref().map(|artifacts| {
                    crate::formal_pipeline::resolution_metadata::purity_contracts(
                        &artifacts.interface,
                    )
                })
            })
    } else {
        None
    };
    let conformance_contracts = if successful {
        result
            .artifacts
            .as_ref()
            .map(|artifacts| &artifacts.core.interface)
            .or_else(|| {
                result
                    .checked_interface
                    .as_ref()
                    .map(|artifacts| &artifacts.interface)
            })
            .map(crate::formal_pipeline::resolution_metadata::conformance_contracts)
    } else {
        None
    };
    let document = json!({
        "schema": "terlan.semantic.core-export.v1",
        "compiler_version": env!("CARGO_PKG_VERSION"),
        "syntax_contract": crate::validation::phase_manifest::current_syntax_contract_identity()?,
        "source_name": path,
        "module_name": module_name,
        "source_digest": digest(source),
        "target_profile": target.as_str(),
        "native_policy": state.native_policy.as_str(),
        "status": status,
        "core": core,
        "interface_source": interface,
        "source_declarations": declarations,
        "resolved_imports": resolved_imports,
        "purity_contracts": purity_contracts,
        "coverage_observations": coverage_observations,
        "requirement_observations": requirement_observations,
        "binding_type_observations": binding_types,
        "call_resolution_observations": calls,
        "conformance_contracts": conformance_contracts,
        "dependencies": dependencies,
        "diagnostics": diagnostics,
    });
    Ok((document, result.exit_code))
}

/// Exports the same shared source-interface graph used by project checking.
/// The Terlan caller owns freezing, scope selection, identities, and storage.
fn export_sources(root: &Path, state: &CliState) -> Result<(Value, ExitCode), String> {
    let files = crate::formal_pipeline::terlan_check_sources_in_dir(root)
        .map_err(|error| error.to_string())?;
    if files.is_empty() {
        return Err("source root contains no Terlan sources".to_string());
    }
    let mut inputs = Vec::new();
    let mut parsed = Vec::new();
    for path in files {
        let source = crate::support::read_file(&path.to_string_lossy())?;
        if let Ok(syntax) =
            crate::formal_pipeline::parse_source_as_syntax_output(&path.to_string_lossy(), &source)
        {
            crate::commands::source_layout::validate_module_layout(
                root,
                &path,
                &syntax.module_name,
            )?;
            parsed.push((path.clone(), syntax));
        }
        inputs.push((path, source));
    }
    let graph = crate::formal_pipeline::interface_graph::SourceInterfaceGraph::collect(
        parsed.iter().map(|(path, syntax)| (path.as_path(), syntax)),
        state.cache_dir.as_deref(),
    );
    let mut units = Vec::new();
    let mut successful = true;
    for (path, source) in inputs {
        let (document, status) =
            export_with_interfaces(&path.to_string_lossy(), &source, state, Some(&graph))?;
        successful &= status == ExitCode::SUCCESS;
        let relative = path
            .strip_prefix(root)
            .map_err(|error| error.to_string())?
            .to_str()
            .ok_or_else(|| "source path is not UTF-8".to_string())?
            .replace('\\', "/");
        units.push(json!({"path": relative, "export": document}));
    }
    Ok((
        json!({
            "schema": "terlan.semantic.core-bundle.v1",
            "compiler_version": env!("CARGO_PKG_VERSION"),
            "syntax_contract": crate::validation::phase_manifest::current_syntax_contract_identity()?,
            "status": if successful { "checked" } else { "invalid" },
            "units": units,
        }),
        if successful {
            ExitCode::SUCCESS
        } else {
            ExitCode::from(1)
        },
    ))
}

#[cfg(test)]
#[path = "emit_core_test.rs"]
mod tests;
