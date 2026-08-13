#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    terlan::vm::run_from_env()
}
