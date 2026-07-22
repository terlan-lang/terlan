use super::*;

#[test]
fn inspect_requires_snapshot_mode() {
    assert_eq!(
        parse_args(&[]).expect_err("missing snapshot must fail"),
        "terlc inspect requires --snapshot"
    );
}

#[test]
fn inspect_rejects_unknown_options() {
    assert_eq!(
        parse_args(&["--watch".to_string()]).expect_err("unknown option must fail"),
        "unknown terlc inspect option: --watch"
    );
}
