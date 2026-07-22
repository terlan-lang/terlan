use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-db-migration-report.json";
const FIXTURE_ROOT: &str = "crates/terlan/src/commands/db/testdata";
const MIGRATION_FIXTURES: &[(&str, &str)] = &[
    ("20260619123000_create_live_users.sql", "committed"),
    (
        "20260619123100_add_live_user_email.sql",
        "committed-after-lock-release",
    ),
    ("20260619123200_fail_atomically.sql", "rolled-back"),
];

const SOURCE_CONTRACTS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/commands/db/execution.rs",
        &[
            "pg_try_advisory_xact_lock",
            "load_applied_migration_history_transaction",
            "finish_transaction(transaction, false)",
            "batch_execute_transaction",
            "migration_failed_message",
            "error[db.migration.failed]",
            "error[db.migration.lock_conflict]",
            "error[db.migration.history_divergent]",
        ],
    ),
    (
        "crates/terlan/src/commands/db/live_test.rs",
        &[
            "CREATE_LIVE_USERS_SQL",
            "FAIL_ATOMICALLY_SQL",
            "acquire_test_migration_lock",
            "VmPostgresDecodedValue::Null",
            "run_schema_snapshot_lifecycle",
        ],
    ),
    (
        "crates/terlan/src/commands/db/snapshot.rs",
        &[
            "schema_fingerprint",
            "migration_snapshot_id",
            "dirty_schema_message",
            "schema_snapshot_drift_message",
            "error[db.schema.dirty]",
            "error[db.snapshot.drift]",
        ],
    ),
    (
        "crates/terlan/src/database_schema.rs",
        &[
            "discover_for_source",
            "snapshot_corrupt_message",
            "unsupported_snapshot_contract_message",
            "error[db.snapshot.corrupt]",
            "error[db.snapshot.unsupported_contract]",
        ],
    ),
    (
        "crates/terlan/src/commands/db/migration.rs",
        &[
            "duplicate_migration_id_message",
            "error[db.migration.duplicate_id]",
        ],
    ),
    (
        "crates/terlan/src/commands/db/status.rs",
        &[
            "MigrationStatusState::ChecksumMismatch",
            "MigrationStatusState::NameMismatch",
            "MigrationStatusState::OutOfOrder",
            "migration_matches_applied",
            "migration_missing_file_message",
            "migration_checksum_mismatch_message",
            "migration_name_mismatch_message",
            "migration_out_of_order_message",
            "applied_at: Option<String>",
            "AT TIME ZONE 'UTC'",
            "error[db.migration.file_missing]",
            "error[db.migration.checksum_mismatch]",
            "error[db.migration.name_mismatch]",
            "error[db.migration.out_of_order]",
        ],
    ),
    (
        "crates/terlan/src/commands/db/mod.rs",
        &[
            "validate_development_database_config",
            "prepare_local_database_dependencies",
            "is_local_database_host",
            "protected_transport_option",
            "requires --confirm",
            "execute_migration_request",
        ],
    ),
];

