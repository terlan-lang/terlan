use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::terlan_quality::QualityResult;

const CARGO_MANIFEST: &str = "crates/terlan/Cargo.toml";
const REPORT_PATH: &str = "target/quality/vm-sql-macro-validation-report.json";
const LIVE_EVIDENCE_PATH: &str = "target/quality/vm-db-migration-live-evidence.json";
const STATIC_VALIDATION_MODE: &str = "compiler-static";
const LIVE_VALIDATION_MODE: &str = "postgres-live";

const SOURCE_CONTRACTS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/commands/db/live_test.rs",
        &[
            "prove_live_sql_contracts",
            "type_check_live_sql_contract",
            "type_check_syntax_module_output_with_database_schema",
            "bind_sql_parameters",
            "live_sql_typed_row_decode",
        ],
    ),
    (
        "crates/terlan/src/compiler/typeck/sql_forms/validation.rs",
        &["sqlparser", "PostgreSqlDialect", "Parser::parse_sql"],
    ),
    (
        "crates/terlan/src/compiler/typeck/sql_forms/classification.rs",
        &[
            "classify_statement",
            "statement_cardinality",
            "statement_transaction_requirement",
        ],
    ),
    (
        "crates/terlan/src/compiler/typeck/sql_forms/projection.rs",
        &["statement_projection_fields", "SqlProjectionError"],
    ),
    (
        "crates/terlan/src/compiler/typeck/sql_forms/database.rs",
        &[
            "statement_schema_projection",
            "SqlDatabaseProjectionColumn",
            "UnknownRelation",
            "UnknownColumn",
            "UnknownQualifier",
            "SelectItem::Wildcard",
        ],
    ),
    (
        "crates/terlan/src/database_schema.rs",
        &[
            "DatabaseSchemaSnapshot",
            "DatabaseColumnCodec",
            "for_schema_column",
            "resolve",
            "discover_for_source",
            "validate_integrity",
            "db.snapshot.ambiguous",
            "db.snapshot.corrupt",
        ],
    ),
    (
        "crates/terlan/src/runtime/native/postgres/row.rs",
        &["DatabaseColumnCodec::resolve", "DecodedValue"],
    ),
    (
        "crates/terlan/src/compiler/typeck/expression/sql.rs",
        &[
            "validate_sql_form_parameter_types",
            "validate_sql_form_row_type",
            "validate_sql_database_column_type",
            "Option",
        ],
    ),
    (
        "crates/terlan/src/compiler/typeck/expression/sql/abi.rs",
        &[
            "sql_scalar_abi_type_is_supported",
            "sql_row_decode_type_is_supported",
            "sql_row_scalar_type_is_supported",
        ],
    ),
    (
        "crates/terlan/src/compiler/typeck/mod.rs",
        &["type_check_syntax_module_output_with_database_schema"],
    ),
    (
        "crates/terlan/src/compiler/typeck/core_sql_lowering.rs",
        &["CoreExpr::SqlQuery", "parameters"],
    ),
];

const DIAGNOSTIC_COVERAGE: &[&str] = &[
    "malformed-sql",
    "empty-or-comment-only-sql",
    "multiple-statements",
    "explicit-postgres-placeholders",
    "injection-shaped-interpolation",
    "duplicate-projection-names",
    "unknown-schema-relation",
    "unknown-schema-column",
    "unknown-schema-qualifier",
    "ambiguous-schema-snapshot",
    "corrupt-schema-snapshot",
    "stale-schema-cache",
    "non-bindable-parameter-types",
    "row-projection-arity-mismatch",
    "non-decodable-row-fields",
    "database-column-codec-mismatch",
    "database-column-nullability-mismatch",
    "unsupported-database-column-codec",
    "vm-owned-transaction-control",
];

const INFERRED_CARDINALITY: &[&str] = &["optional_one", "many_rows", "affected_rows", "ambiguous"];

const INFERRED_ROW_SHAPES: &[&str] = &[
    "visible-named-row",
    "non-empty-scalar-tuple",
    "transparent-scalar-tuple-alias",
    "nullable-scalar-field",
];

