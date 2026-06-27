use super::test_support::*;

/// Verifies adversarial constructor calls diagnose instead of lowering.
///
/// Inputs:
/// - Source calling an unresolved uppercase constructor.
///
/// Output:
/// - Test passes when typechecking reports the unknown constructor.
///
/// Transformation:
/// - Exercises the formal syntax-output path used before backend emission.
#[test]
fn adversarial_typecheck_rejects_unresolved_constructor_call() {
    let diagnostics = check_syntax_output(
        "\
module adversarial_constructor.\n\
\n\
pub main(): Unit ->\n\
    MissingConstructor(1).\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unknown constructor")),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies malformed generic arity is rejected before backend lowering.
///
/// Inputs:
/// - Source applying two type arguments to `Option`.
///
/// Output:
/// - Test passes when diagnostics report the invalid type application.
///
/// Transformation:
/// - Protects generic type instantiation from accepting arity drift.
#[test]
fn adversarial_typecheck_rejects_generic_arity_mismatch() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module adversarial_generic_arity.\n\
\n\
import std.core.Option.{Option, Some, None}.\n\
\n\
pub value(): Option[Int, String] ->\n\
    None.\n\
",
        "std/core/option.terl",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains("type")
                || diagnostic.message.contains("arity")
                || diagnostic.message.contains("argument")
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies private struct fields are not exposed through pattern matching.
///
/// Inputs:
/// - Provider interface with a private field and user source pattern matching
///   on that field.
///
/// Output:
/// - Test passes when field visibility diagnostics are emitted.
///
/// Transformation:
/// - Exercises imported module visibility rules on an adversarial pattern
///   rather than ordinary field access.
#[test]
fn adversarial_typecheck_rejects_private_imported_field_pattern() {
    let diagnostics = check_syntax_output_with_interface(
        "\
module consumer.\n\
\n\
import type provider.User.User.\n\
\n\
pub read(user: User): Int ->\n\
    case user {\n\
        User { #id = id } -> id\n\
    }.\n\
",
        "\
module provider.User.\n\
\n\
pub struct User {\n\
    #id: Int,\n\
    name: String\n\
}.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains("private") || diagnostic.message.contains("field")
        }),
        "diagnostics: {diagnostics:?}"
    );
}

/// Verifies non-constant parameter defaults cannot reference later parameters.
///
/// Inputs:
/// - Function parameter default that references another parameter.
///
/// Output:
/// - Test passes when the default-value validator rejects the runtime
///   dependency.
///
/// Transformation:
/// - Guards against a subtle source-order dependency becoming accepted by
///   parser/typechecker changes.
#[test]
fn adversarial_typecheck_rejects_parameter_default_referencing_parameter() {
    let diagnostics = check_syntax_output(
        "\
module adversarial_default_reference.\n\
\n\
pub clamp(value: Int, max: Int = value): Int ->\n\
    max.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("must be a compile-time constant")),
        "diagnostics: {diagnostics:?}"
    );
}
