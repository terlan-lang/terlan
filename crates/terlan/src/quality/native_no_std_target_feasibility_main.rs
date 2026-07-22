#![forbid(unsafe_code)]

use std::path::Path;
use std::process::ExitCode;

type QualityResult<T> = Result<T, String>;

#[path = "native_no_std_target_feasibility.rs"]
mod native_no_std_target_feasibility;

use native_no_std_target_feasibility::run_native_no_std_target_feasibility;

fn main() -> ExitCode {
    match run_native_no_std_target_feasibility(Path::new(".")) {
        Ok(summary) => {
            println!(
                "[native-no-std-target-feasibility] {} targets, {} features, {} rejected features, and {} adversarial cases checked; report written to {}.",
                summary.target_count,
                summary.feature_count,
                summary.rejected_feature_count,
                summary.adversarial_case_count,
                summary.report_path.display()
            );
            ExitCode::SUCCESS
        }
        Err(message) => {
            eprintln!("{message}");
            ExitCode::from(1)
        }
    }
}
