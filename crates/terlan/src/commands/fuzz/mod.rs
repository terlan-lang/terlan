//! Deterministic, type-directed compiler fuzzing commands.

use std::path::Path;
use std::process::ExitCode;

pub(crate) fn run(args: &[String], global_out_dir: &Path) -> ExitCode {
    let result = terlan_type_fuzz::parse(args).and_then(|action| {
        let compiler = std::env::current_exe()
            .map_err(|error| format!("cannot locate current compiler: {error}"))?;
        let out_dir = (global_out_dir != Path::new("_build")).then_some(global_out_dir);
        terlan_type_fuzz::execute(
            terlan_type_fuzz::apply_output_directory(action, out_dir),
            &compiler,
            env!("TERLAN_CHECKED_FRONTEND_REVISION_SHA256"),
        )
    });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error[type_fuzz]: {message}");
            ExitCode::from(1)
        }
    }
}
