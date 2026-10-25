use super::super::self_host_pattern_ir::PatternNode;
use super::*;

fn binding(name: &str) -> PatternNode {
    PatternNode {
        kind: "binding-pattern".into(),
        text: name.into(),
        children: Vec::new(),
    }
}

fn keyed(field: &str, value: PatternNode) -> PatternNode {
    PatternNode {
        kind: "keyed-container-pattern".into(),
        text: "{}".into(),
        children: vec![PatternNode {
            kind: "keyed-pattern-fields".into(),
            text: String::new(),
            children: vec![PatternNode {
                kind: "keyed-pattern-field".into(),
                text: field.into(),
                children: vec![value],
            }],
        }],
    }
}

#[test]
fn string_keyed_map_pattern_lowers_predicate_and_binding_projection() {
    let pattern = keyed("count", binding("value"));
    let source = parse_type("Map[String, Int]").expect("map type");

    validate_pattern_shape(&pattern, &source, "Map[String, Int]", None)
        .expect("keyed pattern shape");
    pattern_predicate(
        &pattern,
        NativeExpr::Int(0),
        &source,
        "Map[String, Int]",
        None,
    )
    .expect("keyed pattern predicate");
    project_pattern_binding_path(
        &pattern,
        &source,
        "Map[String, Int]",
        NativeExpr::Int(0),
        &[0, 0, 0],
        None,
    )
    .expect("keyed pattern binding projection");
}

#[test]
fn keyed_map_pattern_rejects_non_string_keys() {
    let pattern = keyed("count", binding("value"));
    let source = parse_type("Map[Int, Int]").expect("map type");

    let error = validate_pattern_shape(&pattern, &source, "Map[Int, Int]", None)
        .expect_err("non-string keyed pattern");

    assert!(error.contains("String-keyed Map"), "{error}");
}

#[test]
fn keyed_map_pattern_rejects_malformed_field_shape() {
    let mut pattern = keyed("count", binding("value"));
    pattern.children[0].children[0].children.clear();
    let source = parse_type("Map[String, Int]").expect("map type");

    let error = validate_pattern_shape(&pattern, &source, "Map[String, Int]", None)
        .expect_err("malformed keyed field");

    assert!(
        error.contains("requires one named value pattern"),
        "{error}"
    );
}
