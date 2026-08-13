#![deny(unsafe_code)]

fn main() -> std::process::ExitCode {
    terlan::native_worker::run_from_env()
}
