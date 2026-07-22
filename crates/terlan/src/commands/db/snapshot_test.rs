use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn fixture_snapshot(migration_name: &str, nullable: bool) -> DatabaseSchemaSnapshot {
    let relations = vec![SchemaRelation {
        schema: "public".to_string(),
        name: "users".to_string(),
        kind: "BASE TABLE".to_string(),
        columns: vec![SchemaColumn {
            name: "id".to_string(),
            ordinal: 1,
            data_type: "bigint".to_string(),
            user_type_schema: "pg_catalog".to_string(),
            user_type_name: "int8".to_string(),
            nullable,
            default: None,
            identity: false,
            identity_generation: None,
            generated: false,
            generation_expression: None,
        }],
        constraints: vec![SchemaConstraint {
            name: "users_pkey".to_string(),
            kind: "p".to_string(),
            definition: "PRIMARY KEY (id)".to_string(),
        }],
        indexes: vec![SchemaIndex {
            name: "users_pkey".to_string(),
            definition: "CREATE UNIQUE INDEX users_pkey ON public.users USING btree (id)"
                .to_string(),
        }],
    }];
    let enums = vec![SchemaEnum {
        schema: "public".to_string(),
        name: "user_state".to_string(),
        labels: vec!["active".to_string(), "disabled".to_string()],
    }];
    let migrations = vec![MigrationEngineInput {
        version: "20260718090000".to_string(),
        name: migration_name.to_string(),
        up_sql: "CREATE TABLE users (id bigint PRIMARY KEY);".to_string(),
        up_start_line: 2,
        down_sql: None,
        down_start_line: None,
        checksum: "1".repeat(64),
    }];

    DatabaseSchemaSnapshot {
        schema: DATABASE_SCHEMA_SNAPSHOT_SCHEMA.to_string(),
        database_product: "PostgreSQL".to_string(),
        migration_snapshot_id: migration_snapshot_id(&migrations),
        schema_fingerprint: schema_fingerprint(&relations, &enums).expect("fingerprint"),
        relations,
        enums,
    }
}

fn temp_snapshot_path(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "terlan-schema-snapshot-{name}-{}-{unique}.json",
        std::process::id()
    ))
}

#[test]
fn schema_and_migration_fingerprints_are_deterministic_and_sensitive() {
    let baseline = fixture_snapshot("create_users", false);
    assert_eq!(baseline, fixture_snapshot("create_users", false));
    assert_ne!(
        baseline.schema_fingerprint,
        fixture_snapshot("create_users", true).schema_fingerprint
    );
    assert_ne!(
        baseline.migration_snapshot_id,
        fixture_snapshot("replace_users", false).migration_snapshot_id
    );
    assert!(baseline.schema_fingerprint.starts_with("sha256:"));
    assert_eq!(baseline.schema_fingerprint.len(), 71);
}

#[test]
fn snapshot_round_trip_and_drift_check_are_strict() {
    let path = temp_snapshot_path("round-trip");
    let baseline = fixture_snapshot("create_users", false);
    write_schema_snapshot(&path, &baseline).expect("write snapshot");
    check_schema_snapshot(&path, &baseline).expect("matching snapshot");

    let error = check_schema_snapshot(&path, &fixture_snapshot("create_users", true))
        .expect_err("nullability drift must fail");
    assert!(error.contains("error[db.schema.dirty]"));
    assert!(error.contains("migration identity"));
    assert!(!error.contains("CREATE TABLE"));

    let _ = fs::remove_file(path);
}

#[test]
fn snapshot_check_distinguishes_stale_migration_identity_from_dirty_schema() {
    let path = temp_snapshot_path("stale-migrations");
    let baseline = fixture_snapshot("create_users", false);
    write_schema_snapshot(&path, &baseline).expect("write snapshot");

    let error = check_schema_snapshot(&path, &fixture_snapshot("replace_users", false))
        .expect_err("changed migration identity must fail");

    assert!(error.contains("error[db.snapshot.drift]"));
    assert!(error.contains("is stale"));
    assert!(!error.contains("CREATE TABLE"));
    let _ = fs::remove_file(path);
}

#[test]
fn snapshot_rejects_corruption_and_forged_fingerprints() {
    let malformed_path = temp_snapshot_path("malformed");
    fs::write(&malformed_path, "{not-json\n").expect("write malformed snapshot");
    let error = check_schema_snapshot(&malformed_path, &fixture_snapshot("create_users", false))
        .expect_err("malformed snapshot must fail");
    assert!(error.contains("error[db.snapshot.corrupt]"));
    assert!(error.contains("malformed JSON"));
    let _ = fs::remove_file(&malformed_path);

    let forged_path = temp_snapshot_path("forged");
    let mut forged = fixture_snapshot("create_users", false);
    forged.schema_fingerprint = format!("sha256:{}", "0".repeat(64));
    let text = serde_json::to_string_pretty(&forged).expect("serialize forged snapshot");
    fs::write(&forged_path, text).expect("write forged snapshot");
    let error = check_schema_snapshot(&forged_path, &fixture_snapshot("create_users", false))
        .expect_err("forged fingerprint must fail");
    assert!(error.contains("error[db.snapshot.corrupt]"));
    assert!(error.contains("stored schema fingerprint"));
    let _ = fs::remove_file(forged_path);
}

#[test]
fn snapshot_rejects_unsupported_schema_and_database_product_contracts() {
    let schema_path = temp_snapshot_path("unsupported-schema");
    let mut unsupported_schema = fixture_snapshot("create_users", false);
    unsupported_schema.schema = "terlan.db-schema-snapshot.v2".to_string();
    fs::write(
        &schema_path,
        serde_json::to_string_pretty(&unsupported_schema).expect("serialize snapshot"),
    )
    .expect("write unsupported schema snapshot");
    let error = check_schema_snapshot(&schema_path, &fixture_snapshot("create_users", false))
        .expect_err("unsupported schema version must fail");
    assert!(error.contains("error[db.snapshot.unsupported_contract]"));
    assert!(error.contains("schema version"));
    let _ = fs::remove_file(schema_path);

    let product_path = temp_snapshot_path("unsupported-product");
    let mut unsupported_product = fixture_snapshot("create_users", false);
    unsupported_product.database_product = "SQLite".to_string();
    fs::write(
        &product_path,
        serde_json::to_string_pretty(&unsupported_product).expect("serialize snapshot"),
    )
    .expect("write unsupported product snapshot");
    let error = check_schema_snapshot(&product_path, &fixture_snapshot("create_users", false))
        .expect_err("unsupported database product must fail");
    assert!(error.contains("error[db.snapshot.unsupported_contract]"));
    assert!(error.contains("database product"));
    let _ = fs::remove_file(product_path);
}

#[test]
fn default_snapshot_lives_next_to_migrations_directory() {
    assert_eq!(
        default_snapshot_path(Path::new("db/migrations")),
        PathBuf::from("db/schema.snapshot.json")
    );
    assert_eq!(
        default_snapshot_path(Path::new("migrations")),
        PathBuf::from("schema.snapshot.json")
    );
}
