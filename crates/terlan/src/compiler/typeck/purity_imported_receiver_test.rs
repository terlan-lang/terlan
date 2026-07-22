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
