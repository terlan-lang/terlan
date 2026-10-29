//! Renamed constructors retain provider identity during closed-union coverage.

use super::*;
use crate::terlan_hir::{
    resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
};

fn interfaces() -> HashMap<String, crate::terlan_hir::ModuleInterface> {
    [
        r#"module fixture.First.
pub type Outcome = {Atom["accepted"], Int} | {Atom["rejected"], Bool}.
pub constructor Accepted { (value: Int): Outcome -> {Atom["accepted"], value} }.
pub constructor Rejected { (flag: Bool): Outcome -> {Atom["rejected"], flag} }.
"#,
        r#"module fixture.Second.
pub type Outcome = {Atom["completed"], String} | {Atom["rejected"], String}.
pub constructor Completed { (value: String): Outcome -> {Atom["completed"], value} }.
pub constructor Rejected { (detail: String): Outcome -> {Atom["rejected"], detail} }.
"#,
    ]
    .into_iter()
    .map(|source| {
        let syntax = crate::terlan_syntax::parse_module_as_syntax_output(source).expect("provider");
        let text = syntax_module_output_to_interface(&syntax).to_terlan_interface_text();
        crate::terlan_hir::parse_interface_text(&text).expect("round-trip provider interface")
    })
    .collect()
}

fn check(body: &str) -> Vec<Diagnostic> {
    let source = format!(
        "module fixture.Consumer.\n\
         import fixture.First.{{Accepted, Rejected as FirstFailure}}.\n\
         import fixture.Second.{{Completed, Rejected as SecondFailure}}.\n\
         import type fixture.First.{{Outcome as FirstOutcome}}.\n\
         import type fixture.Second.{{Outcome as SecondOutcome}}.\n{body}\n"
    );
    let syntax = crate::terlan_syntax::parse_module_as_syntax_output(&source).expect("consumer");
    let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces()).module;
    type_check_syntax_module_output(&syntax, &resolved)
}

#[test]
fn aliased_constructor_coverage_preserves_same_named_provider_identities() {
    let diagnostics = check(
        "pub first(value: FirstOutcome): Bool -> case value { FirstFailure(flag) -> flag; Accepted(n) -> n == 7 }.\n\
         pub second(value: SecondOutcome): Bool -> case value { Completed(text) -> text == \"ok\"; SecondFailure(text) -> text == \"no\" }."
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn nested_aliased_constructor_payloads_are_refined_jointly() {
    let diagnostics = check(
        "pub first(value: {FirstOutcome, Bool}): Bool -> case value {\n\
         {FirstFailure(true), _} -> true;\n\
         {FirstFailure(false), _} -> false;\n\
         {Accepted(_), _} -> false }.",
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn aliased_constructor_coverage_rejects_missing_guarded_and_wrong_provider_cases() {
    for branches in [
        "Accepted(_) -> true",
        "Accepted(_) -> true; FirstFailure(true) -> true",
        "Accepted(_) -> true; FirstFailure(true) -> true; FirstFailure(false) where false -> false",
    ] {
        let diagnostics = check(&format!(
            "pub first(value: FirstOutcome): Bool -> case value {{ {branches} }}."
        ));
        assert!(
            diagnostics
                .iter()
                .any(|d| d.message.contains("non-exhaustive case")),
            "{branches}: {diagnostics:?}"
        );
    }
    let diagnostics = check("pub first(value: FirstOutcome): Bool -> case value { Accepted(_) -> true; SecondFailure(_) -> false }.");
    assert!(
        !diagnostics.is_empty(),
        "foreign provider constructor accepted"
    );
}

#[test]
fn imported_atom_constructor_coverage_agrees_locally_and_after_interface_roundtrip() {
    let mut interfaces = HashMap::new();
    for source in [
        include_str!("../../../../../../tests/fixtures/constructor-atom-alias/Tags.terl"),
        include_str!("../../../../../../tests/fixtures/constructor-atom-alias/State.terl"),
        include_str!("../../../../../../tests/fixtures/constructor-atom-alias/StateTest.terl"),
    ] {
        let syntax = crate::terlan_syntax::parse_module_as_syntax_output(source).expect("module");
        let resolved = resolve_syntax_module_output_with_interfaces(&syntax, &interfaces).module;
        let diagnostics = type_check_syntax_module_output(&syntax, &resolved);
        assert!(
            diagnostics.is_empty(),
            "{}: {diagnostics:?}",
            syntax.module_name
        );
        let text = syntax_module_output_to_interface(&syntax).to_terlan_interface_text();
        let (name, interface) = crate::terlan_hir::parse_interface_text(&text).expect("interface");
        interfaces.insert(name, interface);
    }
}
