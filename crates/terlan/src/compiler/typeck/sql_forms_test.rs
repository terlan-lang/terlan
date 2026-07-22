use super::sql_forms::{
    analyze_sql_form, bind_sql_parameters, build_sql_wrapper_plan,
    classification::{statement_cardinality, statement_transaction_requirement},
    parse_single_postgres_statement,
    projection::statement_projection_fields,
    SqlCardinality, SqlFormAnalysisError, SqlParameterBinding, SqlParameterBindingError,
    SqlProjectionError, SqlQueryKind, SqlSyntaxValidationError, SqlTransactionRequirement,
    SqlWrapperPlanError,
};
use crate::terlan_syntax::{
    parse_module_as_syntax_output, SyntaxDeclarationPayload, SyntaxExprOutput,
};

#[test]
fn infers_select_limit_one_as_optional_one() {
    assert_eq!(
        infer_sql_cardinality(
            "\
            SELECT id, name
            FROM users
            WHERE id = ${id}
            LIMIT 1
            ",
        ),
        SqlCardinality::OptionalOne
    );
}

#[test]
fn infers_select_without_limit_one_as_many_rows() {
    assert_eq!(
        infer_sql_cardinality("SELECT id, name FROM users"),
        SqlCardinality::ManyRows
    );
}

#[test]
fn infers_mutating_statement_without_returning_as_affected_rows() {
    assert_eq!(
        infer_sql_cardinality("UPDATE users SET active = false WHERE last_seen < ${cutoff}"),
        SqlCardinality::AffectedRows
    );
}

#[test]
fn infers_mutating_statement_with_returning_as_many_rows() {
    assert_eq!(
        infer_sql_cardinality("DELETE FROM sessions WHERE expires_at < ${now} RETURNING id"),
        SqlCardinality::ManyRows
    );
}

#[test]
fn ignores_limit_one_inside_sql_string_literals_and_comments() {
    assert_eq!(
        infer_sql_cardinality(
            "\
            -- LIMIT 1
            SELECT id
            FROM logs
            WHERE message = 'LIMIT 1'
            ",
        ),
        SqlCardinality::ManyRows
    );
}

#[test]
fn infers_cte_select_from_postgres_ast() {
    assert_eq!(
        infer_sql_cardinality("WITH users AS (SELECT * FROM accounts) SELECT * FROM users"),
        SqlCardinality::ManyRows
    );
}

#[test]
fn infers_fetch_first_one_as_optional_one() {
    assert_eq!(
        infer_sql_cardinality("SELECT id FROM users FETCH FIRST 1 ROW ONLY"),
        SqlCardinality::OptionalOne
    );
}

#[test]
fn keeps_dynamic_limit_and_fetch_with_ties_as_many_rows() {
    assert_eq!(
        infer_sql_cardinality("SELECT id FROM users LIMIT ${limit}"),
        SqlCardinality::ManyRows
    );
    assert_eq!(
        infer_sql_cardinality("SELECT id FROM users FETCH FIRST 1 ROW WITH TIES"),
        SqlCardinality::ManyRows
    );
}

#[test]
fn classifies_postgres_statement_kinds_from_ast() {
    let cases = [
        (
            "WITH found AS (SELECT 1) SELECT * FROM found",
            SqlQueryKind::Select,
        ),
        ("INSERT INTO users (id) VALUES (1)", SqlQueryKind::Insert),
        ("UPDATE users SET active = false", SqlQueryKind::Update),
        ("DELETE FROM users", SqlQueryKind::Delete),
        ("CREATE TABLE users (id INT)", SqlQueryKind::Ddl),
        ("BEGIN", SqlQueryKind::Transaction),
        ("SAVEPOINT before_write", SqlQueryKind::Transaction),
        (
            "SET TRANSACTION ISOLATION LEVEL SERIALIZABLE",
            SqlQueryKind::Transaction,
        ),
        ("EXPLAIN SELECT 1", SqlQueryKind::Other),
    ];

    for (sql, expected) in cases {
        let source =
            format!("module sql_kind.\npub query(): Dynamic ->\n    sql[Row] {{{sql}}}.\n");
        let body = first_function_body(&source);
        let analysis = analyze_sql_form(&body)
            .expect("analyze SQL kind")
            .expect("typed SQL analysis");
        assert_eq!(analysis.query_kind, expected, "SQL: {sql}");
        assert_eq!(
            analysis.query_kind.as_diagnostic_label(),
            expected.as_diagnostic_label()
        );
    }
}

