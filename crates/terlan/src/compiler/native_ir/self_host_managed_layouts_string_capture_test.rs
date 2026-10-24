use super::*;
use super::super::self_host_pattern_ir::PatternNode;

fn segment(kind: &str, text: &str) -> PatternNode {
    PatternNode {
        kind: kind.into(),
        text: text.into(),
        children: Vec::new(),
    }
}

fn string_pattern(children: Vec<PatternNode>) -> PatternNode {
    PatternNode {
        kind: "string-capture-pattern".into(),
        text: r#""users/${id: Int}.json""#.into(),
        children,
    }
}

#[test]
fn segmented_string_capture_lowers_match_and_typed_extraction() {
    let pattern = string_pattern(vec![
        segment("string-literal-segment", "users/"),
        segment("string-capture-segment", "id: Int"),
        segment("string-literal-segment", ".json"),
    ]);
    let source = parse_type("String").expect("String type");

    validate_pattern_shape(&pattern, &source, "String", None)
        .expect("string capture shape");
    pattern_predicate(
        &pattern,
        NativeExpr::Param(0),
        &source,
        "String",
        None,
    )
    .expect("string capture predicate");
    assert_eq!(pattern.bindings()[0].path, vec![1]);
    assert_eq!(
        binding_type_at_pattern(&pattern, &source, "String", &[1], None)
            .expect("capture binding type"),
        parse_type("Int").expect("Int type"),
    );
    project_pattern_binding_path(
        &pattern,
        &source,
        "String",
        NativeExpr::Param(0),
        &[1],
        None,
    )
    .expect("capture extraction");
}

#[test]
fn segmented_string_capture_rejects_unsupported_converter() {
    let pattern = string_pattern(vec![
        segment("string-literal-segment", "users/"),
        segment("string-capture-segment", "items: List[Int]"),
    ]);
    let source = parse_type("String").expect("String type");

    let error = validate_pattern_shape(&pattern, &source, "String", None)
        .expect_err("unsupported capture converter");

    assert!(error.contains("no native capture converter"), "{error}");
}

#[test]
fn segmented_string_capture_rejects_adjacent_captures() {
    let pattern = string_pattern(vec![
        segment("string-capture-segment", "left"),
        segment("string-capture-segment", "right"),
    ]);
    let source = parse_type("String").expect("String type");

    let error = validate_pattern_shape(&pattern, &source, "String", None)
        .expect_err("adjacent captures");

    assert!(error.contains("adjacent captures"), "{error}");
}
