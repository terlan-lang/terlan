use super::sql_forms::{
    analyze_sql_form, bind_sql_parameters, build_sql_wrapper_plan, infer_sql_cardinality,
    projection::simple_select_projection_fields, simple_sql_projection_fields, SqlCardinality,
    SqlFormAnalysisError, SqlParameterBinding, SqlParameterBindingError, SqlWrapperPlanError,
};
use terlan_syntax::{parse_module_as_syntax_output, SyntaxDeclarationPayload, SyntaxExprOutput};

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
fn leaves_unclear_statement_shapes_ambiguous() {
    assert_eq!(
        infer_sql_cardinality("WITH users AS (SELECT * FROM accounts) SELECT * FROM users"),
        SqlCardinality::Ambiguous
    );
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
        simple_select_projection_fields(
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
        simple_select_projection_fields("SELECT register_user($1, $2, $3)::text AS id LIMIT 1"),
        Some(vec!["id".to_string()])
    );
}

#[test]
fn extracts_simple_returning_projection_fields() {
    assert_eq!(
        simple_sql_projection_fields(
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
fn skips_complex_select_projection_fields() {
    assert_eq!(
        simple_select_projection_fields("SELECT count(*) total FROM users"),
        None
    );
    assert_eq!(simple_select_projection_fields("SELECT * FROM users"), None);
    assert_eq!(
        simple_select_projection_fields(
            "WITH users AS (SELECT id FROM accounts) SELECT id FROM users"
        ),
        None
    );
}

#[test]
fn skips_complex_returning_projection_fields() {
    assert_eq!(
        simple_sql_projection_fields("UPDATE users SET active = true RETURNING count(*) total"),
        None
    );
    assert_eq!(
        simple_sql_projection_fields("DELETE FROM users RETURNING *"),
        None
    );
}

#[test]
fn ignores_select_projection_keywords_inside_comments_and_strings() {
    assert_eq!(
        simple_select_projection_fields(
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
    sql[UserRow] {WITH users AS (SELECT * FROM accounts) SELECT * FROM users}.\n\
",
    );

    let analysis = analyze_sql_form(&body)
        .expect("analyze sql form")
        .expect("typed sql analysis");

    assert_eq!(analysis.cardinality, SqlCardinality::Ambiguous);
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
    sql[UserRow] {WITH users AS (SELECT * FROM accounts) SELECT * FROM users}.\n\
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
