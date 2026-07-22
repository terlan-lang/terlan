use super::test_support::{check_syntax_output, check_syntax_output_with_std_interfaces};
use crate::terlan_hir::syntax_module_output_to_interface;
use crate::terlan_syntax::parse_module_as_syntax_output;

/// Verifies closure construction does not execute an effectful closure body.
///
/// The returned closure captures a mutable collection and performs indexed
/// assignment only when invoked. Constructing that deferred function value is
/// therefore valid inside an asserted pure function.
#[test]
fn pure_function_accepts_deferred_effectful_closure_construction() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module pure_deferred_closure.\n\
\n\
import std.io.Console.\n\
\n\
pub type DeferredWrite = (String) -> Unit.\n\
\n\
@pure\n\
pub plan(): DeferredWrite ->\n\
    (value: String) -> Console.println(value).\n\
",
        "std/io/Console.terl",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
}

/// Verifies a function-value invocation is effectful without purity evidence.
///
/// Terlan function types do not yet carry purity contracts, so invoking a
/// callback from an asserted pure function must fail closed even when its
/// input and output types otherwise match.
#[test]
fn pure_function_rejects_unproven_function_value_invocation() {
    let diagnostics = check_syntax_output(
        "\
module pure_callback_invocation.\n\
\n\
@pure\n\
pub execute(callback: (Int) -> Int, value: Int): Int ->\n\
    callback(value).\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "function execute annotated @pure must be pure; found unproven function-value call"
            )),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies callback effects propagate through ordinary local helpers.
#[test]
fn function_value_invocation_effect_propagates_through_local_helper() {
    let diagnostics = check_syntax_output(
        "\
module transitive_callback_invocation.\n\
\n\
pub type Callback = (Int) -> Int.\n\
\n\
pub invoke(callback: Callback, value: Int): Int ->\n\
    callback(value).\n\
\n\
@pure\n\
pub execute(callback: Callback, value: Int): Int ->\n\
    invoke(callback, value).\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains(
            "function execute annotated @pure must be pure; found effectful local function call"
        )
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies conservative purity checking does not prohibit ordinary callbacks.
#[test]
fn ordinary_function_accepts_unproven_function_value_invocation() {
    let diagnostics = check_syntax_output(
        "\
module ordinary_callback_invocation.\n\
\n\
pub execute(callback: (Int) -> Int, value: Int): Int ->\n\
    callback(value).\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {diagnostics:?}"
    );
}

/// Verifies exported inferred purity uses the same deferred-body boundary.
#[test]
fn interface_purity_distinguishes_closure_construction_from_invocation() {
    let provider = parse_module_as_syntax_output(
        "\
module provider.Deferred.\n\
\n\
pub type DeferredWrite = (Int) -> Unit.\n\
\n\
pub plan(items: List[Int]): DeferredWrite ->\n\
    (value: Int) -> items[0] = value.\n\
\n\
pub execute(callback: (Int) -> Int, value: Int): Int ->\n\
    callback(value).\n\
",
    )
    .expect("parse deferred-closure provider");
    let interface = syntax_module_output_to_interface(&provider);

    assert!(
        interface
            .functions
            .get(&("plan".to_string(), 1))
            .is_some_and(|signature| signature.pure),
        "closure construction should receive inferred purity"
    );
    assert!(
        interface
            .functions
            .get(&("execute".to_string(), 2))
            .is_some_and(|signature| !signature.pure),
        "function-value invocation must remain effectful without purity evidence"
    );
}