#[test]
fn infers_transaction_requirements_from_postgres_ast() {
    let cases = [
        (
            "SELECT id FROM users",
            SqlTransactionRequirement::AutocommitAllowed,
        ),
        (
            "SELECT id FROM users FOR UPDATE",
            SqlTransactionRequirement::ActiveTransactionRequired,
        ),
        (
            "SAVEPOINT before_write",
            SqlTransactionRequirement::ActiveTransactionRequired,
        ),
        (
            "ROLLBACK TO SAVEPOINT before_write",
            SqlTransactionRequirement::ActiveTransactionRequired,
        ),
        (
            "RELEASE SAVEPOINT before_write",
            SqlTransactionRequirement::ActiveTransactionRequired,
        ),
        ("BEGIN", SqlTransactionRequirement::VmManagedControl),
        ("COMMIT", SqlTransactionRequirement::VmManagedControl),
        ("ROLLBACK", SqlTransactionRequirement::VmManagedControl),
        (
            "SET TRANSACTION ISOLATION LEVEL SERIALIZABLE",
            SqlTransactionRequirement::VmManagedControl,
        ),
    ];

    for (sql, expected) in cases {
        let actual = transaction_requirement(sql);
        assert_eq!(actual, expected, "SQL: {sql}");
        assert_eq!(
            actual.as_diagnostic_label(),
            expected.as_diagnostic_label(),
            "SQL: {sql}"
        );
    }
}

#[test]
fn maps_cardinality_to_sql_wrapper_result_types() {
    assert_eq!(
        SqlCardinality::OptionalOne.result_type_text(Some("UserRow")),
        Some("Result[Option[UserRow], Error]".to_string())
    );
    assert_eq!(
        SqlCardinality::ManyRows.result_type_text(Some("UserRow")),
        Some("Result[List[UserRow], Error]".to_string())
    );
    assert_eq!(
        SqlCardinality::AffectedRows.result_type_text(Some("UserRow")),
        Some("Result[Int, Error]".to_string())
    );
    assert_eq!(
        SqlCardinality::Ambiguous.result_type_text(Some("UserRow")),
        None
    );
}

#[test]
fn extracts_simple_select_projection_fields() {
    assert_eq!(
        projection_fields(
            "\
            SELECT users.id, users.name AS display_name, active
            FROM users
            ",
        ),
        Some(vec![
            "id".to_string(),
            "display_name".to_string(),
            "active".to_string()
        ])
    );
}

#[test]
fn extracts_explicit_alias_for_select_expression_projection_fields() {
    assert_eq!(
        projection_fields("SELECT register_user(${name}, ${email}, ${active})::text AS id LIMIT 1",),
        Some(vec!["id".to_string()])
    );
}

#[test]
fn preserves_quoted_projection_aliases_from_postgres_ast() {
    assert_eq!(
        projection_fields("SELECT users.id AS \"displayName\" FROM users"),
        Some(vec!["displayName".to_string()])
    );
}

#[test]
fn extracts_simple_returning_projection_fields() {
    assert_eq!(
        projection_fields(
            "\
            INSERT INTO users (name, active)
            VALUES (${name}, true)
            RETURNING users.id, users.name AS display_name, active;
            ",
        ),
        Some(vec![
            "id".to_string(),
            "display_name".to_string(),
            "active".to_string()
        ])
    );
}

#[test]
fn derives_aliased_expression_and_cte_projection_fields_from_ast() {
    assert_eq!(
        projection_fields("SELECT count(*) total FROM users"),
        Some(vec!["total".to_string()])
    );
    assert_eq!(
        projection_fields("WITH users AS (SELECT id FROM accounts) SELECT id FROM users"),
        Some(vec!["id".to_string()])
    );
}

#[test]
fn keeps_wildcard_and_unaliased_expression_projection_fields_unknown() {
    assert_eq!(projection_fields("SELECT * FROM users"), None);
    assert_eq!(projection_fields("SELECT count(*) FROM users"), None);
    assert_eq!(projection_fields("DELETE FROM users RETURNING *"), None);
}