const DIAGNOSTIC_COVERAGE: &[&str] = &[
    "db.migration.lock_conflict",
    "db.migration.lock_protocol",
    "db.migration.history_divergent",
    "db.migration.duplicate_id",
    "db.migration.failed",
    "db.migration.file_missing",
    "db.migration.checksum_mismatch",
    "db.migration.name_mismatch",
    "db.migration.out_of_order",
    "db.schema.dirty",
    "db.snapshot.corrupt",
    "db.snapshot.drift",
    "db.snapshot.unsupported_contract",
    "destructive-rebuild-rejected",
    "destructive-confirmation-required",
    "destructive-protected-transport-rejected",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmDbMigrationCommandSummary {
    pub migration_count: usize,
    pub diagnostic_count: usize,
    pub contract_fingerprint: String,
    pub report_path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MigrationFixtureEvidence {
    id: String,
    name: String,
    checksum: String,
    expected_outcome: &'static str,
}

/// Validates VM migration ownership and writes deterministic release evidence.
pub fn run_vm_db_migration_command(root: &Path) -> QualityResult<VmDbMigrationCommandSummary> {
    let mut diagnostics = Vec::new();
    let mut input_digests = BTreeMap::new();
    let mut contract_hasher = Sha256::new();

    for (relative, anchors) in SOURCE_CONTRACTS {
        let source = read(root, relative)?;
        record_input(relative, &source, &mut input_digests, &mut contract_hasher);
        for anchor in *anchors {
            if !source.contains(anchor) {
                diagnostics.push(format!(
                    "{relative}: missing VM database migration anchor `{anchor}`"
                ));
            }
        }
    }

    let fixtures = load_migration_fixtures(
        root,
        &mut input_digests,
        &mut contract_hasher,
        &mut diagnostics,
    )?;
    diagnostics.extend(validate_make_ownership(root)?);
    if !diagnostics.is_empty() {
        return Err(render_failure(&diagnostics));
    }

    let contract_fingerprint = hex_digest(contract_hasher.finalize().as_slice());
    let migration_evidence = fixtures
        .iter()
        .map(|fixture| {
            json!({
                "id": fixture.id,
                "name": fixture.name,
                "checksum_sha256": fixture.checksum,
                "expected_outcome": fixture.expected_outcome,
            })
        })
        .collect::<Vec<_>>();
    let report = json!({
        "schema": "terlan.vm-db-migration-command.v1",
        "gate_id": "vm-db-migration-command-check",
        "input_digests": input_digests,
        "tool_versions": {"terlan": env!("CARGO_PKG_VERSION")},
        "environment": {
            "default_gate": "static-and-unit",
            "live_database_gate": "docker-required",
            "credentials_reported": false,
            "sql_text_reported": false,
        },
        "diagnostics": {
            "coverage": DIAGNOSTIC_COVERAGE,
            "failures": [],
        },
        "coverage_deltas": {"status": "contract-evidence-only"},
        "benchmark_data": null,
        "support_bundle_references": [],
        "decision": "pass",
        "release_blocking_rationale": "VM migration lock, atomicity, safety, and snapshot contracts are present",
        "generated_at": null,
        "artifact_evidence": {
            "contract_fingerprint_sha256": contract_fingerprint,
            "migration_evidence": migration_evidence,
            "schema_fingerprint": null,
            "schema_fingerprint_status": "docker-live-gate-required",
            "lock_behavior": {
                "scope": "transaction",
                "acquisition": "nonblocking",
                "conflict_outcome": "db.migration.lock_conflict",
                "release": ["commit", "rollback", "connection-close"],
                "history_revalidated_after_lock": true,
            },
            "command_outcomes": [
                "committed",
                "committed-after-lock-release",
                "rolled-back",
                "lock-conflict",
                "history-divergent",
                "duplicate-id",
                "migration-failed",
                "file-missing",
                "checksum-mismatch",
                "name-mismatch",
                "out-of-order",
                "dirty-schema",
                "snapshot-corrupt",
                "snapshot-drift",
                "snapshot-unsupported-contract",
            ],
            "rebuild_safety_decision": {
                "development_flag_required": true,
                "confirmation_flag_required": true,
                "local_database_required": true,
                "protected_transport_rejected": true,
                "decision": "source-and-adversarial-test-enforced",
            },
            "applied_at_metadata": {
                "storage": "postgres-timestamptz-default-now",
                "read_format": "rfc3339-utc-microseconds",
                "status_propagated": true,
                "migration_identity_member": false,
                "wall_clock_values_reported": false,
            },
            "sql_macro_snapshot_compatibility": {
                "ordering": "vm-db-migration-command-check-before-vm-sql-macro-validation-check",
                "live_schema_fingerprint_required": true,
            },
            "generation_timestamp_policy": "omitted-for-determinism",
            "stable_ordering_policy": "serde-json-map-and-btree-input-order",
            "path_redaction_policy": "repository-relative-inputs-only",
            "compatibility_policy": "exact-schema-v1",
        },
    });
    validate_report(&report)?;
    let report_path = write_report(root, &report)?;

    Ok(VmDbMigrationCommandSummary {
        migration_count: fixtures.len(),
        diagnostic_count: DIAGNOSTIC_COVERAGE.len(),
        contract_fingerprint,
        report_path,
    })
}

fn load_migration_fixtures(
    root: &Path,
    input_digests: &mut BTreeMap<String, String>,
    contract_hasher: &mut Sha256,
    diagnostics: &mut Vec<String>,
) -> QualityResult<Vec<MigrationFixtureEvidence>> {
    let mut fixtures = Vec::new();
    for (filename, expected_outcome) in MIGRATION_FIXTURES {
        let relative = format!("{FIXTURE_ROOT}/{filename}");
        let source = read(root, &relative)?;
        record_input(&relative, &source, input_digests, contract_hasher);
        match parse_migration_filename(filename) {
            Ok((id, name)) => fixtures.push(MigrationFixtureEvidence {
                id,
                name,
                checksum: sha256(source.as_bytes()),
                expected_outcome,
            }),
            Err(message) => diagnostics.push(message),
        }
        if !source.contains("-- +terlan Up") {
            diagnostics.push(format!(
                "{relative}: missing canonical `-- +terlan Up` marker"
            ));
        }
    }
    Ok(fixtures)
}

fn parse_migration_filename(filename: &str) -> Result<(String, String), String> {
    let stem = filename
        .strip_suffix(".sql")
        .ok_or_else(|| format!("{filename}: migration fixture must use `.sql`"))?;
    let (id, name) = stem
        .split_once('_')
        .ok_or_else(|| format!("{filename}: migration fixture must contain an id and name"))?;
    if id.len() != 14 || !id.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!(
            "{filename}: migration fixture id must be a 14-digit timestamp"
        ));
    }
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        return Err(format!(
            "{filename}: migration fixture name must use lowercase snake case"
        ));
    }
    Ok((id.to_string(), name.to_string()))
}

