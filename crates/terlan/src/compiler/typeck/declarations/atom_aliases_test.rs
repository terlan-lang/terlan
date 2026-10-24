use crate::terlan_typeck::test_support::{
    check_syntax_output, check_syntax_output_with_interfaces,
};

#[test]
fn duplicate_public_atoms_across_modules_are_errors() {
    let diagnostics = check_syntax_output_with_interfaces(
        r#"module app.AlignItems. pub type FlexEnd = Atom["flex-end"]."#,
        &[r#"module app.Keywords. pub type FlexEnd = Atom["flex-end"]."#],
    );
    let diagnostic = diagnostics
        .iter()
        .find(|d| d.message.contains("duplicate_atom_alias"))
        .expect("duplicate public atom must be rejected");
    assert!(diagnostic.message.contains("app.AlignItems.FlexEnd"));
    assert!(diagnostic.message.contains("app.Keywords.FlexEnd"));
    assert!(diagnostic.message.contains("import the shared alias"));
    assert!(matches!(
        diagnostic.severity,
        crate::terlan_typeck::DiagSeverity::Error
    ));
}

#[test]
fn shorthand_and_explicit_atom_declarations_cannot_duplicate() {
    let diagnostics = check_syntax_output_with_interfaces(
        r#"module app.Second. pub type Ready."#,
        &[r#"module app.First. pub type Available = Atom["ready"]."#],
    );
    assert!(diagnostics
        .iter()
        .any(|d| d.message.contains("duplicate_atom_alias")));
}

#[test]
fn duplicate_public_atoms_within_a_module_are_errors() {
    let diagnostics = check_syntax_output(
        r#"module app.Local. pub type Ready. pub type Available = Atom["ready"]."#,
    );
    assert!(diagnostics
        .iter()
        .any(|d| d.message.contains("duplicate_atom_alias")));
}

#[test]
fn importing_conflicting_atom_providers_is_an_error() {
    let diagnostics = check_syntax_output_with_interfaces(
        "module app.Consumer. import type app.First.{Ready}. import type app.Second.{Available}.",
        &[
            r#"module app.First. pub type Ready = Atom["ready"]."#,
            r#"module app.Second. pub type Available = Atom["ready"]."#,
        ],
    );
    assert!(diagnostics
        .iter()
        .any(|d| d.message.contains("duplicate_atom_alias")));
}

#[test]
fn shared_imports_reexports_unions_and_private_aliases_remain_valid() {
    let diagnostics = check_syntax_output_with_interfaces(
        r#"module app.Consumer.
import app.Keywords.{Ready}.
pub type Available = app.Keywords.Ready.
pub type State = Atom["ready"] | Atom["waiting"].
type PrivateReady = Atom["ready"].
pub opaque type DistinctReady = Atom["ready"].
pub ready(): Available -> Ready.
"#,
        &[r#"module app.Keywords. pub type Ready = Atom["ready"]."#],
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn cached_interface_does_not_duplicate_its_own_source_declaration() {
    let source = "module app.State. pub type Ready.";
    let diagnostics = check_syntax_output_with_interfaces(source, &[source]);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn unrelated_cached_conflicts_do_not_reject_independent_modules() {
    let diagnostics = check_syntax_output_with_interfaces(
        "module app.Independent. pub value(): Int -> 1.",
        &[
            r#"module app.First. pub type Ready = Atom["ready"]."#,
            r#"module app.Second. pub type Available = Atom["ready"]."#,
        ],
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}

#[test]
fn atom_alias_identity_uses_parsed_values() {
    let diagnostics = check_syntax_output_with_interfaces(
        r#"module app.Second. pub type Ready = (Atom [ "ready" ])."#,
        &[r#"module app.First. pub type Available = Atom["ready"]."#],
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message.contains("duplicate_atom_alias")),
        "{diagnostics:?}"
    );
}

#[test]
fn duplicate_atom_aliases_are_rejected_through_transitive_facades() {
    let diagnostics = check_syntax_output_with_interfaces(
        "module app.Consumer. import app.Facade.",
        &[
            r#"module app.First. pub type Ready = Atom["ready"]."#,
            r#"module app.Second. pub type Available = Atom["ready"]."#,
            "module app.Facade. pub first(): app.First.Ready. pub second(): app.Second.Available.",
        ],
    );
    assert!(
        diagnostics
            .iter()
            .any(|d| d.message.contains("duplicate_atom_alias")),
        "{diagnostics:?}"
    );
}

#[test]
fn atom_payloads_are_not_treated_as_dependency_paths() {
    let diagnostics = check_syntax_output_with_interfaces(
        "module app.Consumer. import app.Facade.",
        &[
            r#"module app.First. pub type Ready = Atom["ready"]."#,
            r#"module app.Second. pub type Available = Atom["ready"]."#,
            r#"module app.Facade. pub type Label = Atom["app.First.Ready"]. pub label(): Label."#,
        ],
    );
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
}
