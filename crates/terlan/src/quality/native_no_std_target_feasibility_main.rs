#![forbid(unsafe_code)]

fn main() -> std::process::ExitCode {
    terlan::quality::run_native_target_feasibility_from_workspace()
}
