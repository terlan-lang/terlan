use std::collections::HashMap;

use super::*;
use crate::terlan_hir::{
    resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface,
};
use crate::terlan_syntax::{
    parse_interface_module_as_syntax_output, parse_module_as_syntax_output,
};

/// Imported constructors and provider functions return the same nominal identity.
#[test]
fn imported_struct_constructor_and_function_return_share_provider_identity() {
    for (provider, result) in [
        (
            "pub struct Summary { valid: Bool }. pub load(): Summary.",
            "Summary",
        ),
        (
            "pub struct Summary[T] { valid: T }. pub load(): Summary[Bool].",
            "Summary[Bool]",
        ),
    ] {
        let source = format!(
            "module identity_consumer.\n\
             import fixture.Provider.\n\
             import fixture.Provider.{{Summary}}.\n\
             import type fixture.Provider.Summary.\n\
             pub inspect(flag: Bool): {result} -> case flag {{\n\
                 true -> Provider.load(); false -> Summary(valid = false)\n\
             }}.\n"
        );
        let diagnostics = check_syntax_output_with_interfaces(
            &source,
            &[&format!("module fixture.Provider. {provider}")],
        );
        assert!(diagnostics.is_empty(), "{provider}: {diagnostics:?}");
    }
}

/// Imported field annotations must resolve dependency aliases without consumer type imports.
#[test]
fn imported_struct_optional_field_preserves_dependency_types() {
    let option = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../std/summaries/std.core.Option.typi"
    ));
    for (provider, expression) in [
        (
            "module fixture.Receipts.\npub struct Receipt { value: Option[Int] }.\npub load(): Option[Receipt].\n",
            "value",
        ),
        (
            "module fixture.Receipts.\npub struct Receipt[T] { value: Option[T] }.\npub load(): Option[Receipt[Int]].\n",
            "value",
        ),
        (
            "module fixture.Receipts.\npub struct Payload { count: Int }.\npub struct Receipt { value: Option[Payload] }.\npub load(): Option[Receipt].\n",
            "value.count",
        ),
        (
            "module fixture.Receipts.\npub struct Receipt { value: Option[Detail] }.\npub load(): Option[Receipt].\n",
            "value.count",
        ),
    ] {
        let source = format!(
            "module receipt_consumer.\n\
             import fixture.Receipts.\n\
             import std.core.Option.{{None, Some}}.\n\
             pub inspect(): Int -> case Receipts.load() {{\n\
                 None -> 0;\n\
                 Some(receipt) -> case receipt.value {{ None -> 0; Some(value) -> {expression} }}\n\
             }}.\n"
        );
        let dependency = "module fixture.External.\npub struct Detail { count: Int }.\n";
        let diagnostics = check_syntax_output_with_interfaces(&source, &[option, provider, dependency]);
        assert!(diagnostics.is_empty(), "{provider}: {diagnostics:?}");
    }
}

/// Resolving a dependency alias must not erase its payload into a permissive type variable.
#[test]
fn imported_struct_optional_field_rejects_wrong_payload_use() {
    let option = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../std/summaries/std.core.Option.typi"
    ));
    let provider = "module fixture.Receipts.\npub struct Receipt { value: Option[Bool] }.\npub load(): Receipt.\n";
    let source = "module receipt_consumer.\n\
        import fixture.Receipts.\n\
        import std.core.Option.{None, Some}.\n\
        pub inspect(): Int -> case Receipts.load().value { None -> 0; Some(value) -> value }.\n";
    let diagnostics = check_syntax_output_with_interfaces(source, &[option, provider]);
    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("expected Int found Bool")),
        "must reject the concrete Bool payload: {diagnostics:?}"
    );
}

/// Verifies provider-qualified struct fields remain assignable to a selected
/// transparent union alias from the same provider.
///
/// Inputs:
/// - A provider interface with a struct field typed as a transparent union and
///   another provider exporting a colliding nominal short name.
/// - A consumer that imports both the struct and union, then passes the field
///   to a local function expecting the selected union alias.
///
/// Output:
/// - No type diagnostics.
///
/// Transformation:
/// - Locks provider-local nominal members in imported alias bodies to their
///   qualified identity so field inference and local call inference agree.
#[test]
fn syntax_output_accepts_imported_transparent_union_struct_field_as_selected_alias() {
    let provider = parse_interface_module_as_syntax_output(
        "\
module provider.\n\
\n\
pub type Accepted = Atom[\"accepted\"].\n\
pub struct OutOfOrder {\n\
    expected: Int\n\
}.\n\
pub type Outcome = Accepted | OutOfOrder.\n\
pub struct Result {\n\
    outcome: Outcome\n\
}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse provider interface fixture: {:?}", err));
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let colliding_provider = parse_interface_module_as_syntax_output(
        "\
module other_provider.\n\
\n\
pub struct OutOfOrder {\n\
    marker: String\n\
}.\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse colliding interface fixture: {:?}", err));
    interfaces.insert(
        colliding_provider.module_name.clone(),
        syntax_module_output_to_interface(&colliding_provider),
    );
    let module = parse_module_as_syntax_output(
        "\
module consumer.\n\
\n\
import type provider.{Outcome, Result}.\n\
\n\
pub accepts(outcome: Outcome): Bool ->\n\
    outcome == outcome.\n\
\n\
pub inspect(result: Result): Bool ->\n\
    accepts(result.outcome).\n",
    )
    .unwrap_or_else(|err| panic!("failed to parse consumer syntax output fixture: {:?}", err));
    let resolved = resolve_syntax_module_output_with_interfaces(&module, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&module, &resolved);

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}