#[test]
fn rejects_duplicate_projection_output_names() {
    let body = first_function_body(
        "\
module sql_projection_duplicate.\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {SELECT id AS value, name AS value FROM users}.\n\
",
    );

    let error = analyze_sql_form(&body).expect_err("duplicate projection should fail");

    assert_eq!(
        error,
        SqlFormAnalysisError::Projection(SqlProjectionError::DuplicateOutputName(
            "value".to_string()
        ))
    );
    assert_eq!(
        error.message(),
        "SQL projection contains duplicate output name `value`"
    );
}

#[test]
fn folds_unquoted_projection_names_before_duplicate_validation() {
    let binding = bind_sql_parameters("SELECT id AS value, name AS VALUE FROM users")
        .expect("bind case-folded projection fixture");
    let statement = parse_single_postgres_statement(&binding.sql)
        .expect("parse case-folded projection fixture");

    assert_eq!(
        statement_projection_fields(&statement),
        Err(SqlProjectionError::DuplicateOutputName("value".to_string()))
    );
}

#[test]
fn keeps_quoted_projection_names_case_sensitive() {
    assert_eq!(
        projection_fields("SELECT id AS value, name AS \"VALUE\" FROM users"),
        Some(vec!["value".to_string(), "VALUE".to_string()])
    );
}

#[test]
fn rejects_duplicate_unqualified_names_from_compound_columns() {
    let binding = bind_sql_parameters(
        "SELECT users.id, accounts.id FROM users JOIN accounts ON users.id = accounts.user_id",
    )
    .expect("bind duplicate projection fixture");
    let statement =
        parse_single_postgres_statement(&binding.sql).expect("parse duplicate projection fixture");

    assert_eq!(
        statement_projection_fields(&statement),
        Err(SqlProjectionError::DuplicateOutputName("id".to_string()))
    );
}

#[test]
fn ignores_select_projection_keywords_inside_comments_and_strings() {
    assert_eq!(
        projection_fields(
            "\
            -- SELECT bad FROM ignored
            SELECT id, label
            FROM users
            WHERE note = 'FROM ignored'
            ",
        ),
        Some(vec!["id".to_string(), "label".to_string()])
    );
}

#[test]
fn rewrites_interpolations_to_postgres_placeholders_in_order() {
    assert_eq!(
        bind_sql_parameters(
            "\
            SELECT id, name
            FROM users
            WHERE id = ${id} AND active = ${active}
            ",
        )
        .expect("bind sql parameters"),
        SqlParameterBinding {
            sql: "\
            SELECT id, name
            FROM users
            WHERE id = $1 AND active = $2
            "
            .to_string(),
            parameter_count: 2,
        }
    );
}

#[test]
fn leaves_interpolation_text_inside_sql_strings_and_comments_untouched() {
    assert_eq!(
        bind_sql_parameters(
            "\
            -- ${ignored}
            SELECT '${also_ignored}', id
            FROM logs
            WHERE id = ${id}
            /* ${ignored_too} */
            ",
        )
        .expect("bind sql parameters"),
        SqlParameterBinding {
            sql: "\
            -- ${ignored}
            SELECT '${also_ignored}', id
            FROM logs
            WHERE id = $1
            /* ${ignored_too} */
            "
            .to_string(),
            parameter_count: 1,
        }
    );
}

#[test]
fn leaves_parameter_text_inside_postgres_dollar_quotes_untouched() {
    assert_eq!(
        bind_sql_parameters(
            "SELECT $$${ignored} $1$$, $body$${also_ignored} $2$body$ WHERE id = ${id}"
        )
        .expect("bind SQL containing dollar-quoted strings"),
        SqlParameterBinding {
            sql: "SELECT $$${ignored} $1$$, $body$${also_ignored} $2$body$ WHERE id = $1"
                .to_string(),
            parameter_count: 1,
        }
    );
}

