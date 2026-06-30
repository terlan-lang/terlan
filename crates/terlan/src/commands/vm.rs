use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::runtime::vm::{ReplValue, TerlanVm};
use crate::{CliCommand, CliState};

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
/// - Compiles a Terlan source file through the normal frontend, loads its
///   CoreIR into the in-process Rust VM, and executes a zero-arity entrypoint
///   without emitting Erlang source or invoking BEAM.
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
    Run { source: PathBuf, entry: String },
    Error(String),
}

/// Parses hidden Rust VM command arguments.
fn parse_vm_args(args: &[String]) -> VmArgs {
    match args {
        [] => VmArgs::Error("terlc vm requires a subcommand: run".to_string()),
        [flag] if matches!(flag.as_str(), "--help" | "-h") => VmArgs::Help,
        [subcommand, rest @ ..] if subcommand == "run" => parse_vm_run_args(rest),
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

/// Compiles, loads, and executes one source file with the Rust VM.
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
/// - Treats the checked CoreIR module as the VM load unit, keeping this first
///   VM path tied to compiler output rather than backend-generated artifacts.
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
    let mut vm = TerlanVm::new();
    vm.load_module(artifacts.core);
    vm.execute_zero_arity(&module_name, entry, output)
}

/// Prints hidden Rust VM command usage.
fn print_vm_usage() {
    println!("terlc --experimental vm run <file.terl> [--entry <function>]");
}

#[cfg(test)]
#[path = "vm_test.rs"]
mod vm_test;
