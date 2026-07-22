#![deny(unsafe_code)]

//! Bounded capability worker for explicitly external, crash-isolated, or
//! cross-boundary Rust and native adapters.

macro_rules! vm_capability_component {
    ($($item:item)*) => {
        $($item)*
    };
}

use std::ffi::OsString;
use std::io::{self, BufReader, BufWriter};
use std::process::ExitCode;

#[allow(dead_code)]
#[path = "../database_schema.rs"]
pub(crate) mod database_schema;
#[allow(dead_code, unused_imports)]
#[path = "../runtime/native/mod.rs"]
pub(crate) mod terlan_native;
#[allow(dead_code, unused_imports)]
#[path = "../runtime/native_boundary/mod.rs"]
pub(crate) mod terlan_native_boundary;

mod protocol;
mod sandbox;

/// Runs the capability worker and maps typed failures to process status.
fn main() -> ExitCode {
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

/// Runs the transport-neutral worker loop over caller-provided streams.
fn run(
    args: Vec<OsString>,
    input: impl io::BufRead + Send,
    output: impl io::Write,
) -> Result<(), String> {
    let config = protocol::CapabilityWorkerConfig::parse(&args)?;
    sandbox::verify_capability_worker_sandbox(config.sandbox_profile())?;
    protocol::run_capability_worker(config, input, output)
}

#[cfg(test)]
#[path = "main_test.rs"]
mod main_test;
