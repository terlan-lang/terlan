use super::*;

#[test]
fn imported_heads_preserve_literals_fields_and_generic_shadowing() {
    let names = HashMap::from([("Endpoint".into(), "app.Left.Endpoint".into())]);
    let source = CoreType::Arrow {
        params: vec![core_type_from_text(
            "{Endpoint: List[Endpoint], literal: Atom[\"Endpoint\"]}",
        )
        .expect("structured type")],
        return_type: Box::new(CoreType::Named("Endpoint".into())),
    };
    let mut qualified = source.clone();
    qualify_type(&mut qualified, &names, &[]);
    assert_eq!(
        qualified,
        CoreType::Arrow {
            params: vec![core_type_from_text(
                "{Endpoint: List[app.Left.Endpoint], literal: Atom[\"Endpoint\"]}",
            )
            .expect("qualified type")],
            return_type: Box::new(CoreType::Named("app.Left.Endpoint".into())),
        }
    );
    for parameter in ["Endpoint", "+Endpoint", "-Endpoint"] {
        let mut shadowed = source.clone();
        qualify_type(&mut shadowed, &names, &[parameter.into()]);
        assert_eq!(shadowed, source);
    }
}
