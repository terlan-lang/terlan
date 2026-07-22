use super::test_support::check_syntax_output;

#[test]
fn sql_parameter_types_accept_runtime_scalar_contract() {
    let diagnostics = check_syntax_output(
        "\
module sql_parameter_scalar_contract.\n\
\n\
pub type UserId = Int.\n\
\n\
pub struct UserRow {\n\
    id: Int\n\
}.\n\
\n\
pub query(\n\
    id: UserId,\n\
    ratio: Float,\n\
    amount: Number,\n\
    name: String,\n\
    active: Bool\n\
): Dynamic ->\n\
    sql[UserRow] {\n\
        SELECT id FROM users\n\
        WHERE id = ${id}\n\
          AND ratio = ${ratio}\n\
          AND amount = ${amount}\n\
          AND name = ${name}\n\
          AND active = ${active}\n\
          AND rank = ${1}\n\
    }.\n",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn sql_parameter_types_reject_structured_dynamic_and_nullable_values() {
    let diagnostics = check_syntax_output(
        "\
module sql_parameter_rejection_contract.\n\
\n\
pub opaque type Maybe[T].\n\
\n\
pub struct UserRow {\n\
    id: Int\n\
}.\n\
\n\
pub bad_list(value: List[Int]): Dynamic ->\n\
    sql[UserRow] {SELECT id FROM users WHERE id = ${value}}.\n\
\n\
pub bad_map(value: Map[String, Int]): Dynamic ->\n\
    sql[UserRow] {SELECT id FROM users WHERE id = ${value}}.\n\
\n\
pub bad_function(value: (Int) -> Int): Dynamic ->\n\
    sql[UserRow] {SELECT id FROM users WHERE id = ${value}}.\n\
\n\
pub bad_dynamic(value: Dynamic): Dynamic ->\n\
    sql[UserRow] {SELECT id FROM users WHERE id = ${value}}.\n\
\n\
pub bad_nullable(value: Maybe[Int]): Dynamic ->\n\
    sql[UserRow] {SELECT id FROM users WHERE id = ${value}}.\n",
    );
    let messages = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>();

    for expected in [
        "SQL parameter 1 has non-bindable type List[Int]",
        "SQL parameter 1 has non-bindable type Map[Binary, Int]",
        "SQL parameter 1 has non-bindable type (Int) -> Int",
        "SQL parameter 1 has non-bindable type Dynamic",
        "SQL parameter 1 has non-bindable type Maybe[Int]",
    ] {
        assert!(
            messages.iter().any(|message| message.contains(expected)),
            "missing `{expected}` in diagnostics: {diagnostics:?}"
        );
    }
}

#[test]
fn sql_parameter_type_diagnostic_preserves_interpolation_index_and_span() {
    let diagnostics = check_syntax_output(
        "\
module sql_parameter_index_contract.\n\
\n\
pub struct UserRow {\n\
    id: Int\n\
}.\n\
\n\
pub query(id: Int, invalid: List[Int]): Dynamic ->\n\
    sql[UserRow] {SELECT id FROM users WHERE id = ${id} AND other = ${invalid}}.\n",
    );
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic
                .message
                .contains("SQL parameter 2 has non-bindable type")
        })
        .unwrap_or_else(|| panic!("missing indexed SQL diagnostic: {diagnostics:?}"));

    assert_eq!(
        diagnostic.message,
        "SQL parameter 2 has non-bindable type List[Int]; expected Int, Float, Number, Binary, or Bool"
    );
    assert!(diagnostic.span.end > diagnostic.span.start);
}
