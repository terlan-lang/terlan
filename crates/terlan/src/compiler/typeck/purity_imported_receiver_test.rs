use super::test_support::{
    check_syntax_output_with_interface, check_syntax_output_with_std_interfaces,
};

#[test]
fn accepts_pure_primitive_receiver_guard_despite_impure_name_collision() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module purity.string_receiver_guard.\n\
\n\
pub includes_terlan(): Bool ->\n\
    case \"terlan\" {\n\
        text where text.contains(\"terlan\") -> true;\n\
        _ -> false\n\
    }.\n\
",
        "std/core/String.terl",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn rejects_unproven_imported_receiver_guard_with_primitive_method_name() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module purity.receiver_guard_collision.\n\
\n\
import type provider.Box.{Box}.\n\
import provider.Box.\n\
\n\
pub contains_one(box: Box): Bool ->\n\
    case box {\n\
        candidate where candidate.contains(1) -> true;\n\
        _ -> false\n\
    }.\n\
",
        "\
module provider.Box.\n\
\n\
pub type Box.\n\
\n\
pub (box: Box) contains(value: Int): Bool.\n\
",
    );

    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("case guard must be pure; found effectful imported receiver method call")));
}

#[test]
fn accepts_pure_function_calling_proven_pure_imported_receiver_method() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module purity.receiver_pure.\n\
\n\
import type provider.Box.{Box}.\n\
import provider.Box.\n\
\n\
@pure\n\
pub read(box: Box): Int ->\n\
    box.value().\n\
",
        "\
module provider.Box.\n\
\n\
pub type Box.\n\
\n\
@pure\n\
pub (box: Box) value(): Int.\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn rejects_qualified_call_to_impure_imported_receiver_method() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module purity.qualified_receiver_impure.\n\
\n\
import type provider.Box.{Box}.\n\
import provider.Box.\n\
\n\
@pure\n\
pub read(box: Box): Int ->\n\
    Box.value(box).\n\
",
        "\
module provider.Box.\n\
\n\
pub type Box.\n\
\n\
@target.vm {process_mailbox: true}\n\
pub (box: Box) value(): Int.\n\
",
    );

    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("effectful imported function call")));
}

#[test]
fn rejects_qualified_effect_execution_receiver_method() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module purity.qualified_effect_run.\n\
\n\
import std.core.Effect.\n\
\n\
@pure\n\
pub execute(value: Int): Int ->\n\
    Effect.run(Effect.succeed(value)).\n\
",
        "\
module std.core.Effect.\n\
\n\
pub type Effect[T].\n\
\n\
@target.vm {process_mailbox: true}\n\
pub (effect: Effect[T]) run(): T.\n\
\n\
@pure\n\
pub succeed(value: T): Effect[T].\n\
",
    );

    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("effectful imported function call")));
}

#[test]
fn accepts_pure_receiver_combinator_with_pure_callback() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module purity.receiver_callback_pure.\n\
\n\
import type provider.Box.{Box}.\n\
import provider.Box.\n\
\n\
@pure\n\
increment(value: Int): Int -> value + 1.\n\
\n\
@pure\n\
pub transform(box: Box): Box ->\n\
    box.map(increment).\n\
