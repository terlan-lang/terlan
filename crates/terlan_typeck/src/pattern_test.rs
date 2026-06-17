use super::test_support::*;

/// Verifies generic comparator callbacks keep their declared return type.
///
/// Inputs:
/// - A local `Option[T]` alias and a `Comparison` result alias.
/// - A generic `compare_option` function that accepts `(T, T) ->
///   Comparison` and calls it from a nested `case` branch.
///
/// Output:
/// - Test passes when the syntax-output typechecker accepts the callback
///   result as `Comparison` rather than inferring the contained `T`.
///
/// Transformation:
/// - Parses the formal syntax-output path, infers the higher-order
///   callback invocation inside pattern-refined branches, and validates the
///   enclosing function return annotation.
#[test]
fn syntax_output_generic_comparator_callback_preserves_declared_return_type() {
    let diagnostics = check_syntax_output(
            "\
module comparator_callback_return.\n\
pub type Comparison = :lt | :eq | :gt.\n\
pub type Option[T] = :none | {:some, T}.\n\
pub compare_option(compare: (T, T) -> Comparison, left: Option[T], right: Option[T]): Comparison ->\n\
    case left {\n\
        :none ->\n\
            case right {\n\
                :none -> :eq;\n\
                {:some, _} -> :lt\n\
            };\n\
\n\
        {:some, left_value} ->\n\
            case right {\n\
                :none -> :gt;\n\
                {:some, right_value} -> compare(left_value, right_value)\n\
            }\n\
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
fn syntax_output_list_cons_patterns_are_valid_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module list_cons_patterns.\n\
pub prepend(head: Int, tail: List[Int]): List[Int] ->\n\
    [head | tail].\n\
\n\
pub head(input: List[Int]): Int ->\n\
    case input {\n\
        [head | _tail] -> head;\n\
        [] -> 0\n\
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
fn syntax_output_binds_case_constructor_patterns_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_case_patterns.\n\
pub type Some = {:some, Int}.\n\
pub unwrap(input: Some): Int ->\n\
    case input {\n\
        Some(value) -> value\n\
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
fn syntax_output_refines_case_guards_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_case_guards.\n\
pub to_int(value: Dynamic): Int ->\n\
    case value {\n\
        x when is_type(x, Int) -> x\n\
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
fn syntax_output_refines_function_guards_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_function_guards.\n\
pub to_int(value) when is_type(value, Int) -> value.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}
