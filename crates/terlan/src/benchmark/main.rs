#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    terlan::benchmark::run_from_env()
}
