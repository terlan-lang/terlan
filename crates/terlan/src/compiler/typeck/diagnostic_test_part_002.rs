
#[test]
fn syntax_output_union_aliases_do_not_generate_constructor_calls_on_formal_path() {
    let diagnostics = check_syntax_output(
        "\
module alias_union_calls.\n\
pub type None = Atom[\"none\"] | Atom[\"empty\"].\n\
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
            .contains("inferred SQL query kind: select")),
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
    sql[UserRow] {CREATE TABLE users (id INT)}.\n\
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
fn sql_macro_validation_reports_stable_malformed_sql_diagnostic() {
    let diagnostics = check_syntax_output(
        "\
module syntax_typed_sql_malformed.\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {SELECT id FROM users WHERE (}.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("SQL form contains malformed PostgreSQL syntax")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn sql_macro_validation_rejects_vm_owned_transaction_control() {
    let diagnostics = check_syntax_output(
        "\
module syntax_typed_sql_transaction_control.\n\
pub begin_transaction(): Dynamic ->\n\
    sql[UnitRow] {BEGIN}.\n\
",
    );

    assert!(
        diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "SQL transaction control is VM-owned; use the typed database transaction API"
            )),
        "diagnostics: {:?}",
        diagnostics
    );
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("inferred SQL transaction requirement: vm_managed_control")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn sql_macro_validation_reports_duplicate_projection_name() {
    let diagnostics = check_syntax_output(
        "\
module syntax_typed_sql_duplicate_projection.\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {SELECT id AS value, name AS value FROM users}.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("SQL projection contains duplicate output name `value`")),
        "diagnostics: {:?}",
        diagnostics
    );
}

#[test]
fn sql_macro_validation_rejects_explicit_postgres_placeholders() {
    let diagnostics = check_syntax_output(
        "\
module syntax_typed_sql_explicit_placeholder.\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {SELECT id FROM users WHERE id = $1}.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("explicit PostgreSQL placeholders are not allowed; use `${expression}`")),
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
fn syntax_output_accepts_local_unguarded_shape_synonym_after_expansion() {
    let diagnostics = check_syntax_output(
        "\
module shape_synonym_expanded.\n\
\n\
shape UserAsset(id) =\n\
    {Atom[\"user_asset\"], id}.\n\
\n\
pub asset_id(value: Dynamic): Int ->\n\
    case value {\n\
        UserAsset(id) -> id;\n\
        _ -> 0\n\
    }.\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn syntax_output_typechecks_composed_shape_guards() {
    let diagnostics = check_syntax_output(
        "\
module guarded_shape_synonym.\n\
\n\
shape Success(body) =\n\
    {status, body} where status >= 200 and status < 300.\n\
\n\
pub body(value: Dynamic): Int ->\n\
    case value {\n\
        Success(found) where found > 0 -> found;\n\
        _ -> 0\n\
    }.\n\
",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn syntax_output_rejects_non_bool_shape_guard_after_expansion() {
    let diagnostics = check_syntax_output(
        "\
module non_bool_shape_guard.\n\
\n\
shape Invalid(value) =\n\
    value where 1.\n\
\n\
pub read(value: Dynamic): Int ->\n\
    case value {\n\
        Invalid(found) -> 1;\n\
        _ -> 0\n\
    }.\n\
",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("case guard expected Bool found 1")),
        "diagnostics: {diagnostics:?}"
    );
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
    let resolved = crate::terlan_hir::resolve_syntax_module_output(&module).module;

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
