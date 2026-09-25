//! Entry point for the shared validation owner.
#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    terlan_test_orchestrator::run_cli()
}
