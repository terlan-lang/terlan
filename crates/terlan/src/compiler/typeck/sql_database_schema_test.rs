use crate::database_schema::{
    schema_fingerprint, DatabaseSchemaSnapshot, SchemaColumn, SchemaRelation,
    DATABASE_SCHEMA_SNAPSHOT_SCHEMA,
};

use super::test_support::check_syntax_output_with_database_schema;

fn schema_snapshot() -> DatabaseSchemaSnapshot {
    snapshot_with_columns(vec![
        column("id", 1, "int8", false),
        column("display_name", 2, "text", false),
    ])
}

fn codec_schema_snapshot() -> DatabaseSchemaSnapshot {
    snapshot_with_columns(vec![
        column("id", 1, "int8", false),
        column("display_name", 2, "text", false),
        column("active", 3, "bool", false),
        column("nickname", 4, "text", true),
        column("profile", 5, "jsonb", false),
        column("amount", 6, "numeric", false),
    ])
}

fn snapshot_with_columns(columns: Vec<SchemaColumn>) -> DatabaseSchemaSnapshot {
    let relations = vec![SchemaRelation {
        schema: "public".to_string(),
        name: "users".to_string(),
        kind: "BASE TABLE".to_string(),
        columns,
        constraints: Vec::new(),
        indexes: Vec::new(),
    }];
    let enums = Vec::new();
    DatabaseSchemaSnapshot {
        schema: DATABASE_SCHEMA_SNAPSHOT_SCHEMA.to_string(),
        database_product: "PostgreSQL".to_string(),
        migration_snapshot_id: "sha256:migrations".to_string(),
        schema_fingerprint: schema_fingerprint(&relations, &enums).expect("fingerprint"),
        relations,
        enums,
    }
}

fn column(name: &str, ordinal: i64, user_type_name: &str, nullable: bool) -> SchemaColumn {
    SchemaColumn {
        name: name.to_string(),
        ordinal,
        data_type: user_type_name.to_string(),
        user_type_schema: "pg_catalog".to_string(),
        user_type_name: user_type_name.to_string(),
        nullable,
        default: None,
        identity: false,
        identity_generation: None,
        generated: false,
        generation_expression: None,
    }
}

fn source(sql: &str) -> String {
    source_with_fields("id: Int,\n             display_name: String", sql)
}

fn source_with_fields(fields: &str, sql: &str) -> String {
    format!(
        "module sql_database_schema.\n\
         \n\
         pub struct UserRow {{\n\
             {fields}\n\
         }}.\n\
         \n\
         pub query(): Dynamic ->\n\
             sql[UserRow] {{{sql}}}.\n"
    )
}

#[test]
fn database_schema_expands_wildcards_for_row_shape_validation() {
    let diagnostics = check_syntax_output_with_database_schema(
        &source("SELECT * FROM users"),
        &schema_snapshot(),
    );
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn database_schema_rejects_unknown_relations_and_columns() {
    let relation_diagnostics = check_syntax_output_with_database_schema(
        &source("SELECT id, display_name FROM accounts"),
        &schema_snapshot(),
    );
    assert!(relation_diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("unknown relation `public.accounts`")));

    let column_diagnostics = check_syntax_output_with_database_schema(
        &source("SELECT id, missing FROM users"),
        &codec_schema_snapshot(),
    );
    assert!(column_diagnostics.iter().any(|diagnostic| diagnostic
        .message
        .contains("unknown column `missing` on relation `public.users`")));
}

#[test]
fn database_schema_projection_is_checked_against_declared_row_fields() {
    let diagnostics = check_syntax_output_with_database_schema(
        &source("SELECT id FROM users"),
        &codec_schema_snapshot(),
    );
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message
        == "SQL row type `UserRow` field `display_name` is not selected by this query"));
}

