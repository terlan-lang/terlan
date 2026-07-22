use std::collections::BTreeSet;

use super::{
    extract_cli_exact_selectors, extract_grouped_test_filters, missing_required_test_coverage,
    parse_cargo_test_names, stale_selectors,
};

/// Verifies Makefile exact-selector extraction.
///
/// Inputs:
/// - Fixture Makefile text containing exact and non-exact test invocations.
///
/// Output:
/// - Extracted selector list.
///
/// Transformation:
/// - Keeps shared and CLI exact-test recipes that include `-- --exact`.
#[test]
fn extract_cli_exact_selectors_keeps_exact_make_recipes() {
    let makefile = r#"
check:
	$(TERLC_EXACT_TEST) commands::build::tests::builds_project -- --exact
	$(TERLC_EXACT_TEST) commands::build::tests::not_exact
	$(OTHER_TEST) commands::serve::tests::ignored -- --exact
	$(TERLC_EXACT_TEST) commands::serve::tests::serves_static -- --exact
	$(EXACT_CARGO_TEST) -p terlan --bin terlan-vm runtime::vm::scheduler::tests::schedules_process -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::http::http_test::vm_http_roundtrips_request_and_response_over_vm_tcp_streams -- --exact
"#;

    let selectors = extract_cli_exact_selectors(makefile).expect("selectors should parse");

    assert_eq!(
        selectors,
        vec![
            "commands::build::tests::builds_project",
            "commands::serve::tests::serves_static",
            "runtime::vm::scheduler::tests::schedules_process",
            "runtime::vm::http::http_test::vm_http_roundtrips_request_and_response_over_vm_tcp_streams"
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

/// Verifies required VM-stream serve selectors are release-gated.
///
/// Inputs:
/// - Selector list containing the VM HTTP/TCP and serve adapter selectors.
///
/// Output:
/// - No missing required selector diagnostics.
///
/// Transformation:
/// - Keeps production HTTP migration coverage from being dropped while the
///   Hyper listener remains transitional.
#[test]
fn missing_required_exact_selectors_accepts_vm_stream_contract() {
    let selectors = vec![
        "runtime::vm::http::http_test::vm_http_roundtrips_request_and_response_over_vm_tcp_streams"
            .to_string(),
        "commands::serve::serve_test::vm_stream_request_executes_dynamic_handler_without_hyper"
            .to_string(),
        "commands::serve::serve_test::vm_stream_request_returns_websocket_upgrade_handshake_without_hyper"
            .to_string(),
    ];

    let missing = missing_required_test_coverage(&selectors, &[]);

    assert!(missing.is_empty());
}

/// Verifies missing VM-stream serve selectors fail the quality gate.
///
/// Inputs:
/// - Selector list without the required VM-stream serve selectors.
///
/// Output:
/// - Stable diagnostics naming the missing required selectors.
///
/// Transformation:
/// - Prevents release gates from silently drifting back to Hyper-only serve
///   coverage.
#[test]
fn missing_required_exact_selectors_reports_vm_stream_contract_gaps() {
    let missing = missing_required_test_coverage(&[], &[]);

    assert!(
        missing.iter().any(|diagnostic| {
            diagnostic.contains(
                "commands::serve::serve_test::vm_stream_request_executes_dynamic_handler_without_hyper",
            )
        }),
        "expected VM-stream dynamic handler selector diagnostic: {missing:?}"
    );
    assert!(
        missing.iter().any(|diagnostic| {
            diagnostic.contains(
                "runtime::vm::http::http_test::vm_http_roundtrips_request_and_response_over_vm_tcp_streams",
            )
        }),
        "expected VM HTTP/TCP selector diagnostic: {missing:?}"
    );
}

#[test]
fn grouped_filter_satisfies_required_vm_stream_contract() {
    let exact = vec![
        "runtime::vm::http::http_test::vm_http_roundtrips_request_and_response_over_vm_tcp_streams"
            .to_string(),
    ];
    let grouped = vec!["commands::serve::serve_test::vm_stream_".to_string()];

    assert!(missing_required_test_coverage(&exact, &grouped).is_empty());
}

#[test]
fn grouped_filter_extraction_reads_binary_test_filter() {
    let makefile = r#"
check:
	$(RUST_TEST) -p terlan --bin terlan-vm runtime::vm::http::http_test
	$(RUST_TEST) -p terlan --bin terlc commands::serve::serve_test::vm_stream_ -- --quiet
"#;

    assert_eq!(
        extract_grouped_test_filters(makefile),
        vec![
            "runtime::vm::http::http_test",
            "commands::serve::serve_test::vm_stream_",
        ]
    );
}
