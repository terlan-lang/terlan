use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::runtime::native_image::control::TvmTransitionOperation;
use crate::runtime::vm::code_server::VmCodeServerEvent;
use crate::runtime::vm::pure_native::{
    PureNativeCapabilityRequest, PureNativeExecution, PureNativeExecutionShard,
};
#[cfg(test)]
use crate::runtime::vm::source_reload::VmSourceReloadAdapter;
use crate::runtime::vm::source_reload::VmSourceReloadBatchReport;
use crate::runtime::vm::ReplValue;
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};
use crate::{CliCommand, CliState};

#[path = "vm/native_reload.rs"]
mod native_reload;

use native_reload::{VmNativeSourceReloadReport, VmNativeSourceReloadService};

/// Runs the hidden experimental Rust VM command group.
///
/// Inputs:
/// - Parsed `vm` command arguments.
/// - Global CLI state, including the hidden `--experimental` flag.
///
/// Output:
/// - Exit code for VM usage validation, compile/load failure, or execution.
///
/// Transformation:
/// - Compiles a Terlan source file into a native image and executes a
///   zero-arity native export. Runtime CoreIR execution is forbidden.
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    if !state.experimental {
        eprintln!("terlc vm is experimental; rerun with --experimental to enable it.");
        return ExitCode::from(2);
    }

    match parse_vm_args(&cmd.args) {
        VmArgs::Help => {
            print_vm_usage();
            ExitCode::SUCCESS
        }
        VmArgs::Run { source, entry } => {
            let mut output = |line: &str| println!("{line}");
            match run_source_file_in_vm(&source, &entry, &state, &mut output) {
                Ok(_) => ExitCode::SUCCESS,
                Err(message) => {
                    eprintln!("{message}");
                    ExitCode::from(1)
                }
            }
        }
        VmArgs::Reload {
            sources,
            diagnostics,
        } => match reload_native_source_files_in_vm(&sources, &state) {
            Ok(report) => {
                for event in &report.sources.events {
                    println!("{}", render_reload_event(&event));
                }
                if diagnostics {
                    println!("{}", render_native_reload_diagnostics(&report));
                }
                ExitCode::SUCCESS
            }
            Err(message) => {
                eprintln!("{message}");
                ExitCode::from(1)
            }
        },
        VmArgs::Error(message) => {
            eprintln!("{message}");
            print_vm_usage();
            ExitCode::from(2)
        }
    }
}

/// Parsed hidden Rust VM command arguments.
enum VmArgs {
    Help,
    Run {
        source: PathBuf,
        entry: String,
    },
    Reload {
        sources: Vec<PathBuf>,
        diagnostics: bool,
    },
    Error(String),
}

/// Parses hidden Rust VM command arguments.
fn parse_vm_args(args: &[String]) -> VmArgs {
    match args {
        [] => VmArgs::Error("terlc vm requires a subcommand: run or reload".to_string()),
        [flag] if matches!(flag.as_str(), "--help" | "-h") => VmArgs::Help,
        [subcommand, rest @ ..] if subcommand == "run" => parse_vm_run_args(rest),
        [subcommand, rest @ ..] if subcommand == "reload" => parse_vm_reload_args(rest),
        [subcommand, ..] => VmArgs::Error(format!("unknown terlc vm subcommand: {subcommand}")),
    }
}

/// Parses `terlc --experimental vm run` arguments.
fn parse_vm_run_args(args: &[String]) -> VmArgs {
    let mut source = None;
    let mut entry = "main".to_string();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return VmArgs::Help,
            "--entry" => {
                let Some(value) = args.get(index + 1) else {
                    return VmArgs::Error("missing value for --entry".to_string());
                };
                entry = value.clone();
                index += 2;
            }
            arg if arg.starts_with('-') => {
                return VmArgs::Error(format!("unknown terlc vm run option: {arg}"));
            }
            path => {
                if source.is_some() {
                    return VmArgs::Error(
                        "terlc vm run accepts exactly one source file".to_string(),
                    );
                }
                source = Some(PathBuf::from(path));
                index += 1;
            }
        }
    }

    let Some(source) = source else {
        return VmArgs::Error("terlc vm run requires a source file".to_string());
    };
    VmArgs::Run { source, entry }
}

