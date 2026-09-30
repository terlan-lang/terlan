//! Fragment APIs execute package source, independently of standard names.

use super::super::source_constructor_test::check_sources;

#[test]
fn template_fragment_source_handles_empty_and_ordered_trusted_text() {
    check_sources(&[
        r#"module app.Fragment.
import std.template.Template.
pub check(): Bool ->
    Template.to_string(Template.empty()) == "" and
    Template.to_string(Template.trusted("<&\"'>")) == "<&\"'>" and
    Template.to_string(Template.join([])) == "" and
    Template.to_string(Template.join([Template.trusted("<p>"), Template.empty(), Template.trusted("hi</p>")])) == "<p>hi</p>".
"#,
        include_str!("../../../../../std/template/Template.terl"),
        include_str!("../../../../../std/core/String.terl"),
    ]);
}

#[test]
fn template_helpers_follow_provider_bodies_and_renaming() {
    for provider in ["std.template.Template", "app.Fragments"] {
        let declaration = format!(
            r#"module {provider}.
pub opaque type Html = String.
pub trusted(value: String): Html -> Html("trusted:" + value).
pub empty(): Html -> Html("empty").
pub join(values: List[Html]): Html -> case values {{ [] -> Html("none"); [first | _] -> first }}.
pub to_string(value: Html): String -> value.
"#
        );
        let caller = format!(
            r#"module app.CustomFragments.
import {provider} as Fragments.
pub check(): Bool ->
    Fragments.to_string(Fragments.trusted("x")) == "trusted:x" and
    Fragments.to_string(Fragments.empty()) == "empty" and
    Fragments.to_string(Fragments.join([])) == "none" and
    Fragments.to_string(Fragments.join([Fragments.trusted("first"), Fragments.trusted("second")])) == "trusted:first".
"#
        );
        check_sources(&[&caller, &declaration]);
    }
}

#[test]
fn ordinary_opaque_scalar_constructors_execute_without_library_names() {
    check_sources(&[r#"module app.Identifiers.
pub opaque type Identifier = Int.
make(value: Int): Identifier -> Identifier(value).
read(value: Identifier): Int -> value.
pub check(): Bool -> read(make(42)) == 42 and read(make(-7)) == -7.
"#]);
}

#[test]
fn fragment_representation_is_not_available_to_importing_callers() {
    use crate::terlan_hir::{
        resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
    };
    use crate::terlan_syntax::parse_module_as_syntax_output;
    use crate::terlan_typeck::type_check_syntax_module_output;
    let provider =
        parse_module_as_syntax_output(include_str!("../../../../../std/template/Template.terl"))
            .expect("template source");
    for body in ["\"raw\"", "Html(\"raw\")", "Template.join([\"raw\"])"] {
        let source = format!("module app.Untrusted.\nimport std.template.Template.\nimport std.template.Template.{{Html}}.\npub bad(): Template.Html -> {body}.\n");
        let syntax = parse_module_as_syntax_output(&source).expect("caller source");
        let interfaces = std::collections::HashMap::from([(
            "std.template.Template".into(),
            syntax_module_output_to_interface(&provider),
        )]);
        let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
        let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
        assert!(!diagnostics.is_empty(), "untrusted caller accepted: {body}");
    }
}
