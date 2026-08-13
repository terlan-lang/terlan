//! Deterministic Postgres schema snapshots for compiler SQL validation.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::migration::MigrationEngineInput;
use crate::database_schema::{
    prefixed_digest, schema_fingerprint, DatabaseSchemaSnapshot, SchemaColumn, SchemaConstraint,
    SchemaEnum, SchemaIndex, SchemaRelation, DATABASE_SCHEMA_SNAPSHOT_SCHEMA,
};
#[cfg(any(
    test,
    all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
))]
use crate::runtime::vm::postgres::VmPostgresDecodedValue;
use crate::runtime::vm::{postgres::VmPostgresRow, postgres_command::VmPostgresCommandClient};

const RELATION_SQL: &str = r#"
SELECT table_schema, table_name, table_type
FROM information_schema.tables
WHERE table_schema <> 'information_schema'
  AND table_schema NOT LIKE 'pg_%'
  AND table_name <> 'terlan_schema_migrations'
ORDER BY table_schema, table_name
"#;

const COLUMN_SQL: &str = r#"
SELECT table_schema,
       table_name,
       column_name,
       ordinal_position::bigint AS ordinal_position,
       data_type,
       udt_schema,
       udt_name,
       (is_nullable = 'YES') AS nullable,
       column_default,
       (is_identity = 'YES') AS identity,
       identity_generation,
       (is_generated <> 'NEVER') AS generated,
       generation_expression
FROM information_schema.columns
WHERE table_schema <> 'information_schema'
  AND table_schema NOT LIKE 'pg_%'
  AND table_name <> 'terlan_schema_migrations'
ORDER BY table_schema, table_name, ordinal_position
"#;

const CONSTRAINT_SQL: &str = r#"
SELECT namespace.nspname AS table_schema,
       relation.relname AS table_name,
       constraint_row.conname AS constraint_name,
       constraint_row.contype::text AS constraint_type,
       pg_get_constraintdef(constraint_row.oid, true) AS definition
FROM pg_constraint AS constraint_row
JOIN pg_class AS relation ON relation.oid = constraint_row.conrelid
JOIN pg_namespace AS namespace ON namespace.oid = relation.relnamespace
WHERE namespace.nspname <> 'information_schema'
  AND namespace.nspname NOT LIKE 'pg_%'
  AND relation.relname <> 'terlan_schema_migrations'
ORDER BY namespace.nspname, relation.relname, constraint_row.conname
"#;

const INDEX_SQL: &str = r#"
SELECT schemaname AS table_schema,
       tablename AS table_name,
       indexname AS index_name,
       indexdef AS definition
FROM pg_indexes
WHERE schemaname <> 'information_schema'
  AND schemaname NOT LIKE 'pg_%'
  AND tablename <> 'terlan_schema_migrations'
ORDER BY schemaname, tablename, indexname
"#;

const ENUM_SQL: &str = r#"
SELECT namespace.nspname AS type_schema,
       enum_type.typname AS type_name,
       enum_value.enumlabel AS label
FROM pg_type AS enum_type
JOIN pg_namespace AS namespace ON namespace.oid = enum_type.typnamespace
JOIN pg_enum AS enum_value ON enum_value.enumtypid = enum_type.oid
WHERE namespace.nspname <> 'information_schema'
  AND namespace.nspname NOT LIKE 'pg_%'
ORDER BY namespace.nspname, enum_type.typname, enum_value.enumsortorder
"#;

pub(super) fn default_snapshot_path(migration_directory: &Path) -> PathBuf {
    migration_directory
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("schema.snapshot.json")
}

/// Captures a canonical schema snapshot through the VM-owned Postgres client.
pub(super) fn capture_schema_snapshot(
    client: &mut VmPostgresCommandClient,
    migrations: &[MigrationEngineInput],
) -> Result<DatabaseSchemaSnapshot, String> {
    let mut relations = load_relations(client)?;
    load_columns(client, &mut relations)?;
    load_constraints(client, &mut relations)?;
    load_indexes(client, &mut relations)?;
    let enums = load_enums(client)?;
    let relations = relations.into_values().collect::<Vec<_>>();
    let migration_snapshot_id = migration_snapshot_id(migrations);
    let schema_fingerprint = schema_fingerprint(&relations, &enums)?;

    Ok(DatabaseSchemaSnapshot {
        schema: DATABASE_SCHEMA_SNAPSHOT_SCHEMA.to_string(),
        database_product: "PostgreSQL".to_string(),
        migration_snapshot_id,
        schema_fingerprint,
        relations,
        enums,
    })
}

