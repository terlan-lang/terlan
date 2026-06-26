use std::collections::BTreeSet;

use super::{extract_cli_exact_selectors, parse_cargo_test_names, stale_selectors};

/// Verifies Makefile exact-selector extraction.
///
/// Inputs:
/// - Fixture Makefile text containing exact and non-exact test invocations.
///
/// Output:
/// - Extracted selector list.
///
/// Transformation:
/// - Keeps only `TERLC_EXACT_TEST` recipes that include `-- --exact`.
#[test]
fn extract_cli_exact_selectors_keeps_exact_make_recipes() {
    let makefile = r#"
check:
	$(TERLC_EXACT_TEST) commands::build::tests::builds_project -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::not_exact
	$(OTHER_TEST) commands::serve::tests::ignored -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tests::serves_static -- --exact
"#;

    let selectors = extract_cli_exact_selectors(makefile).expect("selectors should parse");

    assert_eq!(
        selectors,
        vec![
            "commands::build::tests::builds_project",
            "commands::serve::tests::serves_static"
        ]
    );
}

/// Verifies Cargo test-list parsing.
///
/// Inputs:
/// - Fixture Cargo stdout with test, benchmark, and non-test rows.
///
/// Output:
/// - Set of fully qualified test names.
///
/// Transformation:
/// - Keeps rows marked as `: test` and drops non-test entries.
#[test]
fn parse_cargo_test_names_keeps_test_rows() {
    let stdout = r#"
commands::build::tests::builds_project: test
commands::bench::benches_path: benchmark
commands::serve::tests::serves_static: test
"#;

    let tests = parse_cargo_test_names(stdout);

    assert_eq!(
        tests,
        BTreeSet::from([
            "commands::build::tests::builds_project".to_owned(),
            "commands::serve::tests::serves_static".to_owned()
        ])
    );
}

/// Verifies stale selector detection.
///
/// Inputs:
/// - Selectors from a Makefile fixture.
/// - Current test names from a Cargo-list fixture.
///
/// Output:
/// - Ordered stale selector list.
///
/// Transformation:
/// - Compares selector names against the current Cargo test set.
#[test]
fn stale_selectors_reports_missing_selectors_in_makefile_order() {
    let selectors = vec![
        "commands::build::tests::builds_project".to_owned(),
        "commands::serve::tests::stale_name".to_owned(),
        "commands::serve::tests::also_stale".to_owned(),
    ];
    let tests = BTreeSet::from(["commands::build::tests::builds_project".to_owned()]);

    let stale = stale_selectors(&selectors, &tests);

    assert_eq!(
        stale,
        vec![
            "commands::serve::tests::stale_name",
            "commands::serve::tests::also_stale"
        ]
    );
}

/// Verifies resolved selectors produce no diagnostics.
///
/// Inputs:
/// - Selectors and Cargo test names containing the same entries.
///
/// Output:
/// - Empty stale selector list.
///
/// Transformation:
/// - Confirms the comparator accepts fully resolved exact selectors.
#[test]
fn stale_selectors_accepts_all_resolved_selectors() {
    let selectors = vec![
        "commands::build::tests::builds_project".to_owned(),
        "commands::serve::tests::serves_static".to_owned(),
    ];
    let tests = BTreeSet::from([
        "commands::build::tests::builds_project".to_owned(),
        "commands::serve::tests::serves_static".to_owned(),
    ]);

    let stale = stale_selectors(&selectors, &tests);

    assert!(stale.is_empty());
}
