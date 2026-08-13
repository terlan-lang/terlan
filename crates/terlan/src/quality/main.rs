#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    terlan::quality::run_from_env()
}
