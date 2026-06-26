use std::path::Path;

use regex::Regex;

use super::source_findings_for_text;

/// Verifies Oxc symbols are detected in forbidden source text.
///
/// Inputs:
/// - Fixture Rust source containing Oxc symbol shapes.
///
/// Output:
/// - Source findings with one-based line numbers.
///
/// Transformation:
/// - Applies the Oxc symbol regex to each source line.
#[test]
fn source_findings_for_text_reports_oxc_symbols() {
    let pattern = Regex::new(r"\b(?:Oxc|oxc_|oxc::)").expect("regex should compile");
    let text = "\
fn clean() {}\n\
fn leaked() { let value = OxcThing; }\n\
fn also_leaked() { oxc::parse(); }\n";

    let findings =
        source_findings_for_text(Path::new("crates/terlan_typeck/src/lib.rs"), text, &pattern);

    assert_eq!(findings.len(), 2);
    assert_eq!(findings[0].line, Some(2));
    assert_eq!(findings[1].line, Some(3));
}

/// Verifies normal Rust source does not produce Oxc findings.
///
/// Inputs:
/// - Fixture Rust source without Oxc symbols.
///
/// Output:
/// - Empty findings list.
///
/// Transformation:
/// - Confirms unrelated compiler code remains valid under the boundary check.
#[test]
fn source_findings_for_text_accepts_non_oxc_source() {
    let pattern = Regex::new(r"\b(?:Oxc|oxc_|oxc::)").expect("regex should compile");

    let findings = source_findings_for_text(
        Path::new("crates/terlan_typeck/src/lib.rs"),
        "fn lower_core_expr() {}\n",
        &pattern,
    );

    assert!(findings.is_empty());
}