#[test]
fn analyzes_dollar_quoted_sql_without_parameter_drift() {
    let body = first_function_body(
        "\
module sql_dollar_quote_analysis.\n\
pub query(id: Int): Dynamic ->\n\
    sql[UserRow] {SELECT $body$${ignored}$body$ AS payload WHERE id = ${id}}.\n\
",
    );

    let analysis = analyze_sql_form(&body)
        .expect("analyze dollar-quoted SQL")
        .expect("typed SQL analysis");

    assert_eq!(body.children.len(), 1);
    assert_eq!(analysis.binding.parameter_count, 1);
    assert_eq!(
        analysis.binding.sql,
        "SELECT $body$${ignored}$body$ AS payload WHERE id = $1"
    );
    assert!(analysis.is_ready_for_wrapper_lowering(body.children.len()));
}

#[test]
fn keeps_dollar_digits_inside_postgres_identifiers() {
    assert_eq!(
        bind_sql_parameters("SELECT metric$1 FROM samples WHERE id = ${id}")
            .expect("bind SQL containing dollar identifier"),
        SqlParameterBinding {
            sql: "SELECT metric$1 FROM samples WHERE id = $1".to_string(),
            parameter_count: 1,
        }
    );
}

#[test]
fn rejects_explicit_postgres_placeholders() {
    assert_eq!(
        bind_sql_parameters("SELECT id FROM users WHERE id = $1")
            .expect_err("explicit placeholder should fail"),
        SqlParameterBindingError::ExplicitPlaceholder
    );
}

#[test]
fn supports_nested_braces_inside_interpolation_expressions() {
    assert_eq!(
        bind_sql_parameters("SELECT * FROM users WHERE name = ${names.get({primary = true})}")
            .expect("bind nested interpolation"),
        SqlParameterBinding {
            sql: "SELECT * FROM users WHERE name = $1".to_string(),
            parameter_count: 1,
        }
    );
}

#[test]
fn rejects_empty_interpolation_during_binding() {
    assert_eq!(
        bind_sql_parameters("SELECT * FROM users WHERE id = ${}")
            .expect_err("empty interpolation should fail"),
        SqlParameterBindingError::EmptyInterpolation
    );
}

#[test]
fn reports_stable_binding_error_messages() {
    assert_eq!(
        SqlParameterBindingError::EmptyInterpolation.message(),
        "empty SQL interpolation expression"
    );
    assert_eq!(
        SqlParameterBindingError::ExplicitPlaceholder.message(),
        "explicit PostgreSQL placeholders are not allowed; use `${expression}`"
    );
    assert_eq!(
        SqlParameterBindingError::UnterminatedInterpolation.message(),
        "unterminated SQL interpolation expression"
    );
}

#[test]
fn analyzes_typed_sql_form_from_syntax_output() {
    let body = first_function_body(
        "\
module sql_analysis.\n\
pub find_user(id: Int): Dynamic ->\n\
    sql[UserRow] {\n\
      SELECT id, name FROM users WHERE id = ${id} LIMIT 1\n\
    }.\n\
",
    );

    let analysis = analyze_sql_form(&body)
        .expect("analyze sql form")
        .expect("typed sql analysis");

    assert_eq!(analysis.row_type.as_deref(), Some("UserRow"));
    assert_eq!(analysis.row_type_arg_count, 1);
    assert_eq!(analysis.row_type_arity_message(), None);
    assert_eq!(analysis.binding.parameter_count, 1);
    assert_eq!(analysis.query_kind, SqlQueryKind::Select);
    assert_eq!(
        analysis.transaction_requirement,
        SqlTransactionRequirement::AutocommitAllowed
    );
    assert_eq!(
        analysis.projection_fields,
        Some(vec!["id".to_string(), "name".to_string()])
    );
    assert!(
        analysis.binding.sql.contains("WHERE id = $1"),
        "bound sql: {}",
        analysis.binding.sql
    );
    assert_eq!(analysis.cardinality, SqlCardinality::OptionalOne);
    assert_eq!(
        analysis.result_type.as_deref(),
        Some("Result[Option[UserRow], Error]")
    );
    assert_eq!(
        analysis.parameter_count_consistency_message(1),
        "SQL parameter count consistency satisfied"
    );
    assert_eq!(
        analysis.parameter_count_consistency_message(2),
        "SQL parameter count mismatch: parsed 2 expression(s), bound 1 placeholder(s)"
    );
    assert_eq!(analysis.cardinality_requirement_message(), None);
}

