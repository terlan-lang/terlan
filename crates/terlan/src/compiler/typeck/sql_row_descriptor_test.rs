use super::test_support::check_syntax_output;

#[test]
fn sql_row_descriptor_accepts_scalar_tuple_and_infers_result_type() {
    let diagnostics = check_syntax_output(
        "\
module sql_tuple_row_descriptor.\n\
\n\
pub query(): Int ->\n\
    sql[{Int, String}] {SELECT id, name FROM users LIMIT 1}.\n",
    );

    assert_eq!(diagnostics.len(), 1, "diagnostics: {diagnostics:?}");
    assert_eq!(
        diagnostics[0].message,
        "expected Int found (error, Dynamic) | (ok, (some, (Int, Binary)) | none)"
    );
}

#[test]
fn sql_row_descriptor_accepts_transparent_scalar_tuple_alias() {
    let diagnostics = check_syntax_output(
        "\
module sql_tuple_row_alias.\n\
\n\
pub type UserRow = {Int, String}.\n\
\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {SELECT id, name FROM users}.\n",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn sql_row_descriptor_rejects_tuple_projection_arity_mismatch() {
    let diagnostics = check_syntax_output(
        "\
module sql_tuple_row_arity.\n\
\n\
pub query(): Dynamic ->\n\
    sql[{Int, String}] {SELECT id FROM users}.\n",
    );

    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic.message
            == "SQL projection has 1 column(s), but tuple row type `{Int, String}` has 2 field(s)"),
        "diagnostics: {diagnostics:?}"
    );
}

#[test]
fn sql_row_descriptor_rejects_non_decodable_structural_fields() {
    let diagnostics = check_syntax_output(
        "\
module sql_row_decode_contract.\n\
\n\
pub type BadAlias = List[Int].\n\
\n\
pub struct StructuredRow {\n\
    id: Int,\n\
    tags: List[String]\n\
}.\n\
\n\
pub bad_tuple(): Dynamic ->\n\
    sql[{Int, List[String]}] {SELECT id, tags FROM users}.\n\
\n\
pub bad_struct(): Dynamic ->\n\
    sql[StructuredRow] {SELECT id, tags FROM users}.\n\
\n\
pub bad_alias(): Dynamic ->\n\
    sql[BadAlias] {SELECT id FROM users}.\n",
    );
    let messages = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>();

    for expected in [
        "SQL tuple row field 2 has non-decodable type List[Binary]",
        "SQL row type `StructuredRow` field `tags` has non-decodable type List[Binary]",
        "SQL row type `BadAlias` must resolve to a visible named row type or non-empty scalar tuple",
    ] {
        assert!(
            messages.iter().any(|message| message.contains(expected)),
            "missing `{expected}` in diagnostics: {diagnostics:?}"
        );
    }
}

#[test]
fn sql_row_descriptor_rejects_numeric_types_without_vm_row_codecs() {
    let diagnostics = check_syntax_output(
        "\
module sql_numeric_row_decode_contract.\n\
\n\
pub query(): Dynamic ->\n\
    sql[{Float, Number}] {SELECT ratio, amount FROM metrics}.\n",
    );
    for expected in [
        "SQL tuple row field 1 has non-decodable type Float",
        "SQL tuple row field 2 has non-decodable type Number",
    ] {
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains(expected)),
            "missing `{expected}` in diagnostics: {diagnostics:?}"
        );
    }
}

#[test]
fn sql_row_descriptor_accepts_nullable_scalar_tuple_and_struct_fields() {
    let diagnostics = check_syntax_output(
        "\
module sql_nullable_row_decode_contract.\n\
\n\
pub type Option[T] = Atom[\"none\"] | {Atom[\"some\"], T}.\n\
\n\
pub struct UserRow {\n\
    id: Int,\n\
    nickname: Option[String]\n\
}.\n\
\n\
pub tuple_query(): Dynamic ->\n\
    sql[{Int, Option[String]}] {SELECT id, nickname FROM users}.\n\
\n\
pub struct_query(): Dynamic ->\n\
    sql[UserRow] {SELECT id, nickname FROM users}.\n",
    );

    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn sql_row_descriptor_rejects_nullable_structured_payloads() {
    let diagnostics = check_syntax_output(
        "\
module sql_nullable_row_decode_rejection.\n\
\n\
pub type Option[T] = Atom[\"none\"] | {Atom[\"some\"], T}.\n\
\n\
pub struct UserRow {\n\
    id: Int,\n\
    tags: Option[List[String]]\n\
}.\n\
\n\
pub tuple_query(): Dynamic ->\n\
    sql[{Int, Option[List[String]]}] {SELECT id, tags FROM users}.\n\
\n\
pub struct_query(): Dynamic ->\n\
    sql[UserRow] {SELECT id, tags FROM users}.\n",
    );
    let messages = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>();

    for expected in [
        "SQL tuple row field 2 has non-decodable type",
        "SQL row type `UserRow` field `tags` has non-decodable type",
    ] {
        assert!(
            messages.iter().any(|message| message.contains(expected)),
            "missing `{expected}` in diagnostics: {diagnostics:?}"
        );
    }
}
