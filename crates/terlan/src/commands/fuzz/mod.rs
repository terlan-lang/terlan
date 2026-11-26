//! Deterministic, type-directed compiler fuzzing commands.

use std::process::ExitCode;

pub(crate) fn run(args: &[String]) -> ExitCode {
    let result = terlan_type_fuzz::parse(args).and_then(|action| {
        let compiler = std::env::current_exe()
            .map_err(|error| format!("cannot locate current compiler: {error}"))?;
        terlan_type_fuzz::execute(
            action,
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