/// Summary produced by the SQL macro validation evidence gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmSqlMacroValidationSummary {
    pub parser_version: String,
    pub validation_contract_fingerprint: String,
    pub diagnostic_count: usize,
    pub report_path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct LiveSqlEvidence {
    schema: String,
    migration_snapshot_id: String,
    schema_fingerprint: String,
    replay_migration_snapshot_id: String,
    replay_schema_fingerprint: String,
    schema_drift_rejected: bool,
    live_sql_compiler_contract: bool,
    live_sql_parameter_order: bool,
    live_sql_typed_row_decode: bool,
}

/// Validates maintained-parser ownership and writes deterministic live SQL evidence.
pub fn run_vm_sql_macro_validation(root: &Path) -> QualityResult<VmSqlMacroValidationSummary> {
    let manifest = read(root, CARGO_MANIFEST)?;
    let parser_version = dependency_version(&manifest, "sqlparser")?;
    let mut diagnostics = Vec::new();
    if parser_version != "0.60.0" {
        diagnostics.push(format!(
            "{CARGO_MANIFEST}: sqlparser version drifted from reviewed 0.60.0 to {parser_version}"
        ));
    }

    let mut hasher = Sha256::new();
    hasher.update(format!("sqlparser={parser_version}\n"));
    for (path, anchors) in SOURCE_CONTRACTS {
        let source = read(root, path)?;
        hasher.update(path.as_bytes());
        hasher.update([0]);
        hasher.update(source.as_bytes());
        for anchor in *anchors {
            if !source.contains(anchor) {
                diagnostics.push(format!("{path}: missing SQL validation anchor `{anchor}`"));
            }
        }
    }
    diagnostics.extend(validate_make_ownership(root)?);
    let live = load_live_sql_evidence(root)?;
    diagnostics.extend(validate_live_sql_evidence(&live));
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    let validation_contract_fingerprint = hex_digest(hasher.finalize().as_slice());
    let report = json!({
        "schema": "terlan.vm-sql-macro-validation.v1",
        "parserCrate": {
            "name": "sqlparser",
            "version": parser_version,
            "dialect": "PostgreSQL"
        },
        "validationMode": LIVE_VALIDATION_MODE,
        "validationContractFingerprintSha256": validation_contract_fingerprint,
        "schemaFingerprint": live.schema_fingerprint,
        "migrationSnapshotId": live.migration_snapshot_id,
        "inferredCardinality": INFERRED_CARDINALITY,
        "inferredRowShape": INFERRED_ROW_SHAPES,
        "diagnosticCoverage": DIAGNOSTIC_COVERAGE,
        "databaseAuthoritativeValidation": {
            "complete": true,
            "requiredMode": LIVE_VALIDATION_MODE,
            "snapshotBackedSelectValidation": true,
            "snapshotBackedColumnCodecs": ["binary", "bool", "int", "json"],
            "snapshotBackedExactNullability": true,
            "snapshotIntegrityRequired": true,
            "supportedScope": "single-physical-relation-select",
            "liveCompilerContract": live.live_sql_compiler_contract,
            "liveParameterOrder": live.live_sql_parameter_order,
            "liveTypedRowDecode": live.live_sql_typed_row_decode,
            "schemaDriftRejected": live.schema_drift_rejected
        }
    });
    let report_path = write_report(root, &report)?;

    Ok(VmSqlMacroValidationSummary {
        parser_version,
        validation_contract_fingerprint,
        diagnostic_count: DIAGNOSTIC_COVERAGE.len(),
        report_path,
    })
}

fn load_live_sql_evidence(root: &Path) -> QualityResult<LiveSqlEvidence> {
    let text = read(root, LIVE_EVIDENCE_PATH)?;
    serde_json::from_str(&text)
        .map_err(|error| format!("{LIVE_EVIDENCE_PATH}: invalid live SQL evidence: {error}"))
}

fn validate_live_sql_evidence(evidence: &LiveSqlEvidence) -> Vec<String> {
    let mut diagnostics = validate_database_evidence(
        LIVE_VALIDATION_MODE,
        Some(&evidence.schema_fingerprint),
        Some(&evidence.migration_snapshot_id),
    );
    if evidence.schema != "terlan.vm-db-migration-live-evidence.v1" {
        diagnostics.push(format!(
            "{LIVE_EVIDENCE_PATH}: unsupported evidence schema `{}`",
            evidence.schema
        ));
    }
    if evidence.schema_fingerprint != evidence.replay_schema_fingerprint {
        diagnostics.push(format!(
            "{LIVE_EVIDENCE_PATH}: replay schema fingerprint does not match live schema"
        ));
    }
    if evidence.migration_snapshot_id != evidence.replay_migration_snapshot_id {
        diagnostics.push(format!(
            "{LIVE_EVIDENCE_PATH}: replay migration identity does not match live migrations"
        ));
    }
    for (label, passed) in [
        ("schema drift rejection", evidence.schema_drift_rejected),
        (
            "compiler schema contract",
            evidence.live_sql_compiler_contract,
        ),
        ("parameter ordering", evidence.live_sql_parameter_order),
        ("typed row decoding", evidence.live_sql_typed_row_decode),
    ] {
        if !passed {
            diagnostics.push(format!(
                "{LIVE_EVIDENCE_PATH}: live SQL {label} was not proven"
            ));
        }
    }
    diagnostics
}

