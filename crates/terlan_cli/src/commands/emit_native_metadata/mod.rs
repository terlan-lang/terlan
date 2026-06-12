use std::fs;
use std::process::ExitCode;

use crate::validation::native_policy::source_contains_unsafe_native;
use crate::{CliCommand, CliState};

mod artifacts;
pub(crate) use artifacts::*;

/// Executes the `emit-native-metadata` CLI command.
///
/// Inputs:
/// - `cmd`: parsed CLI command containing command-local arguments.
/// - `state`: parsed global CLI state, including output directory, cache,
///   diagnostic format, native policy, and incremental-write mode.
///
/// Output:
/// - `ExitCode::SUCCESS` when native metadata and stubs are emitted.
/// - `ExitCode::from(2)` when command-local arguments are malformed.
/// - `ExitCode::from(1)` for read, compile, unsafe-native, directory, metadata,
///   validation, or write failures.
///
/// Transformation:
/// - Reads one source file, validates it through the formal compile path,
///   rejects unsafe native declarations, and delegates artifact generation to
///   the shared native artifact emitter.
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    if cmd.args.len() != 1 {
        eprintln!("missing or extra path argument");
        crate::print_usage();
        return ExitCode::from(2);
    }

    let path = &cmd.args[0];
    let source = match crate::support::read_file(path) {
        Ok(source) => source,
        Err(message) => {
            eprintln!("{}", message);
            return ExitCode::from(1);
        }
    };
    if let Err(exit_code) =
        crate::formal_pipeline::compile_syntax_module_through_phases_with_profile(
            path,
            &source,
            state.diagnostic_format,
            state.cache_dir.as_deref(),
            state.native_policy,
            state.target_profile,
        )
    {
        return exit_code;
    }
    if source_contains_unsafe_native(&source) {
        eprintln!("unsafe native declarations require an explicit unsafe mode");
        return ExitCode::from(1);
    }

    if let Err(err) = fs::create_dir_all(&state.out_dir) {
        eprintln!("cannot create output directory: {}", err);
        return ExitCode::from(1);
    }
    if let Err(message) = emit_native_artifacts(
        &source,
        &state.out_dir,
        state.native_policy,
        state.incremental,
    ) {
        eprintln!("{}", message);
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}
