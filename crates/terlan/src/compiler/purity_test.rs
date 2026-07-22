use std::collections::HashMap;

use super::hir::{resolve_syntax_module_output_with_interfaces, syntax_module_output_to_interface};
use super::syntax::{parse_interface_module_as_syntax_output, parse_module_as_syntax_output};
use super::typeck::type_check_syntax_module_output;

/// Verifies body-available purity survives the public interface boundary.
///
/// Inputs:
/// - Unannotated arithmetic, helper-chain, recursive, and receiver functions.
///
/// Output:
/// - Direct and rendered/reparsed interfaces mark every callable pure.
///
/// Transformation:
/// - Exercises conservative syntax inference before implementation bodies are
///   removed from an importable module summary.
#[test]
fn interfaces_export_inferred_purity_for_body_available_callables() {
    let provider = parse_module_as_syntax_output(
        "\
module provider.Math.\n\
\n\
pub struct Box {\n\
    value: Int\n\
}.\n\
\n\
double(value: Int): Int ->\n\
    value * 2.\n\
\n\
pub normalize(value: Int): Int ->\n\
    double(value) + 1.\n\
\n\
left(value: Int): Int ->\n\
    right(value).\n\
\n\
right(value: Int): Int ->\n\
    left(value).\n\
\n\
pub cycle(value: Int): Int ->\n\
    left(value).\n\
\n\
pub (box: Box) read(): Int ->\n\
    box.value.\n\
",
    )
    .expect("parse inferred-purity provider");
    let interface = syntax_module_output_to_interface(&provider);

    for identity in [
        ("normalize".to_string(), 1),
        ("cycle".to_string(), 1),
        ("read".to_string(), 1),
    ] {
        assert!(
            interface
                .functions
                .get(&identity)
                .is_some_and(|signature| signature.pure),
            "missing inferred purity for {identity:?}"
        );
    }

    let rendered = interface.to_terlan_interface_type_text();
    assert!(rendered.contains("@pure\npub normalize(value: Int): Int."));
    assert!(rendered.contains("@pure\npub cycle(value: Int): Int."));
    assert!(rendered.contains("@pure\npub (box: Box) read(): Int."));
    let reparsed = parse_interface_module_as_syntax_output(&rendered)
        .expect("reparse inferred-purity interface");
    let reparsed_interface = syntax_module_output_to_interface(&reparsed);
    assert!(reparsed_interface
        .functions
        .get(&("normalize".to_string(), 1))
        .is_some_and(|signature| signature.pure));
}

/// Verifies unrelated callable metadata does not disable body inference.
///
/// Inputs:
/// - A body-available public test function carrying `@test`, but not `@pure`.
///
/// Output:
/// - Its generated interface records compiler-inferred purity.
///
/// Transformation:
/// - Locks the requirement that purity inference covers all body-available
///   functions rather than only declarations with an empty annotation list.
#[test]
fn interfaces_infer_purity_for_body_available_annotated_callables() {
    let provider = parse_module_as_syntax_output(
        "\
module provider.Annotated.\n\
\n\
@test\n\
pub succeeds(): Bool ->\n\
    true.\n\
",
    )
    .expect("parse annotated inferred-purity provider");
    let interface = syntax_module_output_to_interface(&provider);

    assert!(
        interface
            .functions
            .get(&("succeeds".to_string(), 0))
            .is_some_and(|signature| signature.pure),
        "body-available @test function should receive inferred purity"
    );
}

/// Verifies interface inference refuses unproven effect boundaries.
///
/// Inputs:
/// - Indexed mutation, a qualified unknown call, a dynamic function call, and
///   a compiler-native declaration body.
///
/// Output:
/// - No corresponding public signature receives inferred purity.
///
/// Transformation:
/// - Locks the inference pass to positive proof rather than optimistic absence
///   of a known standard-library operation name.
#[test]
fn interfaces_do_not_infer_purity_across_unproven_effect_boundaries() {
    let provider = parse_module_as_syntax_output(
        "\
module provider.Effects.\n\
\n\
import provider.External.\n\
\n\
pub mutate(items: List[Int]): Unit ->\n\
    items[0] = 1.\n\
\n\
pub load(value: Int): Int ->\n\
    External.load(value).\n\
\n\
pub apply(callback: (Int) -> Int, value: Int): Int ->\n\
    callback(value).\n\
\n\
@compiler.native {provider.effects.connect}\n\
pub connect(): Dynamic ->\n\
    native.\n\
",
    )
    .expect("parse effectful provider");
    let interface = syntax_module_output_to_interface(&provider);

    for identity in [
        ("mutate".to_string(), 1),
        ("load".to_string(), 1),
        ("apply".to_string(), 2),
        ("connect".to_string(), 0),
    ] {
        assert!(
            interface
                .functions
                .get(&identity)
                .is_some_and(|signature| !signature.pure),
            "unexpected inferred purity for {identity:?}"
        );
    }
}

/// Verifies consumers use inferred provider purity without source annotations.
///
/// Inputs:
/// - An unannotated pure provider implementation and an `@pure` consumer.
///
/// Output:
/// - Consumer typechecking succeeds with no purity diagnostics.
///
/// Transformation:
/// - Converts the provider body to an interface, resolves the consumer against
///   that interface, and validates the cross-module call contract.
#[test]
fn pure_consumer_accepts_cross_module_inferred_purity() {
    let provider = parse_module_as_syntax_output(
        "\
module provider.Math.\n\
\n\
pub normalize(value: Int): Int ->\n\
    value * 100.\n\
",
    )
    .expect("parse pure provider");
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let consumer = parse_module_as_syntax_output(
        "\
module consumer.App.\n\
\n\
import provider.Math.\n\
\n\
@pure\n\
pub run(value: Int): Int ->\n\
    Math.normalize(value).\n\
",
    )
    .expect("parse pure consumer");
    let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&consumer, &resolved);

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
}

/// Verifies an effectful provider remains unavailable to a pure consumer.
///
/// Inputs:
/// - An unannotated provider with indexed mutation and an `@pure` consumer.
///
/// Output:
/// - Consumer typechecking reports the imported effectful call.
///
/// Transformation:
/// - Converts the provider to an interface before resolving the consumer, so
///   this exercises the same summary boundary used by separately built code.
#[test]
fn pure_consumer_rejects_cross_module_unproven_effect() {
    let provider = parse_module_as_syntax_output(
        "\
module provider.State.\n\
\n\
pub replace_first(items: List[Int], value: Int): Unit ->\n\
    items[0] = value.\n\
",
    )
    .expect("parse effectful provider");
    let mut interfaces = HashMap::new();
    interfaces.insert(
        provider.module_name.clone(),
        syntax_module_output_to_interface(&provider),
    );
    let consumer = parse_module_as_syntax_output(
        "\
module consumer.App.\n\
\n\
import provider.State.\n\
\n\
@pure\n\
pub run(items: List[Int]): Unit ->\n\
    State.replace_first(items, 1).\n\
",
    )
    .expect("parse pure consumer");
    let resolved = resolve_syntax_module_output_with_interfaces(&consumer, &interfaces).module;
    let diagnostics = type_check_syntax_module_output(&consumer, &resolved);

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "function run annotated @pure must be pure; found effectful imported function call"
            )),
        "missing imported effect diagnostic: {diagnostics:?}"
    );
}