#[test]
fn reports_ambiguous_sql_cardinality_from_analysis() {
    let body = first_function_body(
        "\
module sql_analysis_ambiguous_cardinality.\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {CREATE TABLE users (id INT)}.\n\
",
    );

    let analysis = analyze_sql_form(&body)
        .expect("analyze sql form")
        .expect("typed sql analysis");

    assert_eq!(analysis.cardinality, SqlCardinality::Ambiguous);
    assert_eq!(analysis.query_kind, SqlQueryKind::Ddl);
    assert_eq!(analysis.result_type, None);
    assert_eq!(
        analysis.cardinality_requirement_message().as_deref(),
        Some(
            "SQL form cardinality is ambiguous; use a clear SELECT, SELECT ... LIMIT 1, or RETURNING shape"
        )
    );
    assert!(!analysis.is_ready_for_wrapper_lowering(0));
    assert_eq!(
        analysis.wrapper_lowering_blockers(0),
        vec![
            "SQL form cardinality is ambiguous; use a clear SELECT, SELECT ... LIMIT 1, or RETURNING shape"
                .to_string()
        ]
    );
}

#[test]
fn rejects_empty_sql_form_text_from_analysis() {
    let body = first_function_body(
        "\
module sql_analysis_empty_text.\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {   }.\n\
",
    );

    assert_eq!(analyze_sql_form(&body), Err(SqlFormAnalysisError::EmptySql));
    assert_eq!(
        SqlFormAnalysisError::EmptySql.message(),
        "SQL form text must not be empty"
    );
}

#[test]
fn sql_macro_validation_rejects_malformed_postgres_syntax() {
    let body = first_function_body(
        "\
module sql_validation_malformed.\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {SELECT id FROM users WHERE (}.\n\
",
    );

    assert_eq!(
        analyze_sql_form(&body),
        Err(SqlFormAnalysisError::Syntax(
            SqlSyntaxValidationError::Malformed
        ))
    );
    assert_eq!(
        SqlSyntaxValidationError::Malformed.message(),
        "SQL form contains malformed PostgreSQL syntax"
    );
}

#[test]
fn sql_macro_validation_rejects_multiple_statements() {
    let body = first_function_body(
        "\
module sql_validation_multiple.\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {SELECT id FROM users; DELETE FROM users}.\n\
",
    );

    assert_eq!(
        analyze_sql_form(&body),
        Err(SqlFormAnalysisError::Syntax(
            SqlSyntaxValidationError::MultipleStatements
        ))
    );
}

#[test]
fn sql_macro_validation_rejects_comment_only_forms() {
    let body = first_function_body(
        "\
module sql_validation_comment_only.\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {-- no statement\n}.\n\
",
    );

    assert_eq!(
        analyze_sql_form(&body),
        Err(SqlFormAnalysisError::Syntax(
            SqlSyntaxValidationError::MissingStatement
        ))
    );
}

#[test]
fn sql_macro_validation_keeps_injection_shaped_values_parameterized() {
    let body = first_function_body(
        "\
module sql_validation_parameterized.\n\
pub query(input: String): Dynamic ->\n\
    sql[UserRow] {SELECT id FROM users WHERE name = ${input} LIMIT 1}.\n\
",
    );

    let analysis = analyze_sql_form(&body)
        .expect("validate parameterized SQL")
        .expect("typed SQL analysis");
    assert_eq!(analysis.binding.parameter_count, 1);
    assert!(analysis.binding.sql.contains("name = $1"));
    assert!(!analysis.binding.sql.contains("${input}"));
}

#[test]
fn reports_missing_sql_row_type_argument_from_analysis() {
    let body = first_function_body(
        "\
module sql_analysis_missing_row_type.\n\
pub find_user(): Dynamic ->\n\
    sql{SELECT id, name FROM users}.\n\
",
    );

    let analysis = analyze_sql_form(&body)
        .expect("analyze sql form")
        .expect("raw sql analysis");

    assert_eq!(analysis.row_type, None);
    assert_eq!(
        analysis.row_type_arity_message().as_deref(),
        Some("SQL form requires exactly one explicit row type argument, found 0")
    );
    assert!(!analysis.is_ready_for_wrapper_lowering(0));
    assert!(
        analysis
            .wrapper_lowering_readiness_message(0)
            .contains("SQL wrapper lowering readiness: blocked"),
        "readiness: {}",
        analysis.wrapper_lowering_readiness_message(0)
    );
}

