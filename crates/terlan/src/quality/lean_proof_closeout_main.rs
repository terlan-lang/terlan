#![forbid(unsafe_code)]

use std::path::Path;
use std::process::ExitCode;

type QualityResult<T> = Result<T, String>;

#[path = "lean_proof_closeout.rs"]
mod lean_proof_closeout;

fn main() -> ExitCode {
    match lean_proof_closeout::run_lean_proof_closeout(Path::new(".")) {
        Ok(summary) => {
            println!(
                "[lean-proof-closeout] {} families and {} baseline classes verified; baseline {}; gate report {}.",
                summary.family_count,
                summary.baseline_count,
                summary.baseline_hash,
                summary.gate_report.display()
            );
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}
