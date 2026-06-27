use super::test_support::*;

/// Verifies lowercase booleans are the only built-in boolean literals.
///
/// Inputs:
/// - A syntax-output module returning lowercase `true` and `false`.
///
/// Output:
/// - Empty typecheck diagnostics.
///
/// Transformation:
/// - Runs the formal syntax-output typechecker and confirms lowercase
///   boolean literals infer as `Bool` without imports or declarations.
#[test]
fn syntax_output_accepts_lowercase_canonical_boolean_literals() {
    let diagnostics = check_syntax_output(
        "\
module canonical_boolean_literals.\n\
\n\
pub yes(): Bool ->\n\
    true.\n\
\n\
pub no(): Bool ->\n\
    false.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies declared uppercase aliases remain legal constructor-style names.
///
/// Inputs:
/// - A syntax-output module declaring `True` as a singleton atom alias.
///
/// Output:
/// - Empty typecheck diagnostics.
///
/// Transformation:
/// - Resolves the declared alias before applying the canonical-boolean
///   diagnostic, preserving the rule that uppercase names are valid only
///   when source code declares them explicitly.
#[test]
fn syntax_output_accepts_declared_uppercase_boolean_alias_value() {
    let diagnostics = check_syntax_output(
        "\
module declared_uppercase_boolean_alias.\n\
\n\
pub type True = Atom[\"true\"].\n\
\n\
pub value(): True ->\n\
    True.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies `Unit` is the built-in unit value.
///
/// Inputs:
/// - A syntax-output module returning `Unit` from a `Unit`-typed function.
///
/// Output:
/// - Empty typecheck diagnostics.
///
/// Transformation:
/// - Resolves the exact uppercase source spelling `Unit` as the built-in
///   singleton value with type `Unit`.
#[test]
fn syntax_output_accepts_uppercase_unit_value() {
    let diagnostics = check_syntax_output(
        "\
module canonical_unit_value.\n\
\n\
pub value(): Unit ->\n\
    Unit.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}