/// Writes a snapshot atomically enough to avoid leaving a partial cache file.
pub(super) fn write_schema_snapshot(
    path: &Path,
    snapshot: &DatabaseSchemaSnapshot,
) -> Result<(), String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "cannot create schema snapshot directory `{}`: {error}",
            parent.display()
        )
    })?;
    let text = serde_json::to_string_pretty(snapshot)
        .map_err(|error| format!("cannot serialize schema snapshot: {error}"))?;
    let temporary = temporary_snapshot_path(path);
    fs::write(&temporary, format!("{text}\n")).map_err(|error| {
        format!(
            "cannot write schema snapshot `{}`: {error}",
            temporary.display()
        )
    })?;
    fs::rename(&temporary, path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!(
            "cannot replace schema snapshot `{}`: {error}",
            path.display()
        )
    })
}

/// Checks a persisted snapshot against a newly captured database contract.
pub(super) fn check_schema_snapshot(
    path: &Path,
    current: &DatabaseSchemaSnapshot,
) -> Result<(), String> {
    let persisted = DatabaseSchemaSnapshot::load(path)?;
    if persisted == *current {
        return Ok(());
    }
    if persisted.migration_snapshot_id == current.migration_snapshot_id
        && persisted.schema_fingerprint != current.schema_fingerprint
    {
        return Err(dirty_schema_message(path, &persisted, current));
    }
    Err(schema_snapshot_drift_message(path, &persisted, current))
}

fn dirty_schema_message(
    path: &Path,
    persisted: &DatabaseSchemaSnapshot,
    current: &DatabaseSchemaSnapshot,
) -> String {
    format!(
        "error[db.schema.dirty]: Schema at snapshot `{}` changed while migration identity {} remained unchanged: expected schema {}, found {}.",
        path.display(),
        persisted.migration_snapshot_id,
        persisted.schema_fingerprint,
        current.schema_fingerprint
    )
}

fn schema_snapshot_drift_message(
    path: &Path,
    persisted: &DatabaseSchemaSnapshot,
    current: &DatabaseSchemaSnapshot,
) -> String {
    format!(
        "error[db.snapshot.drift]: Schema snapshot `{}` is stale: expected migrations {} and schema {}, found migrations {} and schema {}.",
        path.display(),
        persisted.migration_snapshot_id,
        persisted.schema_fingerprint,
        current.migration_snapshot_id,
        current.schema_fingerprint
    )
}

fn load_relations(
    client: &mut VmPostgresCommandClient,
) -> Result<BTreeMap<(String, String), SchemaRelation>, String> {
    let mut relations = BTreeMap::new();
    for row in client.query(RELATION_SQL, Vec::new())? {
        let schema = decode_string(client, row, "table_schema")?;
        let name = decode_string(client, row, "table_name")?;
        let kind = decode_string(client, row, "table_type")?;
        let key = (schema.clone(), name.clone());
        relations.insert(
            key,
            SchemaRelation {
                schema,
                name,
                kind,
                columns: Vec::new(),
                constraints: Vec::new(),
                indexes: Vec::new(),
            },
        );
    }
    Ok(relations)
}

fn load_columns(
    client: &mut VmPostgresCommandClient,
    relations: &mut BTreeMap<(String, String), SchemaRelation>,
) -> Result<(), String> {
    for row in client.query(COLUMN_SQL, Vec::new())? {
        let key = relation_key(client, row)?;
        let column = SchemaColumn {
            name: decode_string(client, row, "column_name")?,
            ordinal: decode_int(client, row, "ordinal_position")?,
            data_type: decode_string(client, row, "data_type")?,
            user_type_schema: decode_string(client, row, "udt_schema")?,
            user_type_name: decode_string(client, row, "udt_name")?,
            nullable: decode_bool(client, row, "nullable")?,
            default: decode_optional_string(client, row, "column_default")?,
            identity: decode_bool(client, row, "identity")?,
            identity_generation: decode_optional_string(client, row, "identity_generation")?,
            generated: decode_bool(client, row, "generated")?,
            generation_expression: decode_optional_string(client, row, "generation_expression")?,
        };
        relation_mut(relations, &key)?.columns.push(column);
    }
    Ok(())
}

fn load_constraints(
    client: &mut VmPostgresCommandClient,
    relations: &mut BTreeMap<(String, String), SchemaRelation>,
) -> Result<(), String> {
    for row in client.query(CONSTRAINT_SQL, Vec::new())? {
        let key = relation_key(client, row)?;
        let constraint = SchemaConstraint {
            name: decode_string(client, row, "constraint_name")?,
            kind: decode_string(client, row, "constraint_type")?,
            definition: decode_string(client, row, "definition")?,
        };
        relation_mut(relations, &key)?.constraints.push(constraint);
    }
    Ok(())
}

fn load_indexes(
    client: &mut VmPostgresCommandClient,
    relations: &mut BTreeMap<(String, String), SchemaRelation>,
) -> Result<(), String> {
    for row in client.query(INDEX_SQL, Vec::new())? {
        let key = relation_key(client, row)?;
        let index = SchemaIndex {
            name: decode_string(client, row, "index_name")?,
            definition: decode_string(client, row, "definition")?,
        };
        relation_mut(relations, &key)?.indexes.push(index);
    }
    Ok(())
}

