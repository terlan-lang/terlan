#![allow(dead_code, unused_imports)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

#[path = "../backends/mod.rs"]
pub mod backends;
#[path = "commands.rs"]
pub mod commands;
#[path = "../compiler/mod.rs"]
pub mod compiler;
#[path = "../formal_pipeline.rs"]
pub mod formal_pipeline;
#[path = "../html/mod.rs"]
pub mod html;
#[path = "../runtime/mod.rs"]
pub mod runtime;
#[path = "../support/mod.rs"]
pub mod support;
#[path = "../validation/mod.rs"]
pub mod validation;

pub(crate) use backends::erlang as terlan_erlang;
pub(crate) use compiler::hir as terlan_hir;
pub(crate) use compiler::syntax as terlan_syntax;
pub(crate) use compiler::typeck as terlan_typeck;
pub(crate) use html as terlan_html;
pub(crate) use runtime::native as terlan_native;
pub(crate) use runtime::safenative as terlan_safenative;

use runtime::vm::{ReplValue, TerlanVm};
use validation::native_policy::NativePolicy;
use validation::target_profile::TargetProfile;

/// Terminal color selection for VM compile diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum ColorChoice {
    #[default]
    Auto,
    Always,
    Never,
}

/// Diagnostic serialization mode used by the standalone VM artifact.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DiagnosticFormat {
    Text { color: ColorChoice },
    Json,
}

impl Default for DiagnosticFormat {
    fn default() -> Self {
        Self::Text {
            color: ColorChoice::Auto,
        }
    }
}

/// Parsed standalone VM command.
enum VmCommand {
    Help,
    Version,
    Run {
        source: PathBuf,
        entry: String,
        test_eval: bool,
    },
    Error(String),
}

/// Standalone Terlan VM executable entrypoint.
///
/// Inputs:
/// - Process arguments after `terlan-vm`.
///
/// Output:
/// - Exit code for help, version, compile/load/run success, or usage/runtime
///   failure.
///
/// Transformation:
/// - Builds a complete VM artifact that can compile one Terlan source file to
///   CoreIR, load it into the Rust VM, and execute a zero-arity entrypoint
///   without going through the `terlc` CLI.
fn main() -> ExitCode {
    match parse_args(std::env::args().skip(1).collect()) {
        VmCommand::Help => {
            print_usage();
            ExitCode::SUCCESS
        }
        VmCommand::Version => {
            println!("terlan-vm {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        VmCommand::Run {
            source,
            entry,
            test_eval,
        } => {
            let mut output = |line: &str| println!("{line}");
            match run_source_file(&source, &entry, test_eval, &mut output) {
                Ok(()) => ExitCode::SUCCESS,
                Err(message) => {
                    eprintln!("{message}");
                    ExitCode::from(1)
                }
            }
        }
        VmCommand::Error(message) => {
            eprintln!("{message}");
            print_usage();
            ExitCode::from(2)
        }
    }
}

/// Parses standalone VM arguments.
fn parse_args(args: Vec<String>) -> VmCommand {
    match args.as_slice() {
        [] => VmCommand::Error("terlan-vm requires a command".to_string()),
        [flag] if matches!(flag.as_str(), "--help" | "-h" | "help") => VmCommand::Help,
        [flag] if matches!(flag.as_str(), "--version" | "-V" | "version") => VmCommand::Version,
        [command, rest @ ..] if command == "run" => parse_run_args(rest),
        [command, ..] => VmCommand::Error(format!("unknown terlan-vm command: {command}")),
    }
}

/// Parses `terlan-vm run` arguments.
fn parse_run_args(args: &[String]) -> VmCommand {
    let mut source = None;
    let mut entry = "main".to_string();
    let mut test_eval = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--help" | "-h" => return VmCommand::Help,
            "--test-eval" => {
                test_eval = true;
                index += 1;
            }
            "--entry" => {
                let Some(value) = args.get(index + 1) else {
                    return VmCommand::Error("missing value for --entry".to_string());
                };
                entry = value.clone();
                index += 2;
            }
            arg if arg.starts_with('-') => {
                return VmCommand::Error(format!("unknown terlan-vm run option: {arg}"));
            }
            path => {
                if source.is_some() {
                    return VmCommand::Error(
                        "terlan-vm run accepts exactly one source file".to_string(),
                    );
                }
                source = Some(PathBuf::from(path));
                index += 1;
            }
        }
    }

    let Some(source) = source else {
        return VmCommand::Error("terlan-vm run requires a source file".to_string());
    };
    VmCommand::Run {
        source,
        entry,
        test_eval,
    }
}

/// Compiles, loads, and executes one Terlan source file in the standalone VM.
fn run_source_file(
    source: &Path,
    entry: &str,
    test_eval: bool,
    output: &mut dyn FnMut(&str),
) -> Result<(), String> {
    let contents = fs::read_to_string(source)
        .map_err(|err| format!("failed to read VM source `{}`: {err}", source.display()))?;
    let source_name = source.to_string_lossy();
    let artifacts = formal_pipeline::compile_syntax_module_through_phases_with_profile(
        &source_name,
        &contents,
        DiagnosticFormat::Text {
            color: ColorChoice::Never,
        },
        None,
        NativePolicy::SafeNativeOptional,
        TargetProfile::Erlang,
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
    let value = vm.execute_zero_arity(&module_name, entry, output)?;
    if test_eval {
        evaluate_test_result(value)?;
    }
    Ok(())
}

/// Converts a returned Bool into test-runner process semantics.
fn evaluate_test_result(value: ReplValue) -> Result<(), String> {
    match value {
        ReplValue::Bool(true) => Ok(()),
        ReplValue::Bool(false) => Err("terlan-vm test-eval failed: returned false".to_string()),
        other => Err(format!(
            "terlan-vm test-eval expects Bool return, found {}",
            other.render()
        )),
    }
}

/// Prints standalone VM usage.
fn print_usage() {
    println!("terlan-vm run <file.terl> [--entry <function>] [--test-eval]");
    println!("terlan-vm version");
}

#[cfg(test)]
#[path = "main_test.rs"]
mod main_test;