/// Parses `terlc --experimental vm reload` arguments.
fn parse_vm_reload_args(args: &[String]) -> VmArgs {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "--help" | "-h"))
    {
        return VmArgs::Help;
    }

    let mut diagnostics = false;
    let mut sources = Vec::new();
    for arg in args {
        match arg.as_str() {
            "--diagnostics" => diagnostics = true,
            option if option.starts_with('-') => {
                return VmArgs::Error(format!("unknown terlc vm reload option: {option}"));
            }
            source => sources.push(PathBuf::from(source)),
        }
    }

    if sources.is_empty() {
        return VmArgs::Error("terlc vm reload requires at least one source file".to_string());
    }

    VmArgs::Reload {
        sources,
        diagnostics,
    }
}

/// Compiles and executes one source file through its native image.
///
/// Inputs:
/// - `source`: path to a Terlan implementation source file.
/// - `entry`: zero-arity function name to execute.
/// - `state`: compiler options used by the normal frontend.
/// - `output`: callback for VM console output effects.
///
/// Output:
/// - VM return value on success.
/// - Stable error text on source read, compile, load, or execution failure.
///
/// Transformation:
/// - Uses CoreIR only as compiler input and requires the requested entry to be
///   present in the emitted native image.
fn run_source_file_in_vm(
    source: &Path,
    entry: &str,
    state: &CliState,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    let contents = fs::read_to_string(source)
        .map_err(|err| format!("failed to read VM source `{}`: {err}", source.display()))?;
    let source_name = source.to_string_lossy();
    let artifacts = crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
        &source_name,
        &contents,
        state.diagnostic_format,
        state.cache_dir.as_deref(),
        state.native_policy,
        state.target_profile,
    )
    .map_err(|code| {
        format!(
            "failed to compile VM source `{}` with exit code {:?}",
            source.display(),
            code
        )
    })?;
    let module_name = artifacts.core.module.clone();
    let module_stem = module_name.replace('.', "_");
    let workspace = state.out_dir.join("vm-command-aot").join(&module_stem);
    let image = crate::commands::build::vm_artifact::native_image::compile_repl_native_image(
        &workspace,
        &module_stem,
        &artifacts.core,
    )?
    .ok_or_else(|| {
        format!(
            "error[vm.aot_required]: `{module_name}.{entry}/0` did not produce a native image; runtime CoreIR interpretation has been removed"
        )
    })?;
    let mut shard = PureNativeExecutionShard::load_image(&image)?;
    let qualified = format!("{module_name}.{entry}");
    if !shard.has_export(&qualified, 0) {
        return Err(format!(
            "error[vm.aot_export_missing]: native image does not contain `{qualified}/0`; runtime CoreIR interpretation has been removed"
        ));
    }
    let value = call_with_command_capabilities(&mut shard, &qualified, output)?;
    shard.shutdown()?;
    Ok(value)
}

/// Drives one command-owned call while servicing its explicitly supported capabilities.
fn call_with_command_capabilities(
    shard: &mut PureNativeExecutionShard,
    qualified: &str,
    output: &mut dyn FnMut(&str),
) -> Result<ReplValue, String> {
    let (owner, mut execution) = shard.begin_call(qualified, &[])?;
    loop {
        execution = match execution {
            PureNativeExecution::Complete(value) => {
                shard.finish_completed_call(owner)?;
                return Ok(value);
            }
            PureNativeExecution::HttpResponse(_) => {
                let error = "error[vm.command_result]: VM command entry returned an HTTP response"
                    .to_string();
                shard.cancel_call(owner, error.clone())?;
                return Err(error);
            }
            PureNativeExecution::Suspended(suspension)
                if suspension.operation() == TvmTransitionOperation::Capability =>
            {
                let wait = shard.begin_capability_call(owner, &suspension)?;
                let reply = match command_capability_reply(wait.request(), output) {
                    Ok(reply) => reply,
                    Err(error) => {
                        shard.cancel_call(owner, error.clone())?;
                        return Err(error);
                    }
                };
                shard.resume_capability_call(owner, suspension, wait, reply)?
            }
            PureNativeExecution::Suspended(suspension) => shard.resume_call(owner, suspension)?,
        };
    }
}

/// Services the intentionally narrow host capabilities owned by `terlc vm run`.
fn command_capability_reply(
    request: &PureNativeCapabilityRequest,
    output: &mut dyn FnMut(&str),
) -> Result<NativeBoundaryReplyTerm, String> {
    match (
        request.capability.as_str(),
        request.operation.as_str(),
        request.arguments.as_slice(),
        &request.result_type,
    ) {
        (
            "stdio",
            "std.io.console.println",
            [NativeBoundaryTerm::Text(line)],
            crate::runtime::native_image::TvmBoundaryType::Unit,
        ) => {
            output(line);
            Ok(NativeBoundaryReplyTerm::Ok(NativeBoundaryTerm::Unit))
        }
        (capability, operation, _, _) => Err(format!(
            "error[vm.command_capability_unsupported]: `terlc vm run` does not provide capability `{capability}:{operation}`"
        )),
    }
}