fn load_enums(client: &mut VmPostgresCommandClient) -> Result<Vec<SchemaEnum>, String> {
    let mut enums = BTreeMap::<(String, String), Vec<String>>::new();
    for row in client.query(ENUM_SQL, Vec::new())? {
        let schema = decode_string(client, row, "type_schema")?;
        let name = decode_string(client, row, "type_name")?;
        let label = decode_string(client, row, "label")?;
        enums.entry((schema, name)).or_default().push(label);
    }
    Ok(enums
        .into_iter()
        .map(|((schema, name), labels)| SchemaEnum {
            schema,
            name,
            labels,
        })
        .collect())
}

fn relation_key(
    client: &mut VmPostgresCommandClient,
    row: VmPostgresRow,
) -> Result<(String, String), String> {
    Ok((
        decode_string(client, row, "table_schema")?,
        decode_string(client, row, "table_name")?,
    ))
}

fn relation_mut<'a>(
    relations: &'a mut BTreeMap<(String, String), SchemaRelation>,
    key: &(String, String),
) -> Result<&'a mut SchemaRelation, String> {
    relations.get_mut(key).ok_or_else(|| {
        format!(
            "Postgres catalog returned metadata for unknown relation `{}.{}`",
            key.0, key.1
        )
    })
}

fn decode_string(
    client: &mut VmPostgresCommandClient,
    row: VmPostgresRow,
    column: &str,
) -> Result<String, String> {
    match client.decode_dynamic(row, column)? {
        #[cfg(any(
            test,
            all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
        ))]
        VmPostgresDecodedValue::String(value) => Ok(value),
        #[cfg(any(
            test,
            all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
        ))]
        value => Err(decode_error(column, "string", &value)),
    }
}

fn decode_optional_string(
    client: &mut VmPostgresCommandClient,
    row: VmPostgresRow,
    column: &str,
) -> Result<Option<String>, String> {
    match client.decode_dynamic(row, column)? {
        #[cfg(any(
            test,
            all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
        ))]
        VmPostgresDecodedValue::Null => Ok(None),
        #[cfg(any(
            test,
            all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
        ))]
        VmPostgresDecodedValue::String(value) => Ok(Some(value)),
        #[cfg(any(
            test,
            all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
        ))]
        value => Err(decode_error(column, "nullable string", &value)),
    }
}

fn decode_int(
    client: &mut VmPostgresCommandClient,
    row: VmPostgresRow,
    column: &str,
) -> Result<i64, String> {
    match client.decode_dynamic(row, column)? {
        #[cfg(any(
            test,
            all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
        ))]
        VmPostgresDecodedValue::Int(value) => Ok(value),
        #[cfg(any(
            test,
            all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
        ))]
        value => Err(decode_error(column, "integer", &value)),
    }
}

fn decode_bool(
    client: &mut VmPostgresCommandClient,
    row: VmPostgresRow,
    column: &str,
) -> Result<bool, String> {
    match client.decode_dynamic(row, column)? {
        #[cfg(any(
            test,
            all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
        ))]
        VmPostgresDecodedValue::Bool(value) => Ok(value),
        #[cfg(any(
            test,
            all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
        ))]
        value => Err(decode_error(column, "boolean", &value)),
    }
}

#[cfg(any(
    test,
    all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
))]
fn decode_error(column: &str, expected: &str, value: &VmPostgresDecodedValue) -> String {
    format!(
        "Postgres schema column `{column}` expected {expected}, found {}",
        decoded_kind(value)
    )
}

#[cfg(any(
    test,
    all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
))]
fn decoded_kind(value: &VmPostgresDecodedValue) -> &'static str {
    match value {
        #[cfg(any(
            test,
            all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
        ))]
        VmPostgresDecodedValue::Null => "null",
        #[cfg(any(
            test,
            all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
        ))]
        VmPostgresDecodedValue::Int(_) => "integer",
        #[cfg(any(
            test,
            all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
        ))]
        VmPostgresDecodedValue::Bool(_) => "boolean",
        #[cfg(any(
            test,
            all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
        ))]
        VmPostgresDecodedValue::String(_) => "string",
        #[cfg(any(
            test,
            all(feature = "postgres-libpq", not(feature = "serve-runtime-bin"))
        ))]
        VmPostgresDecodedValue::Json(_) => "json",
    }
}

fn migration_snapshot_id(migrations: &[MigrationEngineInput]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"terlan.migration-snapshot.v1\0");
    for migration in migrations {
        hasher.update(migration.version.as_bytes());
        hasher.update([0]);
        hasher.update(migration.name.as_bytes());
        hasher.update([0]);
        hasher.update(migration.checksum.as_bytes());
        hasher.update([0]);
    }
    let digest = hasher.finalize();
    prefixed_digest(&digest)
}

fn temporary_snapshot_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("schema.snapshot.json");
    path.with_file_name(format!(".{file_name}.tmp-{}", std::process::id()))
}

#[cfg(test)]
#[path = "snapshot_test.rs"]
#[cfg(test)]
mod snapshot_test;