#[test]
fn reports_ready_sql_wrapper_lowering_front_door() {
    let body = first_function_body(
        "\
module sql_analysis_ready_wrapper.\n\
pub find_user(id: Int): Dynamic ->\n\
    sql[UserRow] {SELECT id, name FROM users WHERE id = ${id} LIMIT 1}.\n\
",
    );

    let analysis = analyze_sql_form(&body)
        .expect("analyze sql form")
        .expect("typed sql analysis");

    assert!(analysis.is_ready_for_wrapper_lowering(1));
    assert_eq!(analysis.wrapper_lowering_blockers(1), Vec::<String>::new());
    assert_eq!(
        analysis.wrapper_lowering_readiness_message(1),
        "SQL wrapper lowering readiness: ready"
    );
}

#[test]
fn reports_parameter_drift_as_wrapper_lowering_blocker() {
    let body = first_function_body(
        "\
module sql_analysis_parameter_drift.\n\
pub find_user(id: Int): Dynamic ->\n\
    sql[UserRow] {SELECT id FROM users WHERE id = ${id} LIMIT 1}.\n\
",
    );

    let analysis = analyze_sql_form(&body)
        .expect("analyze sql form")
        .expect("typed sql analysis");

    assert!(!analysis.is_ready_for_wrapper_lowering(2));
    assert_eq!(
        analysis.wrapper_lowering_blockers(2),
        vec![
            "SQL parameter count mismatch: parsed 2 expression(s), bound 1 placeholder(s)"
                .to_string()
        ]
    );
}

#[test]
fn builds_ready_sql_wrapper_plan() {
    let body = first_function_body(
        "\
module sql_wrapper_plan_ready.\n\
pub find_user(id: Int): Dynamic ->\n\
    sql[UserRow] {SELECT id, name FROM users WHERE id = ${id} LIMIT 1}.\n\
",
    );

    let plan = build_sql_wrapper_plan(&body, 1)
        .expect("build wrapper plan")
        .expect("SQL wrapper plan");

    assert_eq!(plan.row_type, "UserRow");
    assert_eq!(plan.parameter_count, 1);
    assert_eq!(plan.query_kind, SqlQueryKind::Select);
    assert_eq!(
        plan.transaction_requirement,
        SqlTransactionRequirement::AutocommitAllowed
    );
    assert_eq!(plan.cardinality, SqlCardinality::OptionalOne);
    assert_eq!(plan.result_type, "Result[Option[UserRow], Error]");
    assert_eq!(
        plan.projection_fields,
        Some(vec!["id".to_string(), "name".to_string()])
    );
    assert!(
        plan.bound_sql.contains("WHERE id = $1"),
        "bound sql: {}",
        plan.bound_sql
    );
}

#[test]
fn builds_locking_query_with_active_transaction_requirement() {
    let body = first_function_body(
        "\
module sql_wrapper_plan_locking_query.\n\
pub lock_user(id: Int): Dynamic ->\n\
    sql[UserRow] {SELECT id FROM users WHERE id = ${id} LIMIT 1 FOR UPDATE}.\n\
",
    );

    let plan = build_sql_wrapper_plan(&body, 1)
        .expect("build locking wrapper plan")
        .expect("locking SQL wrapper plan");

    assert_eq!(
        plan.transaction_requirement,
        SqlTransactionRequirement::ActiveTransactionRequired
    );
    assert_eq!(plan.cardinality, SqlCardinality::OptionalOne);
}

#[test]
fn rejects_vm_owned_transaction_control_from_wrapper_plan() {
    let body = first_function_body(
        "\
module sql_wrapper_plan_transaction_control.\n\
pub begin_transaction(): Dynamic ->\n\
    sql[UnitRow] {BEGIN}.\n\
",
    );

    let analysis = analyze_sql_form(&body)
        .expect("analyze transaction control")
        .expect("transaction SQL analysis");
    assert_eq!(
        analysis.transaction_requirement,
        SqlTransactionRequirement::VmManagedControl
    );
    assert!(analysis.wrapper_lowering_blockers(0).iter().any(|blocker| {
        blocker == "SQL transaction control is VM-owned; use the typed database transaction API"
    }));

    let error = build_sql_wrapper_plan(&body, 0)
        .expect_err("raw transaction control must not produce a wrapper plan");
    assert!(
        error.message().contains(
            "SQL transaction control is VM-owned; use the typed database transaction API"
        ),
        "error: {}",
        error.message()
    );
}

