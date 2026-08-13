use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{run_vm_sql_macro_validation, validate_database_evidence};

struct TestRepo {
    root: PathBuf,
}

impl TestRepo {
    fn new(name: &str) -> io::Result<Self> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "terlan-vm-sql-validation-{name}-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn write(&self, relative: &str, text: &str) -> io::Result<()> {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, text)
    }

    fn write_complete_fixture(&self) -> io::Result<()> {
        self.write(
            "crates/terlan/Cargo.toml",
            "[dependencies]\nsqlparser = \"=0.60.0\"\n",
        )?;
        for (path, source) in SOURCE_FIXTURES {
            self.write(path, source)?;
        }
        self.write(
            "Makefile",
            "vm-db-migration-command-check:\n\tcargo test run_db_migration_and_snapshot_lifecycle_against_docker_postgres\nvm-sql-macro-validation-check:\n\tcargo test --lib --features quality-tools sql\n\tcargo run -p terlan --bin terlan-quality --features quality-tools --quiet -- vm-sql-macro-validation\nvm-postgres-runtime-check: vm-sql-macro-validation-check\n",
        )?;
        let digest = format!("sha256:{}", "a".repeat(64));
        self.write(
            "target/quality/vm-db-migration-live-evidence.json",
            &serde_json::json!({
                "schema": "terlan.vm-db-migration-live-evidence.v1",
                "migration_snapshot_id": digest,
                "schema_fingerprint": digest,
                "replay_migration_snapshot_id": digest,
                "replay_schema_fingerprint": digest,
                "schema_drift_rejected": true,
                "live_sql_compiler_contract": true,
                "live_sql_parameter_order": true,
                "live_sql_typed_row_decode": true
            })
            .to_string(),
        )
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const SOURCE_FIXTURES: &[(&str, &str)] = &[
    (
        "crates/terlan/src/commands/db/live_test.rs",
        "prove_live_sql_contracts type_check_live_sql_contract type_check_syntax_module_output_with_database_schema bind_sql_parameters live_sql_typed_row_decode",
    ),
    (
        "crates/terlan/src/compiler/typeck/sql_forms/validation.rs",
        "sqlparser PostgreSqlDialect Parser::parse_sql",
    ),
    (
        "crates/terlan/src/compiler/typeck/sql_forms/classification.rs",
        "classify_statement statement_cardinality statement_transaction_requirement",
    ),
    (
        "crates/terlan/src/compiler/typeck/sql_forms/projection.rs",
        "statement_projection_fields SqlProjectionError",
    ),
    (
        "crates/terlan/src/compiler/typeck/sql_forms/database.rs",
        "statement_schema_projection SqlDatabaseProjectionColumn UnknownRelation UnknownColumn UnknownQualifier SelectItem::Wildcard",
    ),
    (
        "crates/terlan/src/database_schema.rs",
        "DatabaseSchemaSnapshot DatabaseColumnCodec for_schema_column resolve discover_for_source validate_integrity db.snapshot.ambiguous db.snapshot.corrupt",
    ),
    (
        "crates/terlan/src/runtime/native/postgres/row.rs",
        "DatabaseColumnCodec::resolve DecodedValue",
    ),
    (
        "crates/terlan/src/compiler/typeck/expression/sql.rs",
        "validate_sql_form_parameter_types validate_sql_form_row_type validate_sql_database_column_type Option",
    ),
    (
        "crates/terlan/src/compiler/typeck/expression/sql/abi.rs",
        "sql_scalar_abi_type_is_supported sql_row_decode_type_is_supported sql_row_scalar_type_is_supported",
    ),
    (
        "crates/terlan/src/compiler/typeck/mod.rs",
        "type_check_syntax_module_output_with_database_schema",
    ),
    (
        "crates/terlan/src/compiler/typeck/core_sql_lowering.rs",
        "CoreExpr::SqlQuery parameters",
    ),
];

#[test]
fn static_validation_writes_honest_deterministic_report() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_sql_macro_validation(repo.root()).expect("quality gate");

    assert_eq!(summary.parser_version, "0.60.0");
    assert_eq!(summary.validation_contract_fingerprint.len(), 64);
    assert_eq!(summary.diagnostic_count, 19);
    let report = fs::read_to_string(summary.report_path).expect("report");
    assert!(report.contains("terlan.vm-sql-macro-validation.v1"));
    assert!(report.contains("\"validationMode\": \"postgres-live\""));
    assert!(report.contains("\"complete\": true"));
    assert!(report.contains("\"liveTypedRowDecode\": true"));
    assert!(report.contains("\"snapshotBackedSelectValidation\": true"));
    assert!(report.contains("\"snapshotBackedExactNullability\": true"));
}

