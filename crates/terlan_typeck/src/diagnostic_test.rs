use super::test_support::*;
use super::*;
use terlan_syntax::parse_module_as_syntax_output;

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
        "std/native/collections/vector.terl",
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
        "std/core/option.terl",
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
pub type Ok[T] = {:ok, value: T}.\n\
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
pub type User = {:user, id: Int, name: Binary}.\n\
pub make(id: Int): Dynamic ->\n\
    User(id) with Wrapped { id = id }.\n\
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
pub type Ok[T] = {:ok, value: T}.\n\
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
pub type Props = #{name := Binary}.\n\
pub make(name: Binary): Props ->\n\
    Props(#{name = name}).\n\
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
pub type Props = #{name := Binary}.\n\
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

/// Verifies legacy Erlang-shaped conversion helpers are not implicit.
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

/// Verifies legacy Erlang-shaped predicate helpers are not implicit.
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
pub type None = :none | :empty.\n\
pub unwrap(input: Dynamic): Dynamic ->\n\
    case input {\n\
        None -> :ok\n\
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

#[test]
fn syntax_output_union_aliases_do_not_generate_constructor_calls_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module alias_union_calls.\n\
pub type None = :none | :empty.\n\
pub none(): Dynamic ->\n\
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
fn syntax_output_remote_union_alias_constructor_calls_are_rejected_by_parser_on_formal_path() {
    let error = parse_module_as_syntax_output(
        "\
module remote_alias_union_calls.\n\
pub none(): Dynamic ->\n\
    options.None().\n\
",
    )
    .expect_err("uppercase dotted remote union alias calls are not source syntax");
    assert!(
        format!("{:?}", error).contains("expected lower-case remote function name"),
        "error: {:?}",
        error
    );
}

#[test]
fn syntax_output_remote_alias_constructor_calls_are_rejected_by_parser_on_formal_path() {
    let error = parse_module_as_syntax_output(
        "\
module result_consumer.\n\
pub make(value: Int): Dynamic ->\n\
    result.Ok(value).\n\
",
    )
    .expect_err("uppercase dotted remote alias constructor calls are not source syntax");
    assert!(
        format!("{:?}", error).contains("expected lower-case remote function name"),
        "error: {:?}",
        error
    );
}

#[test]
fn syntax_output_reports_return_mismatch_on_formal_path() {
    let source = "\
module math.\n\
pub bad(X: Int): Binary ->\n\
    X + 1.\n\
";
    let syntax_diagnostics = check_syntax_output(source);

    let syntax_messages = syntax_diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>();
    assert!(syntax_messages
        .iter()
        .any(|message| message.contains("expected Binary found Int")));
}

/// Verifies syntax-output casts stop before backend emission.
///
/// Inputs:
/// - A syntax-output module whose function body uses explicit
///   `value as Int` cast syntax.
///
/// Output:
/// - Test passes when typechecking reports the stable trait-backed
///   conversion diagnostic for the cast.
///
/// Transformation:
/// - Parses through the formal syntax-output path, resolves the module,
///   typechecks the cast node, and confirms the compiler keeps casts as
///   parse-preserved but semantically unsupported until conversion traits
///   are implemented.
#[test]
fn syntax_output_rejects_cast_before_conversion_resolution_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_cast_boundary.\n\
pub cast_int(value: Dynamic): Int ->\n\
    value as Int.\n\
",
    );

    assert!(
            diagnostics.iter().any(|diagnostic| diagnostic
                .message
                .contains("cast from Dynamic to Int requires trait-backed conversion resolution before backend emission")),
            "diagnostics: {:?}",
            diagnostics
        );
}

