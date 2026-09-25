//! Aggregate graph deadlines remain distinct from single-phase deadlines.

use super::graph_timeout;
use std::ffi::OsString;
use std::time::Duration;

#[test]
fn graph_deadline_has_a_separate_bounded_default_and_preserves_command_arguments() {
    for (options, expected) in [
        (vec!["--", "make", "check-gates"], 3600),
        (
            vec!["--graph-timeout-seconds", "7", "--", "make", "check-gates"],
            7,
        ),
        (
            vec![
                "--graph-timeout-seconds",
                "7200",
                "--",
                "make",
                "check-gates",
            ],
            7200,
        ),
    ] {
        let mut arguments = options.into_iter().map(OsString::from);
        assert_eq!(
            graph_timeout(&mut arguments).unwrap(),
            Duration::from_secs(expected)
        );
        assert_eq!(arguments.collect::<Vec<_>>(), ["make", "check-gates"]);
    }
    assert_eq!(crate::phase_timeout_from(None), Duration::from_secs(1800));
}

#[test]
fn graph_deadline_rejects_unbounded_malformed_and_missing_options() {
    for value in ["0", "7201", "18446744073709551615", "invalid", "-1", ""] {
        let mut arguments = ["--graph-timeout-seconds", value, "--", "make"]
            .into_iter()
            .map(OsString::from);
        assert!(graph_timeout(&mut arguments).is_err());
    }
    for options in [
        vec![],
        vec!["--unknown"],
        vec!["--graph-timeout-seconds"],
        vec!["--graph-timeout-seconds", "3", "make"],
    ] {
        assert!(graph_timeout(&mut options.into_iter().map(OsString::from)).is_err());
    }
}
