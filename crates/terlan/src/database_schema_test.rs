use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

fn fixture_snapshot() -> DatabaseSchemaSnapshot {
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
            nullable: false,
            default: None,
            identity: false,
            identity_generation: None,
            generated: false,
            generation_expression: None,
        }],
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

fn temp_directory(name: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "terlan-database-schema-{name}-{}-{unique}",
        std::process::id()
    ))
}

#[test]
fn discovery_loads_the_nearest_verified_project_snapshot() {
    let root = temp_directory("discover");
    let source = root.join("src/admin/page.terl");
    let snapshot_path = root.join("db/schema.snapshot.json");
    fs::create_dir_all(source.parent().expect("source parent")).expect("create source directory");
    fs::create_dir_all(snapshot_path.parent().expect("snapshot parent"))
        .expect("create snapshot directory");
    fs::write(
        &snapshot_path,
        serde_json::to_string_pretty(&fixture_snapshot()).expect("serialize snapshot"),
    )
    .expect("write snapshot");

    let discovered = DatabaseSchemaSnapshot::discover_for_source(&source)
        .expect("discover snapshot")
        .expect("snapshot");
    assert!(discovered.relation("public", "users").is_some());

    let _ = fs::remove_dir_all(root);
}

#[test]
fn discovery_rejects_ambiguous_and_forged_snapshots() {
    let root = temp_directory("adversarial");
    let source = root.join("src/page.terl");
    fs::create_dir_all(source.parent().expect("source parent")).expect("create source directory");
    fs::create_dir_all(root.join("db")).expect("create db directory");
    let valid = serde_json::to_string_pretty(&fixture_snapshot()).expect("serialize snapshot");
    fs::write(root.join("db/schema.snapshot.json"), &valid).expect("write db snapshot");
    fs::write(root.join("schema.snapshot.json"), &valid).expect("write root snapshot");
    let error = DatabaseSchemaSnapshot::discover_for_source(&source)
        .expect_err("ambiguous snapshots must fail");
    assert!(error.contains("error[db.snapshot.ambiguous]"));

    fs::remove_file(root.join("schema.snapshot.json")).expect("remove duplicate snapshot");
    let mut forged = fixture_snapshot();
    forged.schema_fingerprint = format!("sha256:{}", "0".repeat(64));
    fs::write(
        root.join("db/schema.snapshot.json"),
        serde_json::to_string_pretty(&forged).expect("serialize forged snapshot"),
    )
    .expect("write forged snapshot");
    let error = DatabaseSchemaSnapshot::discover_for_source(&source)
        .expect_err("forged snapshot must fail");
    assert!(error.contains("error[db.snapshot.corrupt]"));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn database_column_codecs_are_explicit_and_fail_closed() {
    let mut column = fixture_snapshot().relations.remove(0).columns.remove(0);
    for (database_type, expected) in [
        ("int8", Some(DatabaseColumnCodec::Int)),
        ("bool", Some(DatabaseColumnCodec::Bool)),
        ("text", Some(DatabaseColumnCodec::Binary)),
        ("jsonb", Some(DatabaseColumnCodec::Json)),
        ("numeric", None),
    ] {
        column.user_type_name = database_type.to_string();
        assert_eq!(DatabaseColumnCodec::for_schema_column(&column), expected);
    }
    for (oid, expected) in [
        (20, Some(DatabaseColumnCodec::Int)),
        (16, Some(DatabaseColumnCodec::Bool)),
        (25, Some(DatabaseColumnCodec::Binary)),
        (3802, Some(DatabaseColumnCodec::Json)),
        (1700, None),
    ] {
        assert_eq!(DatabaseColumnCodec::resolve(None, Some(oid)), expected);
    }

    column.user_type_schema = "application".to_string();
    column.user_type_name = "user_id".to_string();
    assert_eq!(DatabaseColumnCodec::for_schema_column(&column), None);
    assert_eq!(column.qualified_database_type(), "application.user_id");
}
