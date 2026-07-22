use super::test_support::*;
use super::*;
use crate::terlan_syntax::parse_module_as_syntax_output;

#[test]
fn syntax_output_accepts_constant_default_parameter_values() {
    let diagnostics = check_syntax_output(
        "\
module parameter_default_ok.\n\
\n\
pub struct Label {\n\
    value: String\n\
}.\n\
\n\
pub greet(name: String, excited: Bool = false): String ->\n\
    name.\n\
\n\
pub (label: Label) pad(width: Int = 2): Label ->\n\
    label.\n\
\n\
pub trait ShowLabel[T] {\n\
    label(value: T, separator: String = \":\"): String.\n\
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
fn syntax_output_rejects_dynamic_default_parameter_values() {
    let diagnostics = check_syntax_output(
        "\
module parameter_default_dynamic.\n\
\n\
pub make(value: Int = fallback()): Int ->\n\
    value.\n\
\n\
pub fallback(): Int ->\n\
    1.\n\
",
    );
    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("default value for parameter `value` must be a compile-time constant")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies receiver calls cannot be used as parameter defaults.
///
/// Inputs:
/// - A function parameter default that calls `users.len()` and performs
///   arithmetic.
///
/// Output:
/// - Test passes when typechecking reports the default is not a compile-time
///   constant.
///
/// Transformation:
/// - Exercises the default-parameter validator against the binary-search
///   shorthand shape `high: Int = users.len() - 1`, which depends on a runtime
///   parameter and must remain illegal.
#[test]
fn syntax_output_rejects_receiver_call_default_parameter_values() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module parameter_default_receiver_call.\n\
\n\
import std.native.collections.Vector.\n\
\n\
pub search(users: Vector[Int], high: Int = users.len() - 1): Int ->\n\
    high.\n\
",
        "std/native/collections/Vector.terl",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("default value for parameter `high` must be a compile-time constant")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_mismatched_default_parameter_values() {
    let diagnostics = check_syntax_output(
        "\
module parameter_default_mismatch.\n\
\n\
pub add(step: Int = \"slow\"): Int ->\n\
    step.\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("default value for parameter `step`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_accepts_omitted_local_function_default_argument() {
    let diagnostics = check_syntax_output(
        "\
module omitted_function_default_ok.\n\
\n\
pub greet(name: String, excited: Bool = false): String ->\n\
    name.\n\
\n\
pub run(): String ->\n\
    greet(\"Ada\").\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_omitted_required_local_function_argument() {
    let diagnostics = check_syntax_output(
        "\
module omitted_required_function_arg.\n\
\n\
pub create_user(id: Int, name: String = \"Ada\"): Int ->\n\
    id.\n\
\n\
pub run(): Int ->\n\
    create_user(name = \"Bob\").\n\
",
    );
    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("missing required argument `id` for call to `create_user`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_checks_constructor_default_parameter_values() {
    let diagnostics = check_syntax_output(
        "\
module constructor_default_ok.\n\
\n\
pub type User = {name: String, active: Bool}.\n\
\n\
pub constructor User {\n\
    (name: String, active: Bool = true): User ->\n\
        User(name, active)\n\
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
fn syntax_output_accepts_local_named_call_arguments() {
    let diagnostics = check_syntax_output(
        "\
module named_call_ok.\n\
\n\
pub create_user(id: Int, name: String, active: Bool = true): Int ->\n\
    id.\n\
\n\
pub run(): Int ->\n\
    create_user(1, active = false, name = \"Ada\").\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_unknown_local_named_call_argument() {
    let diagnostics = check_syntax_output(
        "\
module named_call_unknown.\n\
\n\
pub create_user(id: Int, name: String): Int ->\n\
    id.\n\
\n\
pub run(): Int ->\n\
    create_user(1, label = \"Ada\").\n\
",
    );
    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("unknown named argument `label` for call to `create_user`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_local_named_call_argument_supplied_positionally() {
    let diagnostics = check_syntax_output(
        "\
module named_call_duplicate.\n\
\n\
pub create_user(id: Int, name: String): Int ->\n\
    id.\n\
\n\
pub run(): Int ->\n\
    create_user(1, id = 2).\n\
",
    );
    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("argument `id` for call to `create_user` is already supplied positionally")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_duplicate_local_named_call_argument() {
    let diagnostics = check_syntax_output(
        "\
module named_call_duplicate_name.\n\
\n\
pub create_user(id: Int, name: String): Int ->\n\
    id.\n\
\n\
pub run(): Int ->\n\
    create_user(id = 1, id = 2).\n\
",
    );
    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("duplicate named argument `id` for call to `create_user`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_accepts_local_named_constructor_arguments() {
    let diagnostics = check_syntax_output(
        "\
module named_constructor_ok.\n\
\n\
pub type User = {id: Int, name: String, active: Bool}.\n\
\n\
pub constructor User {\n\
    (id: Int, name: String, active: Bool): User ->\n\
        User(id, name, active)\n\
}.\n\
\n\
pub run(): User ->\n\
    User(1, active = false, name = \"Ada\").\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_accepts_omitted_constructor_default_arguments() {
    let diagnostics = check_syntax_output(
        "\
module constructor_default_arg_ok.\n\
\n\
pub type User = {id: Int, name: String, active: Bool}.\n\
\n\
pub constructor User {\n\
    (id: Int, name: String = \"Ada\", active: Bool = true): User ->\n\
        User(id, name, active)\n\
}.\n\
\n\
pub run(): User ->\n\
    User(id = 1, active = false).\n\
",
    );
    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_omitted_required_constructor_argument() {
    let diagnostics = check_syntax_output(
        "\
module constructor_default_arg_missing_required.\n\
\n\
pub type User = {id: Int, name: String, active: Bool}.\n\
\n\
pub constructor User {\n\
    (id: Int, name: String = \"Ada\", active: Bool = true): User ->\n\
        User(id, name, active)\n\
}.\n\
\n\
pub run(): User ->\n\
    User(active = false).\n\
",
    );
    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("missing required argument `id` for constructor `User`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_unknown_local_named_constructor_argument() {
    let diagnostics = check_syntax_output(
        "\
module named_constructor_unknown.\n\
\n\
pub type User = {id: Int, name: String}.\n\
\n\
pub constructor User {\n\
    (id: Int, name: String): User ->\n\
        User(id, name)\n\
}.\n\
\n\
pub run(): User ->\n\
    User(1, label = \"Ada\").\n\
",
    );
    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("unknown named argument `label` for call to `User`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_local_named_constructor_argument_supplied_positionally() {
    let diagnostics = check_syntax_output(
        "\
module named_constructor_positional_duplicate.\n\
\n\
pub type User = {id: Int, name: String}.\n\
\n\
pub constructor User {\n\
    (id: Int, name: String): User ->\n\
        User(id, name)\n\
}.\n\
\n\
pub run(): User ->\n\
    User(1, id = 2).\n\
",
    );
    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("argument `id` for call to `User` is already supplied positionally")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_mismatched_constructor_default_parameter_values() {
    let diagnostics = check_syntax_output(
        "\
module constructor_default_bad.\n\
\n\
pub type User = {name: String, active: Bool}.\n\
\n\
pub constructor User {\n\
    (name: String, active: Bool = \"yes\"): User ->\n\
        User(name, active)\n\
}.\n\
",
    );
    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("default value for parameter `active`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_unknown_constructor_patterns_are_rejected_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module constructor_patterns.\n\
pub unwrap(input: Dynamic): Dynamic ->\n\
    case input {\n\
        Missing -> input\n\
    }.\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor pattern Missing"),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies missing constructor imports are reported at constructor use sites.
///
/// Inputs:
/// - A module using `Option[Int]` and `Some(value)` without importing
///   `std.core.Option.{Option, Some}`.
///
/// Output:
/// - Test passes when typechecking reports the missing `Some` constructor
///   directly.
///
/// Transformation:
/// - Exercises the same stale-module failure shape as an external file that
///   references option constructors without making them visible through an
///   import.
#[test]
fn syntax_output_rejects_option_constructors_without_imports() {
    let diagnostics = check_syntax_output_with_std_interfaces(
        "\
module option_constructors_without_imports.\n\
\n\
pub make(value: Int): Option[Int] ->\n\
    Some(value).\n\
",
        "std/core/Option.terl",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor Some / 1"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_list_cons_expr_rejects_non_list_tail_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module list_cons_expr_tail.\n\
pub prepend(head: Int, tail: Binary): List[Int] ->\n\
    [head | tail].\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("list cons tail")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_unknown_constructor_calls_are_rejected_on_formal_path() {
    let source = "\
module constructor_calls.\n\
pub make(value: Dynamic): Dynamic ->\n\
    Missing(value).\n\
";
    let diagnostics = check_syntax_output(source);
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor Missing / 1"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_remote_constructor_calls_are_rejected_by_parser_on_formal_path() {
    let error = parse_module_as_syntax_output(
        "\
module option_consumer.\n\
pub make(value: Dynamic): Dynamic ->\n\
    option.Some(value).\n\
",
    )
    .expect_err("uppercase dotted remote constructor calls are not source syntax");
    assert!(
        format!("{:?}", error).contains("expected lower-case remote function name"),
        "error: {:?}",
        error
    );
}

#[test]
fn syntax_output_unknown_remote_constructor_calls_are_rejected_by_parser_on_formal_path() {
    let error = parse_module_as_syntax_output(
        "\
module option_consumer.\n\
pub make(value: Dynamic): Dynamic ->\n\
    option.Missing(value).\n\
",
    )
    .expect_err("uppercase dotted remote constructor calls are not source syntax");
    assert!(
        format!("{:?}", error).contains("expected lower-case remote function name"),
        "error: {:?}",
        error
    );
}

#[test]
fn syntax_output_single_shape_alias_constructor_calls_report_arity_mismatch_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module alias_constructor_call_arity.\n\
pub type Ok[T] = {Atom[\"ok\"], value: T}.\n\
pub make(): Dynamic ->\n\
    Ok().\n\
",
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.message == "constructor Ok has arity mismatch: expected 1..1 args, found 0"
        }),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_list_aliases_do_not_generate_constructor_calls_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module list_alias_constructor_calls.\n\
pub type Items[T] = List[T].\n\
pub make(values: List[Int]): Items[Int] ->\n\
    Items(values).\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor Items / 1"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_single_shape_alias_constructor_chains_report_arity_mismatch_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module alias_constructor_chain_arity.\n\
pub type User = {Atom[\"user\"], id: Int, name: Binary}.\n\
pub make(id: Int): Dynamic ->\n\
    User(id) with Wrapped { id: id }.\n\
",
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.message == "constructor User has arity mismatch: expected 2..2 args, found 1"
        }),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_single_shape_alias_constructor_patterns_report_arity_mismatch_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module alias_constructor_pattern_arity.\n\
pub type Ok[T] = {Atom[\"ok\"], value: T}.\n\
pub unwrap(input: Ok[Int]): Int ->\n\
    case input {\n\
        Ok(value, extra) -> value\n\
    }.\n\
",
    );
    assert!(
        diagnostics.iter().any(|diag| {
            diag.message == "constructor Ok has arity mismatch: expected 1..1 args, found 2"
        }),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_list_aliases_do_not_generate_constructor_patterns_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module list_alias_constructor_patterns.\n\
pub type Items[T] = List[T].\n\
pub unwrap(input: Items[Int]): List[Int] ->\n\
    case input {\n\
        Items(values) -> values\n\
    }.\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor pattern Items"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_structural_tuple_aliases_do_not_generate_constructor_calls_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module structural_alias_constructor_calls.\n\
pub type Pair = {left: Int, right: Int}.\n\
pub make(): Pair ->\n\
    Pair(1, 2).\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor Pair / 2"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_structural_tuple_aliases_do_not_generate_constructor_patterns_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module structural_alias_constructor_patterns.\n\
pub type Pair = {left: Int, right: Int}.\n\
pub left(input: Pair): Int ->\n\
    case input {\n\
        Pair(left, _right) -> left\n\
    }.\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor pattern Pair"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_map_aliases_do_not_generate_constructor_calls_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module map_alias_constructor_calls.\n\
pub type Props = {name: Binary}.\n\
pub make(name: Binary): Props ->\n\
    Props({name: name}).\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor Props / 1"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_map_aliases_do_not_generate_constructor_patterns_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module map_alias_constructor_patterns.\n\
pub type Props = {name: Binary}.\n\
pub name(input: Props): Binary ->\n\
    case input {\n\
        Props(values) -> values\n\
    }.\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor pattern Props"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_remote_list_alias_constructor_calls_are_rejected_by_parser_on_formal_path() {
    let error = parse_module_as_syntax_output(
        "\
module remote_list_alias_constructor_calls.\n\
pub make(values: List[Int]): items.Items[Int] ->\n\
    items.Items(values).\n\
",
    )
    .expect_err("uppercase dotted remote alias constructor calls are not source syntax");
    assert!(
        format!("{:?}", error).contains("expected lower-case remote function name"),
        "error: {:?}",
        error
    );
}

/// Verifies legacy Vm-shaped conversion helpers are not implicit.
///
/// Inputs:
/// - A source module calling `integer_to_binary(1)` without an import.
///
/// Output:
/// - A diagnostic explaining that the helper is outside the implicit
///   prelude.
///
/// Transformation:
/// - Enforces the implicit-prelude boundary so only target-neutral compiler
///   functions such as `type_of` and `is_type` are implicit.
#[test]
fn syntax_output_rejects_legacy_conversion_helpers_from_implicit_prelude() {
    let diagnostics = check_syntax_output(
        "\
module legacy_conversion_helper_prelude.\n\
pub value(): Dynamic ->\n\
    integer_to_binary(1).\n\
",
    );
    assert!(
            diagnostics.iter().any(|diag| diag.message
                == "`integer_to_binary/1` is not part of the implicit prelude; import or define it explicitly"),
            "diagnostics: {:?}",
            diagnostics
        );
}

/// Verifies legacy Vm-shaped predicate helpers are not implicit.
///
/// Inputs:
/// - A source module calling `is_integer(1)` without an import.
///
/// Output:
/// - A diagnostic explaining that the predicate is outside the implicit
///   prelude.
///
/// Transformation:
/// - Keeps guard and predicate syntax target-neutral by requiring source
///   code to use `is_type(value, Int)` or an explicitly imported helper.
#[test]
fn syntax_output_rejects_legacy_predicate_helpers_from_implicit_prelude() {
    let diagnostics = check_syntax_output(
        "\
module legacy_predicate_helper_prelude.\n\
pub value(): Dynamic ->\n\
    is_integer(1).\n\
",
    );
    assert!(
            diagnostics.iter().any(|diag| diag.message
                == "`is_integer/1` is not part of the implicit prelude; import or define it explicitly"),
            "diagnostics: {:?}",
            diagnostics
        );
}

#[test]
fn syntax_output_literal_alias_constructor_calls_are_rejected_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module alias_literal_calls.\n\
pub type None = Atom[\"none\"].\n\
pub none(): None ->\n\
    None().\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor None / 0"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_remote_literal_alias_constructor_calls_are_rejected_by_parser_on_formal_path() {
    let error = parse_module_as_syntax_output(
        "\
module remote_alias_literal_calls.\n\
pub none(): Dynamic ->\n\
    literals.None().\n\
",
    )
    .expect_err("uppercase dotted remote literal alias calls are not source syntax");
    assert!(
        format!("{:?}", error).contains("expected lower-case remote function name"),
        "error: {:?}",
        error
    );
}

#[test]
fn syntax_output_union_aliases_do_not_generate_constructor_patterns_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module alias_union_patterns.\n\
pub type None = Atom[\"none\"] | Atom[\"empty\"].\n\
pub unwrap(input: Dynamic): Dynamic ->\n\
    case input {\n\
        None -> Atom[\"ok\"]\n\
    }.\n\
",
    );
    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor pattern None"),
        "diagnostics: {:?}",
        diagnostics
    );
}
