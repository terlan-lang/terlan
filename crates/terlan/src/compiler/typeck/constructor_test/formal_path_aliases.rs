use super::*;

#[test]
fn syntax_output_raw_atom_patterns_do_not_require_constructor_declarations_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module raw_atom_patterns.\n\
pub unwrap(input: Dynamic): Dynamic ->\n\
    case input {\n\
        Atom[\"none\"] -> Atom[\"none\"];\n\
        Atom[\"empty\"] -> Atom[\"empty\"]\n\
    }.\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_declared_constructor_calls_are_valid_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module constructor_calls.\n\
pub constructor Some {\n\
    (value: Dynamic): Dynamic -> {Atom[\"some\"], value}\n\
}.\n\
pub make(value: Dynamic): Dynamic ->\n\
    Some(value).\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_single_shape_alias_constructor_calls_are_valid_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module alias_constructor_calls.\n\
pub type Ok[T] = {Atom[\"ok\"], value: T}.\n\
pub make(value: Int): Dynamic ->\n\
    Ok(value).\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_single_shape_alias_constructor_patterns_are_valid_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module alias_constructor_patterns.\n\
pub type Ok[T] = {Atom[\"ok\"], value: T}.\n\
pub unwrap(input: Ok[Int]): Int ->\n\
    case input {\n\
        Ok(value) -> value\n\
    }.\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_literal_alias_constructor_patterns_are_valid_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module alias_literal_patterns.\n\
pub type None = Atom[\"none\"].\n\
pub unwrap(input: None): Dynamic ->\n\
    case input {\n\
        None -> Atom[\"ok\"]\n\
    }.\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies atom-literal aliases compare against their literal runtime
/// value.
///
/// Inputs:
/// - A syntax-output module defining `Unit = Atom["unit"]`.
/// - A public function returning `Unit`.
/// - A comparison between the function result and `Atom["unit"]`.
///
/// Output:
/// - Test passes when syntax-output typechecking accepts the comparison
///   without diagnostics.
///
/// Transformation:
/// - Runs the formal syntax-output typechecker and confirms binary
///   comparison inference expands transparent aliases before rejecting
///   otherwise distinct operand spellings.
#[test]
fn syntax_output_literal_aliases_compare_with_literal_values_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module alias_literal_comparisons.\n\
pub type Unit = Atom[\"unit\"].\n\
pub value(): Unit ->\n\
    Unit.\n\
pub matches(): Bool ->\n\
    value() == Unit.\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies atom aliases compare against canonical atom literal values.
///
/// Inputs:
/// - A singleton alias shorthand `Ready.`.
/// - A comparison between the alias value and the canonical literal.
///
/// Output:
/// - Empty typecheck diagnostics.
///
/// Transformation:
/// - Resolves the shorthand alias value to its singleton atom representation
///   and unifies it with the explicit `Atom["ready"]` expression form.
#[test]
fn syntax_output_atom_aliases_compare_with_canonical_atom_literal_values() {
    let diagnostics = check_syntax_output(
        "\
module atom_alias_literal_comparisons.\n\
pub type Ready.\n\
pub value(): Ready ->\n\
    Ready.\n\
pub matches(): Bool ->\n\
    value() == Atom[\"ready\"].\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies bodyless atom aliases can be used as constructor patterns.
///
/// Inputs:
/// - A singleton alias shorthand `Ready.`.
/// - A case expression matching a value of that alias with `Ready`.
///
/// Output:
/// - Empty typecheck diagnostics.
///
/// Transformation:
/// - Resolves the bodyless alias to the same singleton atom constructor used by
///   explicit `Atom["ready"]` aliases, then validates constructor-pattern
///   matching on the formal syntax-output path.
#[test]
fn syntax_output_bodyless_atom_alias_constructor_patterns_match_values() {
    let diagnostics = check_syntax_output(
        "\
module atom_alias_pattern_match.\n\
pub type Ready.\n\
pub value(): Ready ->\n\
    Ready.\n\
pub matches(): Bool ->\n\
    case value() {\n\
        Ready -> true\n\
    }.\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies canonical atom literals do not become unit synonyms.
///
/// Inputs:
/// - A function returning `Atom["unit"]` as an `Atom`.
///
/// Output:
/// - Empty typecheck diagnostics.
///
/// Transformation:
/// - Confirms only bare lowercase `unit` is rejected; the explicit atom
///   literal spelling remains an ordinary symbolic atom value.
#[test]
fn syntax_output_accepts_canonical_unit_named_atom_literal() {
    let diagnostics = check_syntax_output(
        "\
module canonical_unit_named_atom_literal.\n\
pub value(): Atom ->\n\
    Atom[\"unit\"].\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_literal_alias_values_are_valid_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module alias_literal_values.\n\
pub type None = Atom[\"none\"].\n\
pub none(): None ->\n\
    None.\n\
",
    );
    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}

#[test]
fn syntax_output_quoted_atom_alias_constructor_patterns_are_valid_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module alias_quoted_literal_patterns.\n\
pub type ModuleAtom = Atom[\"Elixir.Module\"].\n\
pub unwrap(input: ModuleAtom): Dynamic ->\n\
    case input {\n\
        ModuleAtom -> Atom[\"ok\"]\n\
    }.\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_supports_constructor_chain_now() {
    let diagnostics = check_syntax_output(
        "\
module syntax_constructor_chain_expr.\n\
pub type User = Dynamic.\n\
pub constructor User {\n\
    (id: Int, name: Binary): Dynamic ->\n\
        id\n\
}.\n\
pub demo(id: Int, name: Binary): Dynamic ->\n\
    User(id, name) with Admin { id: id, name: name }.\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {:?}", diagnostics);
}
