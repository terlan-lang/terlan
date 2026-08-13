//! Crash-isolated native capability worker entrypoint.

use std::ffi::OsString;
use std::io::{self, BufReader, BufWriter};
use std::process::ExitCode;

use terlan_runtime_abi::{BoundaryError, ErrorDomain};

mod protocol;
mod sandbox;

/// Runs the capability worker using process arguments and standard streams.
pub fn run_from_env() -> ExitCode {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    if matches!(args.as_slice(), [flag] if flag == "--version" || flag == "-V") {
        println!("terlan-native-worker {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS;
    }
    let input = io::stdin();
    let output = io::stdout();
    match run(args, BufReader::new(input), BufWriter::new(output.lock())) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

fn run(
    args: Vec<OsString>,
    input: impl io::BufRead + Send,
    output: impl io::Write,
) -> Result<(), BoundaryError> {
    let config = protocol::CapabilityWorkerConfig::parse(&args).map_err(|error| {
        BoundaryError::message(ErrorDomain::NativeBoundary, "parse worker policy", error)
    })?;
    sandbox::verify_capability_worker_sandbox(config.sandbox_profile()).map_err(|error| {
        BoundaryError::message(ErrorDomain::NativeBoundary, "attest worker sandbox", error)
    })?;
    protocol::run_capability_worker(config, input, output).map_err(|error| {
        BoundaryError::message(
            ErrorDomain::NativeBoundary,
            "execute capability protocol",
            error,
        )
    })
}

#[cfg(test)]
#[path = "main_test.rs"]
#[cfg(test)]
mod main_test;
