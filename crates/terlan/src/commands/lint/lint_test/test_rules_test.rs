use std::path::Path;

use super::lint_source;

/// Verifies roundtrip-shaped tests suggest property coverage.
#[test]
fn lint_reports_roundtrip_test_without_property_runner() {
    let diagnostics = lint_source(
        Path::new("CodecTest.terl"),
        r#"
module sample.CodecTest.

@test
pub json_roundtrip_preserves_value(): Bool ->
    assert_equal("1", Json.stringify(Json.parse("1"))).
"#,
    );

    let rendered = diagnostics
        .iter()
        .map(super::super::render_diagnostic)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(rendered.contains("suggestion[TL0404:tests.property-candidate]"));
    assert!(rendered.contains("property-shaped test should use std.test.Gen property runners"));
    assert!(!rendered.contains("[fix available]"));
}

/// Verifies property runners satisfy property-shaped test names.
#[test]
fn lint_accepts_roundtrip_test_with_property_runner() {
    let diagnostics = lint_source(
        Path::new("CodecTest.terl"),
        r#"
module sample.CodecTest.

import std.test.Gen.{elements, for_all}.

@test
pub json_roundtrip_preserves_value(): Bool ->
    for_all(elements(["1", "2"]), (value) -> Json.stringify(Json.parse(value)) == value).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0404"),
        "property runner must satisfy property-candidate lint: {diagnostics:?}"
    );
}

/// Verifies focused ordering examples are not treated as property laws.
#[test]
fn lint_accepts_ordinary_ordering_example_without_property_runner() {
    let diagnostics = lint_source(
        Path::new("OrderingTest.terl"),
        r#"
module sample.OrderingTest.

@test
pub ordering_trait_orders_smaller_integer_first(): Bool ->
    assert_equal(Lt, compare(1, 2)).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .all(|diagnostic| diagnostic.rule_id != "TL0404"),
        "ordinary ordering examples must not require property coverage: {diagnostics:?}"
    );
}

/// Verifies actual ordering laws receive property-test guidance.
#[test]
fn lint_reports_ordering_law_without_property_runner() {
    let diagnostics = lint_source(
        Path::new("OrderingTest.terl"),
        r#"
module sample.OrderingTest.

@test
pub ordering_transitive_law_holds(): Bool ->
    assert(compare(a, b).is_transitive(compare(b, c))).
"#,
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.rule_id == "TL0404"),
        "ordering law tests should receive property guidance: {diagnostics:?}"
    );
}
