use super::*;
use super::super::self_host_pattern_ir::PatternNode;

fn literal(text: &str) -> PatternNode {
    PatternNode {
        kind: "literal-pattern".into(),
        text: text.into(),
        children: Vec::new(),
    }
}

#[test]
fn quoted_string_literal_pattern_uses_managed_equality() {
    let source = parse_type("String").expect("String type");
    let pattern = literal(r#""line\nvalue""#);

    validate_pattern_shape(&pattern, &source, "String", None)
        .expect("String literal pattern shape");
    let predicate = pattern_predicate(
        &pattern,
        NativeExpr::Param(0),
        &source,
        "String",
        None,
    )
    .expect("String literal predicate");

    assert!(matches!(predicate, NativeExpr::ManagedOperation { .. }));
}

#[test]
fn unquoted_string_literal_metadata_is_rejected() {
    let source = parse_type("String").expect("String type");
    let pattern = literal("forged");

    let error = pattern_predicate(
        &pattern,
        NativeExpr::Param(0),
        &source,
        "String",
        None,
    )
    .expect_err("unquoted String pattern metadata");

    assert!(error.contains("must be quoted"), "{error}");
}
