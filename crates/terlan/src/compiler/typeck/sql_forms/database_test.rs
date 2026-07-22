use crate::database_schema::{
    schema_fingerprint, DatabaseSchemaSnapshot, SchemaColumn, SchemaRelation,
    DATABASE_SCHEMA_SNAPSHOT_SCHEMA,
};

use super::*;
use crate::terlan_typeck::sql_forms::parse_single_postgres_statement;

fn snapshot() -> DatabaseSchemaSnapshot {
    let relations = vec![SchemaRelation {
        schema: "public".to_string(),
        name: "users".to_string(),
        kind: "BASE TABLE".to_string(),
        columns: vec![column("id", 1), column("display_name", 2)],
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

fn column(name: &str, ordinal: i64) -> SchemaColumn {
    SchemaColumn {
        name: name.to_string(),
        ordinal,
        data_type: "text".to_string(),
        user_type_schema: "pg_catalog".to_string(),
        user_type_name: "text".to_string(),
        nullable: false,
        default: None,
        identity: false,
        identity_generation: None,
        generated: false,
        generation_expression: None,
    }
}

fn projection(
    sql: &str,
) -> Result<Option<Vec<SqlDatabaseProjectionColumn>>, SqlDatabaseValidationError> {
    let statement = parse_single_postgres_statement(sql).expect("parse SQL");
    statement_schema_projection(&statement, &snapshot())
}

#[test]
fn snapshot_validation_expands_wildcards_and_respects_aliases() {
    assert_eq!(
        projection("SELECT users.* FROM public.users")
            .expect("projection")
            .map(projection_names),
        Some(vec!["id".to_string(), "display_name".to_string()])
    );
    assert_eq!(
        projection("SELECT u.id, u.display_name AS name FROM users AS u")
            .expect("projection")
            .map(projection_names),
        Some(vec!["id".to_string(), "name".to_string()])
    );
}

fn projection_names(columns: Vec<SqlDatabaseProjectionColumn>) -> Vec<String> {
    columns
        .into_iter()
        .map(|column| column.output_name)
        .collect()
}

#[test]
fn snapshot_validation_preserves_source_type_and_nullability_through_aliases() {
    let columns = projection("SELECT display_name AS name FROM users")
        .expect("projection")
        .expect("resolved projection");
    assert_eq!(columns[0].output_name, "name");
    assert_eq!(columns[0].source_column.user_type_name, "text");
    assert!(!columns[0].source_column.nullable);
}

#[test]
fn snapshot_validation_defers_computed_aliases_to_live_describe() {
    assert_eq!(
        projection("SELECT id + 1 AS next_id FROM users").expect("projection"),
        None
    );
}

#[test]
fn snapshot_validation_rejects_unknown_relations_columns_and_qualifiers() {
    assert!(matches!(
        projection("SELECT id FROM accounts"),
        Err(SqlDatabaseValidationError::UnknownRelation(_))
    ));
    assert!(matches!(
        projection("SELECT missing FROM users"),
        Err(SqlDatabaseValidationError::UnknownColumn { .. })
    ));
    assert!(matches!(
        projection("SELECT account.id FROM users"),
        Err(SqlDatabaseValidationError::UnknownQualifier(_))
    ));
}

#[test]
fn snapshot_validation_preserves_quoted_identifier_case() {
    let error = projection("SELECT \"ID\" FROM users").expect_err("case mismatch must fail");
    assert_eq!(
        error.message(),
        "SQL query references unknown column `ID` on relation `public.users`"
    );
}
