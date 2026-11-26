//! Deterministic, type-directed compiler fuzzing commands.

mod artifacts;
mod oracle;
mod resources;

use std::process::ExitCode;

use terlan_type_fuzz::{Action, GeneratedCase, GenerationConfig, RunOptions};

fn generate_case(index: u32, options: &RunOptions) -> GeneratedCase {
    terlan_type_fuzz::generate(
        index,
        &GenerationConfig {
            seed: options.seed,
            max_depth: options.max_depth,
            oracle: options.oracle,
            timeout_ms: options.case_timeout_ms,
            rss_limit_bytes: options.rss_limit_bytes,
            cpu_limit_ms: options.cpu_limit_ms,
            compiler_revision: env!("TERLAN_CHECKED_FRONTEND_REVISION_SHA256"),
        },
    )
}

pub(crate) fn run(args: &[String]) -> ExitCode {
    match terlan_type_fuzz::parse(args).and_then(execute) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error[type_fuzz]: {message}");
            ExitCode::from(1)
        }
    }
}

fn execute(action: Action) -> Result<(), String> {
    match action {
        Action::Types(options) => artifacts::generate_run(&options),
        Action::Replay(path) => artifacts::replay(&path),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_generates_and_replays_offline_cases() {
        let root = tempfile::tempdir().expect("temporary fuzz command root");
        let output = root.path().to_string_lossy().into_owned();
        let generate = [
            "types",
            "--cases",
            "1",
            "--oracle",
            "none",
            "--out-dir",
            &output,
        ]
        .map(str::to_string);
        assert_eq!(run(&generate), ExitCode::SUCCESS);
        let defaults = terlan_type_fuzz::RunOptions::default();
        let case = generate_case(0, &defaults);
        let replay = [
            "replay".to_string(),
            root.path()
                .join(case.manifest.case_id)
                .to_string_lossy()
                .into_owned(),
        ];
        assert_eq!(run(&replay), ExitCode::SUCCESS);
        assert_eq!(run(&[]), ExitCode::from(1));
    }
}