/// Publishes changed source files through the VM source-reload adapter.
///
/// Inputs:
/// - `sources`: changed source or asset paths reported by a caller.
///
/// Output:
/// - Code-server publication events for changed Terlan sources.
/// - Error text when no Terlan source was published or a source fails.
///
/// Transformation:
/// - Gives the experimental command surface a concrete source-hot-reload
///   boundary while keeping long-lived watcher state in the VM adapter.
#[cfg(test)]
fn reload_source_files_in_vm(sources: &[PathBuf]) -> Result<Vec<VmCodeServerEvent>, String> {
    Ok(reload_source_files_in_vm_with_report(sources)?.events)
}

/// Publishes changed source files and returns an inspectable reload report.
///
/// Inputs:
/// - `sources`: changed source or asset paths reported by a caller.
///
/// Output:
/// - Batch diagnostics and code-server publication events.
/// - Error text when no Terlan source was published or a source fails.
///
/// Transformation:
/// - Keeps the command-facing reload path aligned with the VM adapter report so
///   dev-server, CLI, and debugger tooling can share one diagnostic contract.
#[cfg(test)]
fn reload_source_files_in_vm_with_report(
    sources: &[PathBuf],
) -> Result<VmSourceReloadBatchReport, String> {
    let mut adapter = VmSourceReloadAdapter::new();
    let report = adapter.publish_changed_files_with_report(sources)?;

    if report.events.is_empty() {
        return Err("terlc vm reload did not receive any .terl source files".to_string());
    }
    Ok(report)
}

/// Compiles and admits one source batch as an executable native generation.
fn reload_native_source_files_in_vm(
    sources: &[PathBuf],
    state: &CliState,
) -> Result<VmNativeSourceReloadReport, String> {
    VmNativeSourceReloadService::new().reload(sources, state)
}

/// Renders one VM source-reload event for command output.
///
/// Inputs:
/// - `event`: code-server event returned by the source reload adapter.
///
/// Output:
/// - Stable human-readable event summary.
///
/// Transformation:
/// - Keeps the experimental command output source-facing instead of exposing
///   internal Rust enum formatting as a CLI contract.
fn render_reload_event(event: &VmCodeServerEvent) -> String {
    match event {
        VmCodeServerEvent::Published { module, generation } => {
            format!("published {module} generation {}", generation.as_u64())
        }
        VmCodeServerEvent::HotReloaded {
            module,
            previous_generation,
            active_generation,
            ..
        } => format!(
            "hot-reloaded {module} generation {} -> {}",
            previous_generation.as_u64(),
            active_generation.as_u64()
        ),
        VmCodeServerEvent::GenerationRetired { module, generation } => {
            format!("retired {module} generation {}", generation.as_u64())
        }
        VmCodeServerEvent::GenerationPurged { module, generation } => {
            format!("purged {module} generation {}", generation.as_u64())
        }
    }
}

/// Renders VM source-reload batch diagnostics for command output.
///
/// Inputs:
/// - `report`: source reload report returned by the VM adapter.
///
/// Output:
/// - Stable single-line diagnostic summary.
///
/// Transformation:
/// - Keeps diagnostic output field-based and script-readable without exposing
///   the internal Rust struct formatting.
fn render_reload_diagnostics(report: &VmSourceReloadBatchReport) -> String {
    format!(
        "reload diagnostics: changed_paths={} unique_sources={} ignored_paths={} duplicate_sources={} events={}",
        report.changed_paths,
        report.unique_source_paths,
        report.ignored_paths,
        report.duplicate_source_paths,
        report.events.len()
    )
}

/// Renders native generation and source batch reload diagnostics.
fn render_native_reload_diagnostics(report: &VmNativeSourceReloadReport) -> String {
    format!(
        "{} native_generation={} native_image={} generation_references={}",
        render_reload_diagnostics(&report.sources),
        report.native_generation,
        report.native_image.display(),
        report.references.total()
    )
}

/// Prints hidden Rust VM command usage.
fn print_vm_usage() {
    println!("terlc --experimental vm run <file.terl> [--entry <function>]");
    println!("terlc --experimental vm reload [--diagnostics] <file.terl> [file.terl ...]");
}

#[cfg(test)]
#[path = "vm_test.rs"]
mod vm_test;
