use super::*;

/// Executes the `terlc serve` command after parsing compiler CLI state.
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) fn run(cmd: CliCommand, state: CliState) -> ExitCode {
    let args = match parse_serve_args(&cmd.args, &state) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    run_parsed(args)
}

/// Executes the compiler-free persisted-image runtime entrypoint.
pub(crate) fn run_serve_runtime(raw_args: Vec<String>) -> ExitCode {
    let args = match super::super::args::parse_serve_runtime_args(&raw_args) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::from(2);
        }
    };
    run_parsed(args)
}
