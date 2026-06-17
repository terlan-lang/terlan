use super::test_support::*;
use super::*;
use terlan_syntax::parse_module_as_syntax_output;

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
    let diagnostics = check_syntax_output(
        "\
module constructor_calls.\n\
pub make(value: Dynamic): Dynamic ->\n\
    Missing(value).\n\
",
    );
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
/// - Enforces the 0.0.3 prelude boundary so only target-neutral compiler
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
}

#[test]
fn expands_syntax_derives_reports_unknown_struct_and_preserves_module() {
    let module = parse_module_as_syntax_output(
        "\
module syntax_derive_expansion_unknown.\n\
pub struct User derives MissingParent {\n\
    id: Int\n\
}.\n",
    )
    .expect("parse syntax-output derive expansion fixture");
    let resolved = terlan_hir::resolve_syntax_module_output(&module).module;

    let (expanded, diagnostics) = expand_syntax_derives(module.clone(), &resolved);

    assert_eq!(
        expanded, module,
        "invalid derive expansion must preserve the original module"
    );
    assert_eq!(
        diagnostics.len(),
        1,
        "expected one derive-expansion diagnostic"
    );
    assert!(
        diagnostics[0]
            .message
            .contains("unknown derived struct `MissingParent`")
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
    let module = parse_module_as_syntax_output(
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
    )
    .expect("parse syntax output kind diagnostic fixture");

    let diagnostics = collect_syntax_kind_diagnostics(&module);

    assert!(
        diagnostics
            .iter()
            .any(|diag| diag.message.contains("kind mismatch")),
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
