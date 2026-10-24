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
        render_stylesheet_with_options(&rules, &BTreeSet::from([("TileStyle".into(), "margin_left".into())])).unwrap(),
        ".tile:hover {\n  width: 30px;\n  background-color: transparent;\n}\n"
    );
    assert_eq!(
        render_stylesheet_with_options(&record("Ok", vec![("value", rules.clone())]), &BTreeSet::from([("TileStyle".into(), "margin_left".into())])).unwrap(),
        render_stylesheet_with_options(&rules, &BTreeSet::from([("TileStyle".into(), "margin_left".into())])).unwrap()
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

#[test]
fn direct_enums_and_typed_units_serialize_without_property_wrappers() {
    let length = record("std.css.Values.Length", vec![("amount", ReplValue::Float("-10.0".into())), ("unit", ReplValue::Atom("px".into()))]);
    let rules = ReplValue::List(vec![rule(vec![
        ("animation_direction", ReplValue::Atom("alternate".into())),
        ("display", ReplValue::Atom("none".into())),
        ("margin_left", length),
    ])]);
    assert_eq!(render_stylesheet(&rules).unwrap(), ".tile:hover {\n  animation-direction: alternate;\n  display: none;\n  margin-left: -10px;\n}\n");
    assert!(render_stylesheet(&ReplValue::List(vec![rule(vec![("display", ReplValue::Atom("alternate".into()))])])).is_err());
}

#[test]
fn optional_none_and_some_css_none_are_distinct() {
    let optional = BTreeSet::from([("TileStyle".into(), "display".into())]);
    let omitted = ReplValue::List(vec![rule(vec![("display", ReplValue::Atom("none".into()))])]);
    assert_eq!(render_stylesheet_with_options(&omitted, &optional).unwrap(), ".tile:hover {\n}\n");
    let explicit = ReplValue::List(vec![rule(vec![("display", ReplValue::Tuple(vec![ReplValue::Atom("some".into()), ReplValue::Atom("none".into())]))])]);
    assert_eq!(render_stylesheet_with_options(&explicit, &optional).unwrap(), ".tile:hover {\n  display: none;\n}\n");
    let mut interface = crate::terlan_hir::ModuleInterface { module:"example.Styles".into(), ..Default::default() };
    interface.struct_fields.insert("TileStyle".into(), vec![crate::terlan_hir::StructFieldSignature {name:"display".into(), annotation:"std.core.Option.Option[Atom[\"none\"]]".into(), is_private:false}]);
    let fields = optional_style_fields(&[interface]);
    assert!(fields.contains(&("example.Styles.TileStyle".into(), "display".into())));
}

#[test]
fn compound_records_serialize_their_typed_parts() {
    let sequence = record("std.css.Values.Sequence", vec![("parts", ReplValue::List(vec![
        record("std.css.Values.Length", vec![("amount", ReplValue::Float("0.0".into())), ("unit", ReplValue::Atom("px".into()))]),
        ReplValue::Atom("auto".into()),
    ]))]);
    let rules = ReplValue::List(vec![rule(vec![("margin", sequence)])]);
    assert_eq!(render_stylesheet(&rules).unwrap(), ".tile:hover {\n  margin: 0px auto;\n}\n");
}