fn validate_make_ownership(root: &Path) -> QualityResult<Vec<String>> {
    let makefile = read(root, "Makefile")?;
    let required = [
        "vm-db-migration-command-check: vm-dev-dependency-orchestration-check db-command-check",
        "terlan-quality --quiet -- vm-db-migration-command",
        "test -s target/quality/vm-db-migration-report.json",
        "vm-sql-macro-validation-check: vm-db-migration-command-check",
    ];
    Ok(required
        .iter()
        .filter(|anchor| !makefile.contains(**anchor))
        .map(|anchor| format!("Makefile: missing VM database migration gate anchor `{anchor}`"))
        .collect())
}

fn validate_report(report: &Value) -> QualityResult<()> {
    let text = serde_json::to_string(report)
        .map_err(|error| format!("{REPORT_PATH}: failed to validate report: {error}"))?;
    let forbidden = ["postgres://", "postgresql://", "password", "CREATE TABLE"];
    let leaked = forbidden
        .iter()
        .filter(|term| text.contains(**term))
        .copied()
        .collect::<Vec<_>>();
    if leaked.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{REPORT_PATH}: report leaks forbidden database material: {}",
            leaked.join(", ")
        ))
    }
}

fn record_input(
    relative: &str,
    source: &str,
    input_digests: &mut BTreeMap<String, String>,
    contract_hasher: &mut Sha256,
) {
    let digest = sha256(source.as_bytes());
    input_digests.insert(relative.to_string(), format!("sha256:{digest}"));
    contract_hasher.update(relative.as_bytes());
    contract_hasher.update([0]);
    contract_hasher.update(source.as_bytes());
}

fn read(root: &Path, relative: &str) -> QualityResult<String> {
    fs::read_to_string(root.join(relative)).map_err(|error| {
        format!("{relative}: failed to read VM database migration evidence: {error}")
    })
}

fn sha256(bytes: &[u8]) -> String {
    hex_digest(Sha256::digest(bytes).as_slice())
}

fn hex_digest(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn write_report(root: &Path, report: &Value) -> QualityResult<PathBuf> {
    let path = root.join(REPORT_PATH);
    let parent = path
        .parent()
        .ok_or_else(|| format!("{REPORT_PATH}: report path has no parent"))?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "{}: failed to create report directory: {error}",
            parent.display()
        )
    })?;
    let text = serde_json::to_string_pretty(report)
        .map_err(|error| format!("{REPORT_PATH}: failed to serialize report: {error}"))?;
    let temporary = parent.join(format!(
        ".vm-db-migration-report-{}.tmp",
        std::process::id()
    ));
    fs::write(&temporary, format!("{text}\n"))
        .map_err(|error| format!("{REPORT_PATH}: failed to write temporary report: {error}"))?;
    fs::rename(&temporary, &path).map_err(|error| {
        let _ = fs::remove_file(&temporary);
        format!("{REPORT_PATH}: failed to publish report atomically: {error}")
    })?;
    Ok(path)
}

fn render_failure(diagnostics: &[String]) -> String {
    format!(
        "[vm-db-migration-command] failures:\n  - {}",
        diagnostics.join("\n  - ")
    )
}

#[cfg(test)]
#[path = "vm_db_migration_command_test.rs"]
mod vm_db_migration_command_test;
