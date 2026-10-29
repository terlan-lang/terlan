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
    qualify_type(&mut qualified, &names, &HashMap::new(), &[]);
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
        qualify_type(&mut shadowed, &names, &HashMap::new(), &[parameter.into()]);
        assert_eq!(shadowed, source);
    }
}

#[test]
fn selected_transparent_alias_uses_provider_identity_in_core_signatures() {
    let provider = crate::terlan_syntax::parse_module_as_syntax_output(
        "module fixture.Policy.\npub type Continue.\npub type Stop = {Atom[\"stop\"], Int}.\npub type Decision = Continue | Stop.\n",
    )
    .expect("provider syntax");
    let interfaces = HashMap::from([(
        provider.module_name.clone(),
        crate::terlan_hir::syntax_module_output_to_interface(&provider),
    )]);
    let consumer = crate::terlan_syntax::parse_module_as_syntax_output(
        "module fixture.Consumer.\nimport type fixture.Policy.{Decision as Result}.\npub choose(value: Result): Result -> value.\n",
    )
    .expect("consumer syntax");
    let resolved =
        crate::terlan_hir::resolve_syntax_module_output_with_interfaces(&consumer, &interfaces)
            .module;
    let core = super::super::lower_syntax_module_output_to_core(&consumer, &resolved);
    let choose = core
        .functions
        .iter()
        .find(|function| function.name == "choose")
        .expect("choose function");

    let canonical = CoreType::Named("fixture.Policy.Decision".to_string());
    assert_eq!(choose.params[0].core_ty, Some(canonical.clone()));
    assert_eq!(choose.core_return_type, Some(canonical));
}

#[test]
fn unrenamed_standard_alias_keeps_structural_core_identity() {
    let provider = crate::terlan_syntax::parse_module_as_syntax_output(
        "module std.core.Option.\npub type None.\npub type Some[T] = {Atom[\"some\"], T}.\npub type Option[T] = None | Some[T].\n",
    )
    .expect("provider syntax");
    let interfaces = HashMap::from([(
        provider.module_name.clone(),
        crate::terlan_hir::syntax_module_output_to_interface(&provider),
    )]);
    let consumer = crate::terlan_syntax::parse_module_as_syntax_output(
        "module fixture.Consumer.\nimport type std.core.Option.\npub keep(value: Option[String]): Option[String] -> value.\n",
    )
    .expect("consumer syntax");
    let resolved =
        crate::terlan_hir::resolve_syntax_module_output_with_interfaces(&consumer, &interfaces)
            .module;
    let core = super::super::lower_syntax_module_output_to_core(&consumer, &resolved);
    let keep = core
        .functions
        .iter()
        .find(|function| function.name == "keep")
        .expect("keep function");

    let structural = CoreType::Apply {
        constructor: "Option".to_string(),
        args: vec![CoreType::String],
    };
    assert_eq!(keep.params[0].core_ty, Some(structural.clone()));
    assert_eq!(keep.core_return_type, Some(structural));
}
