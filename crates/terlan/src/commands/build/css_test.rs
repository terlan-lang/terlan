use super::*;

fn text(value: &str) -> ReplValue {
    ReplValue::String(value.into())
}
fn record(name: &str, fields: Vec<(&str, ReplValue)>) -> ReplValue {
    ReplValue::Record {
        name: name.into(),
        fields: fields
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect(),
    }
}
fn rule(fields: Vec<(&str, ReplValue)>) -> ReplValue {
    ReplValue::Tuple(vec![
        ReplValue::Atom("css_rule".into()),
        text(".tile:hover"),
        record("TileStyle", fields),
    ])
}
fn token(property: &str, css: &str) -> ReplValue {
    record(
        "Token",
        vec![("property", text(property)), ("css", text(css))],
    )
}

#[test]
fn typed_records_keep_declaration_order_and_omit_absent_fields() {
    let rules = ReplValue::List(vec![rule(vec![
        ("width", token("width", "30px")),
        ("margin_left", ReplValue::Atom("none".into())),
        (
            "background_color",
            ReplValue::Tuple(vec![
                ReplValue::Atom("some".into()),
                token("background-color", "transparent"),
            ]),
        ),
    ])]);
    assert_eq!(
        render_stylesheet(&rules).unwrap(),
        ".tile:hover {\n  width: 30px;\n  background-color: transparent;\n}\n"
    );
    assert_eq!(
        render_stylesheet(&record("Ok", vec![("value", rules.clone())])).unwrap(),
        render_stylesheet(&rules).unwrap()
    );
}

#[test]
fn invalid_property_fields_values_and_result_errors_fail_the_build() {
    for fields in [
        vec![("colour", text("red"))],
        vec![("width", ReplValue::Int(30))],
        vec![("width", token("width", "30px; color: red"))],
        vec![
            ("width", token("width", "1px")),
            ("width", token("width", "2px")),
        ],
        vec![("width", token("height", "30px"))],
    ] {
        assert!(render_stylesheet(&ReplValue::List(vec![rule(fields)])).is_err());
    }
    assert!(render_stylesheet(&ReplValue::Tuple(vec![
        ReplValue::Atom("error".into()),
        text("bad theme")
    ]))
    .unwrap_err()
    .contains("bad theme"));
    assert!(
        render_stylesheet(&record("Err", vec![("reason", text("bad theme"))]))
            .unwrap_err()
            .contains("bad theme")
    );
}
