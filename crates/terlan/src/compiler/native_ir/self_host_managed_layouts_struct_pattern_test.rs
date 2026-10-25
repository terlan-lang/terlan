use super::super::self_host_adapter::LocalStructField;
use super::super::self_host_pattern_ir::PatternNode;
use super::*;

fn binding(name: &str) -> PatternNode {
    PatternNode {
        kind: "binding-pattern".into(),
        text: name.into(),
        children: Vec::new(),
    }
}

fn structure(name: &str, field: &str, value: PatternNode) -> PatternNode {
    PatternNode {
        kind: "struct-pattern".into(),
        text: name.into(),
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

fn catalog() -> StructPatternCatalog {
    let mut fields = std::collections::HashMap::new();
    fields.insert(
        "count".into(),
        LocalStructField {
            index: 1,
            type_name: "Int".into(),
        },
    );
    let mut structs = std::collections::HashMap::new();
    structs.insert("app.Widget".into(), fields);
    structs
}

#[test]
fn named_struct_pattern_uses_catalog_field_type_and_index() {
    let pattern = structure("Widget", "count", binding("value"));
    let source = parse_type("app.Widget").expect("struct type");
    let structs = catalog();
    let catalogs = PatternCatalogs {
        variants: None,
        structs: Some(&structs),
    };

    validate_pattern_shape_with_catalogs(&pattern, &source, "app.Widget", catalogs)
        .expect("struct pattern shape");
    pattern_predicate_with_catalogs(
        &pattern,
        NativeExpr::Param(0),
        &source,
        "app.Widget",
        catalogs,
    )
    .expect("struct pattern predicate");
    assert_eq!(
        binding_type_at_pattern_with_catalogs(
            &pattern,
            &source,
            "app.Widget",
            &[0, 0, 0],
            catalogs,
        )
        .expect("struct binding type"),
        parse_type("Int").expect("Int type"),
    );
    project_pattern_binding_path_with_catalogs(
        &pattern,
        &source,
        "app.Widget",
        NativeExpr::Param(0),
        &[0, 0, 0],
        catalogs,
    )
    .expect("struct binding projection");
}

#[test]
fn named_struct_pattern_rejects_unknown_field() {
    let pattern = structure("Widget", "missing", binding("value"));
    let source = parse_type("app.Widget").expect("struct type");
    let structs = catalog();

    let error = validate_pattern_shape_with_catalogs(
        &pattern,
        &source,
        "app.Widget",
        PatternCatalogs {
            variants: None,
            structs: Some(&structs),
        },
    )
    .expect_err("unknown struct field");

    assert!(error.contains("has no field"), "{error}");
}

#[test]
fn named_struct_pattern_rejects_source_type_mismatch() {
    let pattern = structure("Other", "count", binding("value"));
    let source = parse_type("app.Widget").expect("struct type");
    let structs = catalog();

    let error = validate_pattern_shape_with_catalogs(
        &pattern,
        &source,
        "app.Widget",
        PatternCatalogs {
            variants: None,
            structs: Some(&structs),
        },
    )
    .expect_err("mismatched struct name");

    assert!(error.contains("disagrees with source type"), "{error}");
}
