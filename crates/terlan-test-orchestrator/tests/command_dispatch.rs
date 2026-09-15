//! Invalid command lines must never fall through into the expensive correctness suite.

use std::process::Command;
use std::time::Duration;
use terlan_process_owner::ProcessControl;

#[test]
fn invalid_commands_do_not_launch_the_suite() {
    for args in [
        vec!["--unknown-command"],
        vec!["--run-owned"],
        vec!["--run-owned", "--timeout-seconds", "0", "--", "true"],
        vec!["--run-owned", "--timeout-seconds", "86401", "--", "true"],
        vec!["--run-owned", "--timeout-seconds", "1", "--"],
        vec!["--coverage-request"],
        vec!["--with-cargo-coverage"],
        vec![
            "--with-cargo-coverage",
            "missing.json",
            "--",
            "sh",
            "-c",
            "true",
        ],
        vec!["--install-snapshot"],
        vec!["--install-snapshot", "unused", "unexpected"],
        vec!["--inspect-coverage"],
        vec!["--inspect-coverage", "missing.json"],
        vec!["--inspect-coverage", "missing.json", "--"],
        vec!["--verify-coverage"],
        vec!["--verify-coverage", "missing.json"],
        vec!["--verify-coverage", "missing.json", "--"],
        vec!["--verify-cargo-coverage"],
        vec!["--verify-cargo-coverage", "missing.json"],
        vec![
            "--verify-cargo-coverage",
            "missing.json",
            "requests.json",
            "extra",
        ],
    ] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_terlan-test-orchestrator"));
        command.args(&args);
        let captured = ProcessControl::new(Duration::from_secs(5))
            .capture_stdout_observed(&mut command, 16 * 1024, |_| Ok(()), |_| Ok(()))
            .unwrap();
        assert!(captured.outcome.is_err(), "{args:?}");
        assert!(
            captured.stdout.is_empty(),
            "must not start suite admission: {args:?}"
        );
    }
}
