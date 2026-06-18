use std::fs;
use std::process::ExitCode;

mod cast_semantics;
mod core_lowering;
mod declarations;
mod direct_ast;
mod oxc_backend;
mod std_core_string_intrinsics;
pub(crate) mod target_contract;

pub(crate) use declarations::emit_core_module_to_typescript_declarations;
pub(crate) use oxc_backend::{emit_core_module_with_direct_oxc_ast, validate_js_module_with_oxc};

#[cfg(test)]
pub(crate) use oxc_backend::assert_oxc_accepts_js_artifact;

#[cfg(test)]
pub(crate) use declarations::{split_top_level_args, typer_type_to_typescript};

use crate::{
    formal_pipeline::compile_syntax_module_through_phases_with_profile,
    support::write_if_changed_or_forced, CliState,
};

/// Executes the `emit-js` CLI command.
///
/// Inputs:
/// - `args`: command-local arguments after the `emit-js` verb.
/// - `state`: parsed global CLI state (output directory, incremental mode,
///   diagnostic format, native policy, cache directory).
///
/// Output:
/// - `ExitCode::SUCCESS` on successful JavaScript/TypeScript output generation.
/// - `ExitCode::from(2)` on malformed command arguments.
/// - `ExitCode::from(1)` for read, compile, diagnostics, or write failures.
///
/// Transformation:
/// - Parses CLI flags, validates input source through compile phases, and writes
///   JavaScript stubs plus optional declaration files to the requested output
///   directory.
pub(crate) fn run(args: &[String], state: &CliState) -> ExitCode {
    let args = match parse_emit_js_args(args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{}", message);
            crate::print_usage();
            return ExitCode::from(2);
        }
    };

    let source = match crate::support::read_file(&args.path) {
        Ok(source) => source,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    let artifacts = match compile_syntax_module_through_phases_with_profile(
        &args.path,
        &source,
        state.diagnostic_format,
        state.cache_dir.as_deref(),
        state.native_policy,
        state.target_profile,
    ) {
        Ok(compiled) => compiled,
        Err(exit_code) => return exit_code,
    };
    let crate::formal_pipeline::CheckedSyntaxModuleArtifacts { core, .. } = artifacts;
    if let Err(err) = fs::create_dir_all(&state.out_dir) {
        eprintln!("cannot create output directory: {}", err);
        return ExitCode::from(1);
    }

    let js_target = state.out_dir.join(format!("{}.js", core.module));
    let js = match oxc_backend::emit_core_module_with_oxc_codegen(&core) {
        Ok(js) => js,
        Err(message) => {
            eprintln!("Oxc rejected generated JavaScript: {message}");
            return ExitCode::from(1);
        }
    };
    if let Err(err) = write_if_changed_or_forced(&js_target, js.as_bytes(), state.incremental) {
        eprintln!("failed to write JavaScript output: {}", err);
        return ExitCode::from(1);
    }

    if args.declarations {
        let declarations_target = state.out_dir.join(format!("{}.d.ts", core.module));
        let declarations = declarations::emit_core_module_to_typescript_declarations(&core);
        if let Err(err) = write_if_changed_or_forced(
            &declarations_target,
            declarations.as_bytes(),
            state.incremental,
        ) {
            eprintln!("failed to write TypeScript declarations: {}", err);
            return ExitCode::from(1);
        }
    }

    ExitCode::SUCCESS
}

/// Parsed command-local arguments for `terlc emit-js`.
///
/// Inputs:
/// - Source path supplied after the command verb.
/// - Optional declaration-emission flag.
///
/// Output:
/// - Compact argument record consumed by `run`.
///
/// Transformation:
/// - Separates command-local JavaScript emission choices from global CLI state
///   such as output directory, target profile, and incremental mode.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct EmitJsArgs {
    pub(crate) path: String,
    pub(crate) declarations: bool,
}

/// Parses command-local flags for `emit-js`.
///
/// Inputs:
/// - `args`: flag list from the `emit-js` verb.
///
/// Output:
/// - Parsed argument struct with source path and declaration flag.
/// - `Err(String)` for missing/invalid arguments.
///
/// Transformation:
/// - Parses positional path and optional `--declarations` flag while rejecting
///   unknown arguments.
pub(crate) fn parse_emit_js_args(args: &[String]) -> Result<EmitJsArgs, String> {
    if args.is_empty() {
        return Err("emit-js requires a file".to_string());
    }

    let path = args[0].clone();
    let mut declarations = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--declarations" => {
                declarations = true;
                i += 1;
            }
            other => return Err(format!("unexpected emit-js argument: {other}")),
        }
    }

    Ok(EmitJsArgs { path, declarations })
}

#[cfg(test)]
#[path = "cast_emit_test.rs"]
mod cast_emit_test;

#[cfg(test)]
#[path = "emit_js_test.rs"]
mod emit_js_test;

#[cfg(test)]
#[path = "std_core_string_intrinsic_test.rs"]
mod std_core_string_intrinsic_test;

#[cfg(test)]
#[path = "target_contract_test.rs"]
mod target_contract_test;