",
        "\
module provider.Box.\n\
\n\
pub type Box.\n\
\n\
@pure\n\
pub (box: Box) map(callback: (Int) -> Int): Box.\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn rejects_pure_receiver_combinator_with_effectful_callback() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module purity.receiver_callback_effectful.\n\
\n\
import type provider.Box.{Box}.\n\
import provider.Box.\n\
\n\
effectful(value: Int): Int -> Box.observe(value).\n\
\n\
@pure\n\
pub transform(box: Box): Box ->\n\
    box.map(effectful).\n\
",
        "\
module provider.Box.\n\
\n\
pub type Box.\n\
\n\
@pure\n\
pub (box: Box) map(callback: (Int) -> Int): Box.\n\
pub observe(value: Int): Int.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("effectful imported receiver method call")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn accepts_pure_receiver_combinator_forwarding_conditional_callback() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module purity.receiver_callback_forwarded.\n\
\n\
import type provider.Box.{Box}.\n\
import provider.Box.\n\
\n\
@pure\n\
pub transform(box: Box, callback: (Int) -> Int): Box ->\n\
    box.map(callback).\n\
",
        "\
module provider.Box.\n\
\n\
pub type Box.\n\
\n\
@pure\n\
pub (box: Box) map(callback: (Int) -> Int): Box.\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn accepts_qualified_receiver_combinator_with_pure_callback() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module purity.qualified_receiver_callback_pure.\n\
\n\
import type provider.Box.{Box}.\n\
import provider.Box.\n\
\n\
@pure\n\
increment(value: Int): Int -> value + 1.\n\
\n\
@pure\n\
pub transform(box: Box): Box ->\n\
    Box.map(box, increment).\n\
",
        "\
module provider.Box.\n\
\n\
pub type Box.\n\
\n\
@pure\n\
pub (box: Box) map(callback: (Int) -> Int): Box.\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn rejects_qualified_receiver_combinator_with_effectful_callback() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module purity.qualified_receiver_callback_effectful.\n\
\n\
import type provider.Box.{Box}.\n\
import provider.Box.\n\
\n\
effectful(value: Int): Int -> Box.observe(value).\n\
\n\
@pure\n\
pub transform(box: Box): Box ->\n\
    Box.map(box, effectful).\n\
",
        "\
module provider.Box.\n\
\n\
pub type Box.\n\
\n\
@pure\n\
pub (box: Box) map(callback: (Int) -> Int): Box.\n\
pub observe(value: Int): Int.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("effectful imported function call")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn accepts_local_combinator_with_pure_callback() {
    let diagnostics = super::test_support::check_syntax_output(
        "\
module purity.local_callback_pure.\n\
\n\
@pure\n\
apply(callback: (Int) -> Int, value: Int): Int -> callback(value).\n\
\n\
@pure\n\
increment(value: Int): Int -> value + 1.\n\
\n\
@pure\n\
pub execute(value: Int): Int -> apply(increment, value).\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn rejects_local_combinator_with_effectful_callback() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module purity.local_callback_effectful.\n\
\n\
import provider.Box.\n\
\n\
@pure\n\
apply(callback: (Int) -> Int, value: Int): Int -> callback(value).\n\
\n\
effectful(value: Int): Int -> Box.observe(value).\n\
\n\
@pure\n\
pub execute(value: Int): Int -> apply(effectful, value).\n\
",
        "\
module provider.Box.\n\
\n\
pub observe(value: Int): Int.\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("effectful local function call")),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn accepts_type_resolved_pure_receiver_despite_impure_name_collision() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module purity.receiver_resolved_collision.\n\
\n\
import type provider.Box.{Box}.\n\
import provider.Box.\n\
\n\
@pure\n\
pub read(box: Box): Int ->\n\
    box.value().\n\
",
        "\
module provider.Box.\n\
\n\
pub type Box.\n\
pub type Other.\n\
\n\
@pure\n\
pub (box: Box) value(): Int.\n\
pub (other: Other) value(): Int.\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn rejects_pure_function_calling_unproven_imported_receiver_method() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module purity.receiver_impure.\n\
\n\
import type provider.Box.{Box}.\n\
import provider.Box.\n\
\n\
@pure\n\
pub read(box: Box): Int ->\n\
    box.value().\n\
",
        "\
module provider.Box.\n\
\n\
pub type Box.\n\
\n\
pub (box: Box) value(): Int.\n\
",
    );

    assert!(diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("effectful imported receiver method call")));
}

#[test]
fn imported_receiver_effect_propagates_through_local_helper() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module purity.receiver_transitive.\n\
\n\
import type provider.Box.{Box}.\n\
import provider.Box.\n\
\n\
read_step(box: Box): Int ->\n\
    box.value().\n\
\n\
@pure\n\
pub read(box: Box): Int ->\n\
    read_step(box).\n\
",
        "\
module provider.Box.\n\
\n\
pub type Box.\n\
\n\
pub (box: Box) value(): Int.\n\
",
    );

    assert!(diagnostics
        .iter()
        .any(|diagnostic| diagnostic.message.contains("effectful local function call")));
}

#[test]
fn imported_receiver_name_does_not_shadow_pure_module_function() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module purity.receiver_module_collision.\n\
\n\
import type provider.Box.{Box}.\n\
import provider.Box.\n\
\n\
@pure\n\
pub normalize(value: Int): Int ->\n\
    Box.normalize(value).\n\
",
        "\
module provider.Box.\n\
\n\
pub type Box.\n\
\n\
@pure\n\
pub normalize(value: Int): Int.\n\
pub (box: Box) normalize(): Int.\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}
