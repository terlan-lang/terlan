#![forbid(unsafe_code)]

use std::process::ExitCode;

fn main() -> ExitCode {
    let args = std::env::args().skip(1).collect();
    terlan::run_serve_runtime(args)
}