fn dependency_version(manifest: &str, dependency: &str) -> QualityResult<String> {
    manifest
        .lines()
        .map(str::trim)
        .find_map(|line| {
            let value = line.strip_prefix(dependency)?.trim_start();
            let value = value.strip_prefix('=')?.trim();
            value
                .strip_prefix('"')
                .and_then(|value| value.strip_suffix('"'))
                .and_then(|value| value.strip_prefix('='))
                .map(str::to_string)
        })
        .ok_or_else(|| format!("{CARGO_MANIFEST}: missing exact `{dependency}` dependency"))
}

fn validate_database_evidence(
    mode: &str,
    schema_fingerprint: Option<&str>,
    migration_snapshot_id: Option<&str>,
) -> Vec<String> {
    let mut diagnostics = Vec::new();
    match mode {
        STATIC_VALIDATION_MODE => {
            if schema_fingerprint.is_some() || migration_snapshot_id.is_some() {
                diagnostics.push(
                    "compiler-static SQL validation must not claim database schema identities"
                        .to_string(),
                );
            }
        }
        LIVE_VALIDATION_MODE => {
            validate_sha256_identity(&mut diagnostics, "schema fingerprint", schema_fingerprint);
            validate_sha256_identity(
                &mut diagnostics,
                "migration snapshot id",
                migration_snapshot_id,
            );
        }
        _ => diagnostics.push(format!("unknown SQL validation mode `{mode}`")),
    }
    diagnostics
}

fn validate_sha256_identity(diagnostics: &mut Vec<String>, label: &str, identity: Option<&str>) {
    let valid = identity.is_some_and(|value| {
        value.strip_prefix("sha256:").is_some_and(|digest| {
            digest.len() == 64
                && digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
    });
    if !valid {
        diagnostics.push(format!(
            "postgres-live SQL validation requires a valid {label}"
        ));
    }
}

fn validate_make_ownership(root: &Path) -> QualityResult<Vec<String>> {
    let makefile = read(root, "Makefile")?;
    let required = [
        "vm-sql-macro-validation-check:",
        "--lib --features quality-tools sql",
        "run_db_migration_and_snapshot_lifecycle_against_docker_postgres",
        "vm-postgres-runtime-check: vm-sql-macro-validation-check",
        "--features quality-tools --quiet -- vm-sql-macro-validation",
    ];
    Ok(required
        .iter()
        .filter(|anchor| !makefile.contains(**anchor))
        .map(|anchor| format!("Makefile: missing SQL validation gate anchor `{anchor}`"))
        .collect())
}

fn read(root: &Path, relative: &str) -> QualityResult<String> {
    fs::read_to_string(root.join(relative))
        .map_err(|error| format!("{relative}: failed to read SQL validation evidence: {error}"))
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_report(root: &Path, report: &serde_json::Value) -> QualityResult<PathBuf> {
    let path = root.join(REPORT_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "{}: failed to create report directory: {error}",
                parent.display()
            )
        })?;
    }
    let text = serde_json::to_string_pretty(report)
        .map_err(|error| format!("{REPORT_PATH}: failed to serialize report: {error}"))?;
    fs::write(&path, format!("{text}\n"))
        .map_err(|error| format!("{REPORT_PATH}: failed to write report: {error}"))?;
    Ok(path)
}

fn render_failure(diagnostics: &[String]) -> String {
    format!(
        "[vm-sql-macro-validation] failures:\n  - {}",
        diagnostics.join("\n  - ")
    )
}

#[cfg(test)]
#[path = "vm_sql_macro_validation_test.rs"]
#[cfg(test)]
mod vm_sql_macro_validation_test;