#[test]
fn syntax_output_checks_macro_expr_arity_mismatch() {
    let diagnostics = check_syntax_output(
        "\
module syntax_macro_arity.
pub macro asserter(X: Int, Y: Int): Ast[Int] ->
    quote X.

pub bad(X: Int): Bool ->
    ?asserter(X).
",
    );

    assert!(
        diagnostics.iter().any(
            |diag| diag.message.contains("wrong arity for macro `asserter`")
                && diag.message.contains("found 1")
        ),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_raw_macro_expr_without_macro_resolution() {
    let diagnostics = check_syntax_output(
        "\
module syntax_raw_macro_expr.\n\
pub query(): Dynamic ->\n\
    sql{select * from users}.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("raw macro expression `sql` requires macro resolution")),
        "diagnostics: {:?}",
        diagnostics
    );
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("Postgres SQL form lowering is not implemented yet")),
        "diagnostics: {:?}",
        diagnostics
    );
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("parsed 0 SQL parameter expression(s)")),
        "diagnostics: {:?}",
        diagnostics
    );
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("bound 0 SQL parameter placeholder(s)")),
        "diagnostics: {:?}",
        diagnostics
    );
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("SQL parameter count consistency satisfied")),
        "diagnostics: {:?}",
        diagnostics
    );
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("inferred SQL cardinality: many_rows")),
        "diagnostics: {:?}",
        diagnostics
    );
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("wrapper result type: Result[List[Dynamic], Error]")),
        "diagnostics: {:?}",
        diagnostics
    );
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("SQL form requires exactly one explicit row type argument, found 0")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_typechecks_typed_sql_interpolation_children_before_gate() {
    let diagnostics = check_syntax_output(
        "\
module syntax_typed_sql_interpolation_expr.\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {select * from users where active = ${True}}.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("`True` is not a built-in boolean literal")),
        "diagnostics: {:?}",
        diagnostics
    );
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("SQL row type `UserRow` is not a visible struct")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_sql_unknown_row_type_before_gate() {
    let diagnostics = check_syntax_output(
        "\
module syntax_typed_sql_unknown_row_type.\n\
pub query(): Dynamic ->\n\
    sql[MissingRow] {select id from users}.\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "SQL row type `MissingRow` is not a visible struct, type alias, or imported type"
            )),
        "diagnostics: {:?}",
        diagnostics
    );
    assert!(
        diagnostics.iter().all(|diagnostic| !diagnostic
            .message
            .contains("Postgres SQL form lowering is not implemented yet")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_accepts_visible_sql_struct_row_type_before_gate() {
    let diagnostics = check_syntax_output(
        "\
module syntax_typed_sql_visible_row_type.\n\
pub struct UserRow {\n\
    id: Int\n\
}.\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {select id from users}.\n\
",
    );

    assert!(
        diagnostics.iter().all(|diagnostic| !diagnostic
            .message
            .contains("SQL row type `UserRow` is not a visible struct")),
        "diagnostics: {:?}",
        diagnostics
    );
    assert!(
        diagnostics.iter().all(|diagnostic| !diagnostic
            .message
            .contains("Postgres SQL form lowering is not implemented yet")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_sql_projection_field_not_on_row_struct() {
    let diagnostics = check_syntax_output(
        "\
module syntax_typed_sql_extra_projection_field.\n\
pub struct UserRow {\n\
    id: Int\n\
}.\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {select id, email from users}.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("SQL selected column `email` is not a field on row type `UserRow`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_returning_sql_projection_field_not_on_row_struct() {
    let diagnostics = check_syntax_output(
        "\
module syntax_typed_sql_returning_extra_projection_field.\n\
pub struct UserRow {\n\
    id: Int\n\
}.\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {insert into users (id) values (${1}) returning id, email}.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("SQL selected column `email` is not a field on row type `UserRow`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_sql_projection_missing_row_struct_field() {
    let diagnostics = check_syntax_output(
        "\
module syntax_typed_sql_missing_projection_field.\n\
pub struct UserRow {\n\
    id: Int,\n\
    name: String\n\
}.\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {select id from users}.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("SQL row type `UserRow` field `name` is not selected by this query")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_uses_sql_wrapper_result_type_for_return_checking() {
    let diagnostics = check_syntax_output(
        "\
module syntax_typed_sql_return_match.\n\
pub struct UserRow {\n\
    id: Int\n\
}.\n\
pub query(): Result[Option[UserRow], Error] ->\n\
    sql[UserRow] {select id from users limit 1}.\n\
",
    );

    assert!(
        diagnostics.iter().all(|diagnostic| !diagnostic
            .message
            .contains("expected Result[Option[UserRow], Error] found")),
        "diagnostics: {:?}",
        diagnostics
    );
    assert!(
        diagnostics.iter().all(|diagnostic| !diagnostic
            .message
            .contains("Postgres SQL form lowering is not implemented yet")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_sql_wrapper_result_return_mismatch() {
    let diagnostics = check_syntax_output(
        "\
module syntax_typed_sql_return_mismatch.\n\
pub struct UserRow {\n\
    id: Int\n\
}.\n\
pub query(): Result[List[UserRow], Error] ->\n\
    sql[UserRow] {select id from users limit 1}.\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("(ok, (some, UserRow) | none)")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_reports_ambiguous_sql_cardinality_before_gate() {
    let diagnostics = check_syntax_output(
        "\
module syntax_typed_sql_ambiguous_cardinality.\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {WITH users AS (SELECT * FROM accounts) SELECT * FROM users}.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("SQL form cardinality is ambiguous")),
        "diagnostics: {:?}",
        diagnostics
    );
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("wrapper result type: ambiguous")),
        "diagnostics: {:?}",
        diagnostics
    );
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("SQL wrapper lowering readiness: blocked")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_reports_empty_sql_form_before_gate() {
    let diagnostics = check_syntax_output(
        "\
module syntax_typed_sql_empty_text.\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {   }.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("SQL form analysis error: SQL form text must not be empty")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn collects_syntax_raw_macro_diagnostics() {
    let module = parse_module_as_syntax_output(
        "\
module syntax_raw_macro_expr_report.\n\
pub query(): Dynamic ->\n\
    sql{select * from users}.\n\
",
    )
    .expect("parse syntax-output module");
    let diagnostics = collect_syntax_raw_macro_diagnostics(&module);

    assert_eq!(diagnostics.len(), 1, "diagnostics: {:?}", diagnostics);
    assert!(
        diagnostics[0]
            .message
            .contains("raw macro expression `sql` requires macro resolution"),
        "diagnostic: {:?}",
        diagnostics[0]
    );
    assert!(
        diagnostics[0]
            .message
            .contains("Postgres SQL form lowering is not implemented yet"),
        "diagnostic: {:?}",
        diagnostics[0]
    );
    assert!(
        diagnostics[0]
            .message
            .contains("parsed 0 SQL parameter expression(s)"),
        "diagnostic: {:?}",
        diagnostics[0]
    );
    assert_ne!(diagnostics[0].span, Span::new(0, 0));
}

#[test]
fn expands_syntax_raw_macros_preserves_module_and_reports_diagnostics() {
    let module = parse_module_as_syntax_output(
        "\
module syntax_raw_macro_expansion.\n\
pub query(): Dynamic ->\n\
    sql{select * from users}.\n\
",
    )
    .expect("parse syntax-output module");

    let (expanded, diagnostics) = expand_syntax_raw_macros(module.clone());

    assert_eq!(
        expanded, module,
        "macro-expansion is currently explicit/no-op"
    );
    assert_eq!(
        diagnostics.len(),
        1,
        "expected one raw macro expansion diagnostic"
    );
    assert!(
        diagnostics[0]
            .message
            .contains("raw macro expression `sql` requires macro resolution"),
        "diagnostic: {:?}",
        diagnostics
    );
    assert!(
        diagnostics[0]
            .message
            .contains("Postgres SQL form lowering is not implemented yet"),
        "diagnostic: {:?}",
        diagnostics
    );
    assert!(
        diagnostics[0]
            .message
            .contains("parsed 0 SQL parameter expression(s)"),
        "diagnostic: {:?}",
        diagnostics
    );
}

#[test]
fn expands_syntax_includes_reports_unknown_struct_and_preserves_module() {
    let module = parse_module_as_syntax_output(
        "\
module syntax_include_expansion_unknown.\n\
pub struct User includes MissingParent {\n\
    id: Int\n\
}.\n",
    )
    .expect("parse syntax-output include expansion fixture");
    let resolved = terlan_hir::resolve_syntax_module_output(&module).module;

    let (expanded, diagnostics) = expand_syntax_includes(module.clone(), &resolved);

    assert_eq!(
        expanded, module,
        "invalid include expansion must preserve the original module"
    );
    assert_eq!(
        diagnostics.len(),
        1,
        "expected one include-expansion diagnostic"
    );
    assert!(
        diagnostics[0]
            .message
            .contains("unknown included struct `MissingParent`")
            && diagnostics[0]
                .message
                .contains("declaration of struct `User`"),
        "diagnostic: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_local_opaque_constructor_patterns_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module syntax_opaque_patterns.\n\
pub opaque type UserId = Int.\n\
pub unwrap(input: UserId): Int ->\n\
    case input {\n\
        UserId(value) -> value\n\
    }.\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message == "unknown constructor pattern UserId"),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_remote_opaque_constructor_calls_are_rejected_by_parser_on_formal_path() {
    let error = parse_module_as_syntax_output(
        "\
module syntax_remote_opaque_calls.\n\
pub make(value: Int): users.UserId ->\n\
    users.UserId(value).\n\
",
    )
    .expect_err("uppercase dotted remote opaque constructor calls are not source syntax");

    assert!(
        format!("{:?}", error).contains("expected lower-case remote function name"),
        "error: {:?}",
        error
    );
}

#[test]
fn syntax_output_collects_kind_diagnostics_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module hkt_bad.\n\
\n\
pub trait Functor[F[_]] {\n\
    map[A, B](value: F[A], f: (A) -> B): F[B].\n\
}.\n\
\n\
pub bad(value: Functor[Int]): Int ->\n\
    1.\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("kind mismatch")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_collects_binary_hkt_kind_diagnostics_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module hkt_binary_bad.\n\
\n\
pub type Option[T] = Atom[\"none\"] | {Atom[\"some\"], value: T}.\n\
\n\
pub trait BiFunctor[F[_, _]] {\n\
    bimap[A, B, C, D](value: F[A, B], left: (A) -> C, right: (B) -> D): F[C, D].\n\
}.\n\
\n\
pub bad(value: BiFunctor[Option]): Int ->\n\
    1.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("BiFunctor expects type argument 1 of kind Type -> Type -> Type")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_accepts_matching_hkt_constructor_argument_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module hkt_good.\n\
\n\
pub type Option[T] = Atom[\"none\"] | {Atom[\"some\"], value: T}.\n\
\n\
pub trait Functor[F[_]] {\n\
    map[A, B](value: F[A], f: (A) -> B): F[B].\n\
}.\n\
\n\
pub good(value: Functor[Option]): Int ->\n\
    1.\n\
",
    );

    assert!(
        !diagnostics
            .iter()
            .any(|diag| diag.message.contains("kind mismatch")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies HKT slot covariance accepts a covariant constructor.
///
/// Inputs:
/// - A covariant alias `Box[+T]`.
/// - A trait requiring a unary covariant constructor `Producer[F[+_]]`.
///
/// Output:
/// - Test passes when `Producer[Box]` produces no kind or variance diagnostic.
///
/// Transformation:
/// - Exercises source-level HKT slot variance metadata on trait applications.
#[test]
fn syntax_output_accepts_covariant_hkt_constructor_argument_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module hkt_covariant_good.\n\
\n\
pub opaque type Box[+T] = {value: T}.\n\
\n\
pub trait Producer[F[+_]] {\n\
    produce[A](value: F[A]): F[A].\n\
}.\n\
\n\
pub good(value: Producer[Box]): Int ->\n\
    1.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies HKT slot covariance rejects an invariant constructor.
///
/// Inputs:
/// - An invariant alias `Cell[T]`.
/// - A trait requiring a unary covariant constructor `Producer[F[+_]]`.
///
/// Output:
/// - Test passes when `Producer[Cell]` reports a covariance mismatch.
///
/// Transformation:
/// - Makes `F[+_]` semantically meaningful instead of only a parsed marker.
#[test]
fn syntax_output_rejects_invariant_hkt_constructor_for_covariant_slot_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module hkt_covariant_bad.\n\
\n\
pub opaque type Cell[T] = {value: T}.\n\
\n\
pub trait Producer[F[+_]] {\n\
    produce[A](value: F[A]): F[A].\n\
}.\n\
\n\
pub bad(value: Producer[Cell]): Int ->\n\
    1.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("Producer expects type argument 1 slot 1 to be covariant")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies HKT slot contravariance accepts a contravariant constructor.
///
/// Inputs:
/// - A contravariant alias `Sink[-T]`.
/// - A trait requiring a unary contravariant constructor `Consumer[F[-_]]`.
///
/// Output:
/// - Test passes when `Consumer[Sink]` produces no kind or variance diagnostic.
///
/// Transformation:
/// - Exercises negative HKT slot variance on source trait applications.
#[test]
fn syntax_output_accepts_contravariant_hkt_constructor_argument_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module hkt_contravariant_good.\n\
\n\
pub opaque type Sink[-T] = {value: T}.\n\
\n\
pub trait Consumer[F[-_]] {\n\
    consume[A](value: F[A]): Unit.\n\
}.\n\
\n\
pub good(value: Consumer[Sink]): Int ->\n\
    1.\n\
",
    );

    assert!(
        diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies HKT slot contravariance rejects a covariant constructor.
///
/// Inputs:
/// - A covariant alias `Box[+T]`.
/// - A trait requiring a unary contravariant constructor `Consumer[F[-_]]`.
///
/// Output:
/// - Test passes when `Consumer[Box]` reports a contravariance mismatch.
///
/// Transformation:
/// - Prevents `F[-_]` from being parsed as decoration without semantic force.
#[test]
fn syntax_output_rejects_covariant_hkt_constructor_for_contravariant_slot_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module hkt_contravariant_bad.\n\
\n\
pub opaque type Box[+T] = {value: T}.\n\
\n\
pub trait Consumer[F[-_]] {\n\
    consume[A](value: F[A]): Unit.\n\
}.\n\
\n\
pub bad(value: Consumer[Box]): Int ->\n\
    1.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("Consumer expects type argument 1 slot 1 to be contravariant")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_hkt_parameter_application_arity_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module hkt_param_arity_bad.\n\
\n\
pub trait Functor[F[_]] {\n\
    bad[A, B](value: F[A, B]): Int.\n\
}.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("type constructor `F` expects 1 type argument(s), found 2")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_concrete_type_constructor_application_arity_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module hkt_concrete_arity_bad.\n\
\n\
pub type Option[T] = Atom[\"none\"] | {Atom[\"some\"], value: T}.\n\
\n\
pub bad(value: Option[Int, String]): Int ->\n\
    1.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("type constructor `Option` expects 1 type argument(s), found 2")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_trait_application_arity_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module hkt_trait_arity_bad.\n\
\n\
pub type Option[T] = Atom[\"none\"] | {Atom[\"some\"], value: T}.\n\
\n\
pub trait Functor[F[_]] {\n\
    map[A, B](value: F[A], f: (A) -> B): F[B].\n\
}.\n\
\n\
pub bad(value: Functor[Option, Option]): Int ->\n\
    1.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diag| diag
            .message
            .contains("type constructor `Functor` expects 1 type argument(s), found 2")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn syntax_output_rejects_public_constructor_returning_private_type() {
    let diagnostics = check_syntax_output(
        "\
module public_constructor_private_return.\n\
\n\
struct Secret {\n\
    value: Int\n\
}.\n\
\n\
pub constructor Secret {\n\
    (value: Int): Secret -> value\n\
}.\n",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("public constructor Secret exposes private return type Secret")),
        "diagnostics: {:?}",
        diagnostics
    );
}

/// Verifies uppercase boolean spellings are not built-in values.
///
/// Inputs:
/// - A syntax-output module returning undeclared `True` and `False`.
///
/// Output:
/// - Diagnostics explaining that uppercase spellings must be declared or
///   replaced with lowercase literals.
///
/// Transformation:
/// - Runs the formal syntax-output typechecker and rejects unresolved
///   constructor-style boolean names instead of letting them widen to
///   `Dynamic`.
#[test]
fn syntax_output_rejects_undeclared_uppercase_boolean_spellings() {
    let diagnostics = check_syntax_output(
        "\
module uppercase_boolean_spellings.\n\
\n\
pub yes(): Bool ->\n\
    True.\n\
\n\
pub no(): Bool ->\n\
    False.\n\
",
    );

    assert!(
            diagnostics.iter().any(|diag| diag.message
                == "`True` is not a built-in boolean literal; use lowercase `true` or declare `True` explicitly"),
            "diagnostics: {:?}",
            diagnostics
        );
    assert!(
            diagnostics.iter().any(|diag| diag.message
                == "`False` is not a built-in boolean literal; use lowercase `false` or declare `False` explicitly"),
            "diagnostics: {:?}",
            diagnostics
        );
}

/// Verifies lowercase `unit` is not the built-in unit value.
///
/// Inputs:
/// - A syntax-output module returning lowercase `unit` from a `Unit`-typed
///   function.
///
/// Output:
/// - A typecheck diagnostic for the return-type mismatch.
///
/// Transformation:
/// - Treats lowercase `unit` as an ordinary atom-like source expression
///   rather than as the compiler-owned `Unit` singleton.
#[test]
fn syntax_output_rejects_lowercase_unit_as_builtin_value() {
    let diagnostics = check_syntax_output(
        "\
module lowercase_unit_value.\n\
\n\
pub value(): Unit ->\n\
    unit.\n\
",
    );

    assert!(
            diagnostics
                .iter()
                .any(|diag| diag.message
                    == "`unit` is not a built-in unit value; use uppercase `Unit`"),
            "diagnostics: {:?}",
            diagnostics
        );
}
