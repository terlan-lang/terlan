#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    terlan::quality::run_lean_proof_closeout_from_workspace()
}