#[test]
fn builds_returning_sql_wrapper_plan_projection_fields() {
    let body = first_function_body(
        "\
module sql_wrapper_plan_returning.\n\
pub create_user(name: String): Dynamic ->\n\
    sql[UserRow] {INSERT INTO users (name) VALUES (${name}) RETURNING id, name}.\n\
",
    );

    let plan = build_sql_wrapper_plan(&body, 1)
        .expect("build returning wrapper plan")
        .expect("SQL wrapper plan");

    assert_eq!(plan.row_type, "UserRow");
    assert_eq!(plan.parameter_count, 1);
    assert_eq!(plan.cardinality, SqlCardinality::ManyRows);
    assert_eq!(plan.result_type, "Result[List[UserRow], Error]");
    assert_eq!(
        plan.projection_fields,
        Some(vec!["id".to_string(), "name".to_string()])
    );
}

#[test]
fn wrapper_plan_ignores_non_sql_expressions() {
    let body = first_function_body(
        "\
module sql_wrapper_plan_non_sql.\n\
pub value(): Int ->\n\
    1.\n\
",
    );

    assert_eq!(
        build_sql_wrapper_plan(&body, 0).expect("non-SQL wrapper plan"),
        None
    );
}

#[test]
fn wrapper_plan_reports_readiness_blockers() {
    let body = first_function_body(
        "\
module sql_wrapper_plan_blocked.\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {CREATE TABLE users (id INT)}.\n\
",
    );

    let error = build_sql_wrapper_plan(&body, 0).expect_err("ambiguous SQL should block plan");

    assert!(matches!(error, SqlWrapperPlanError::NotReady(_)));
    assert!(
        error
            .message()
            .contains("SQL form cardinality is ambiguous"),
        "error: {}",
        error.message()
    );
}

#[test]
fn analysis_ignores_non_sql_expressions() {
    let body = first_function_body(
        "\
module sql_analysis_ignores_non_sql.\n\
pub value(): Int ->\n\
    42.\n\
",
    );

    assert_eq!(
        analyze_sql_form(&body).expect("analyze non-sql expression"),
        None
    );
}

/// Returns the first function clause body from a syntax-output fixture.
///
/// Inputs:
/// - `source`: complete Terlan module source with one function declaration.
///
/// Output:
/// - The first function clause body expression.
///
/// Transformation:
/// - Parses source through the formal syntax-output path and extracts the
///   expression node used by SQL-form analysis tests.
fn first_function_body(source: &str) -> SyntaxExprOutput {
    let module = parse_module_as_syntax_output(source).expect("parse syntax-output module");
    let declaration = module
        .declarations
        .first()
        .expect("module should contain one declaration");
    let SyntaxDeclarationPayload::Function { clauses, .. } = &declaration.payload else {
        panic!("expected function declaration");
    };
    clauses
        .first()
        .expect("function should contain one clause")
        .body
        .clone()
}

fn infer_sql_cardinality(raw: &str) -> SqlCardinality {
    let binding = bind_sql_parameters(raw).expect("bind SQL cardinality fixture");
    let statement =
        parse_single_postgres_statement(&binding.sql).expect("parse SQL cardinality fixture");
    statement_cardinality(&statement)
}

fn transaction_requirement(raw: &str) -> SqlTransactionRequirement {
    let binding = bind_sql_parameters(raw).expect("bind SQL transaction fixture");
    let statement =
        parse_single_postgres_statement(&binding.sql).expect("parse SQL transaction fixture");
    statement_transaction_requirement(&statement)
}

fn projection_fields(raw: &str) -> Option<Vec<String>> {
    let binding = bind_sql_parameters(raw).expect("bind SQL projection fixture");
    let statement =
        parse_single_postgres_statement(&binding.sql).expect("parse SQL projection fixture");
    statement_projection_fields(&statement).expect("derive SQL projection fixture")
}