#[test]
fn live_validation_rejects_unproven_runtime_contract() {
    let repo = TestRepo::new("unproven-live-runtime").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let evidence_path = repo
        .root()
        .join("target/quality/vm-db-migration-live-evidence.json");
    let mut evidence: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&evidence_path).expect("read evidence"))
            .expect("parse evidence");
    evidence["live_sql_typed_row_decode"] = serde_json::Value::Bool(false);
    fs::write(evidence_path, evidence.to_string()).expect("rewrite evidence");

    let error = run_vm_sql_macro_validation(repo.root()).expect_err("must fail");
    assert!(error.contains("live SQL typed row decoding was not proven"));
}

#[test]
fn static_validation_rejects_missing_maintained_parser_anchor() {
    let repo = TestRepo::new("parser-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "crates/terlan/src/compiler/typeck/sql_forms/validation.rs",
        "PostgreSqlDialect Parser::parse_sql",
    )
    .expect("rewrite source");

    let error = run_vm_sql_macro_validation(repo.root()).expect_err("must fail");
    assert!(error.contains("missing SQL validation anchor `sqlparser`"));
}

#[test]
fn static_validation_rejects_missing_snapshot_schema_validation_anchor() {
    let repo = TestRepo::new("schema-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "crates/terlan/src/compiler/typeck/sql_forms/database.rs",
        "UnknownRelation UnknownColumn UnknownQualifier SelectItem::Wildcard",
    )
    .expect("rewrite source");

    let error = run_vm_sql_macro_validation(repo.root()).expect_err("must fail");
    assert!(error.contains("missing SQL validation anchor `statement_schema_projection`"));
}

#[test]
fn static_validation_rejects_missing_database_codec_anchor() {
    let repo = TestRepo::new("codec-anchor").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "crates/terlan/src/database_schema.rs",
        "DatabaseSchemaSnapshot discover_for_source validate_integrity db.snapshot.ambiguous db.snapshot.corrupt",
    )
    .expect("rewrite source");

    let error = run_vm_sql_macro_validation(repo.root()).expect_err("must fail");
    assert!(error.contains("missing SQL validation anchor `DatabaseColumnCodec`"));
}

#[test]
fn static_validation_rejects_gate_order_drift() {
    let repo = TestRepo::new("gate-order").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        "vm-sql-macro-validation-check:\n\tcargo test --lib --features quality-tools sql\n\tcargo run -p terlan --bin terlan-quality --features quality-tools --quiet -- vm-sql-macro-validation\nvm-postgres-runtime-check:\n",
    )
    .expect("rewrite Makefile");

    let error = run_vm_sql_macro_validation(repo.root()).expect_err("must fail");
    assert!(error.contains("vm-postgres-runtime-check: vm-sql-macro-validation-check"));
}

#[test]
fn static_validation_rejects_non_exact_parser_dependency() {
    let repo = TestRepo::new("parser-version").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "crates/terlan/Cargo.toml",
        "[dependencies]\nsqlparser = \"0.60.0\"\n",
    )
    .expect("rewrite manifest");

    let error = run_vm_sql_macro_validation(repo.root()).expect_err("must fail");
    assert!(error.contains("missing exact `sqlparser` dependency"));
}

#[test]
fn live_validation_requires_real_schema_and_migration_identities() {
    let diagnostics = validate_database_evidence("postgres-live", None, Some("sha256:bad"));

    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics
        .iter()
        .any(|item| item.contains("schema fingerprint")));
    assert!(diagnostics
        .iter()
        .any(|item| item.contains("migration snapshot id")));
}

#[test]
fn live_validation_accepts_lowercase_sha256_identities() {
    let digest = format!("sha256:{}", "a".repeat(64));

    assert!(validate_database_evidence("postgres-live", Some(&digest), Some(&digest)).is_empty());
}

#[test]
fn static_validation_rejects_database_identity_claims() {
    let digest = format!("sha256:{}", "a".repeat(64));
    let diagnostics = validate_database_evidence("compiler-static", Some(&digest), None);

    assert_eq!(
        diagnostics,
        ["compiler-static SQL validation must not claim database schema identities"]
    );
}