#[test]
fn database_schema_accepts_exact_scalar_codecs_and_nullable_rows() {
    let diagnostics = check_syntax_output_with_database_schema(
        &source_with_fields(
            "id: Int,\n             active: Bool,\n             nickname: Option[String]",
            "SELECT id, active, nickname FROM users",
        )
        .replace(
            "module sql_database_schema.\n",
            "module sql_database_schema.\n\npub type Option[T] = Atom[\"none\"] | {Atom[\"some\"], T}.\n",
        ),
        &codec_schema_snapshot(),
    );
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn database_schema_rejects_codec_mismatches_for_structs_and_tuples() {
    let struct_diagnostics = check_syntax_output_with_database_schema(
        &source_with_fields(
            "id: String,\n             active: Int",
            "SELECT id, active FROM users",
        ),
        &codec_schema_snapshot(),
    );
    assert!(struct_diagnostics.iter().any(|diagnostic| diagnostic.message
        == "SQL selected column `id` decodes as Int, but row type `UserRow` field `id` has type Binary"));
    assert!(struct_diagnostics.iter().any(|diagnostic| diagnostic.message
        == "SQL selected column `active` decodes as Bool, but row type `UserRow` field `active` has type Int"));

    let tuple_source = "module sql_database_tuple.\n\n\
pub query(): Dynamic ->\n\
    sql[{String, Int}] {SELECT id, active FROM users}.\n";
    let tuple_diagnostics =
        check_syntax_output_with_database_schema(tuple_source, &codec_schema_snapshot());
    assert!(tuple_diagnostics.iter().any(|diagnostic| diagnostic.message
        == "SQL selected column `id` decodes as Int, but tuple row field 1 has type Binary"));
    assert!(tuple_diagnostics.iter().any(|diagnostic| diagnostic.message
        == "SQL selected column `active` decodes as Bool, but tuple row field 2 has type Int"));
}

#[test]
fn database_schema_enforces_nullability_in_both_directions() {
    let source = source_with_fields(
        "id: Option[Int],\n             nickname: String",
        "SELECT id, nickname FROM users",
    )
    .replace(
        "module sql_database_schema.\n",
        "module sql_database_schema.\n\npub type Option[T] = Atom[\"none\"] | {Atom[\"some\"], T}.\n",
    );
    let diagnostics = check_syntax_output_with_database_schema(&source, &codec_schema_snapshot());
    assert!(
        diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .starts_with("SQL selected column `id` decodes as Int, but row type `UserRow` field `id` has type ")),
        "diagnostics: {diagnostics:?}"
    );
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message
        == "SQL selected column `nickname` decodes as Option[Binary], but row type `UserRow` field `nickname` has type Binary"));
}

#[test]
fn database_schema_preserves_source_codec_through_projection_aliases() {
    let diagnostics = check_syntax_output_with_database_schema(
        &source_with_fields("user_id: Int", "SELECT id AS user_id FROM users"),
        &schema_snapshot(),
    );
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");
}

#[test]
fn database_schema_rejects_catalog_types_without_typed_vm_codecs() {
    let diagnostics = check_syntax_output_with_database_schema(
        &source_with_fields("amount: String", "SELECT amount FROM users"),
        &codec_schema_snapshot(),
    );
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message
        == "SQL selected column `amount` uses unsupported PostgreSQL type `pg_catalog.numeric`; no typed Terlan row codec is available"));
}

#[test]
fn database_schema_accepts_only_the_canonical_json_codec_type() {
    let canonical = "module sql_database_json.\n\n\
import type std.data.Json.\n\n\
pub struct UserRow {\n\
    profile: Json\n\
}.\n\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {SELECT profile FROM users}.\n";
    let diagnostics = check_syntax_output_with_database_schema(canonical, &codec_schema_snapshot());
    assert!(diagnostics.is_empty(), "diagnostics: {diagnostics:?}");

    let unrelated = "module sql_database_json.\n\n\
pub opaque type Json.\n\n\
pub struct UserRow {\n\
    profile: Json\n\
}.\n\n\
pub query(): Dynamic ->\n\
    sql[UserRow] {SELECT profile FROM users}.\n";
    let diagnostics = check_syntax_output_with_database_schema(unrelated, &codec_schema_snapshot());
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.message
        == "SQL row type `UserRow` field `profile` has non-decodable type Json; expected Int, Binary, Bool, std.data.Json.Json, or Option of one of these"));
}
