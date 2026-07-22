use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-persistent-actor-schema-report.json";
const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REQUIRED_STATE_SCHEMA_ANCHORS: &[&str] = &[
    "pub(crate) struct VmDistributedStateVersion",
    "pub(crate) struct VmDistributedStateEntry",
    "pub(crate) struct VmDistributedStateConflict",
    "pub(crate) enum VmDistributedStateWriteOutcome",
    "sequence: u64",
    "node_id: String",
    "export_snapshot",
    "import_snapshot",
    "snapshot version must be valid",
    "snapshot contains duplicate state scope",
];

const REQUIRED_STORAGE_SCHEMA_ANCHORS: &[&str] = &[
    "pub(crate) struct VmDistributedStorageSnapshot",
    "checkpoint_id: String",
    "sequence: u64",
    "checksum: u32",
    "expected_checksum",
    "with_checksum",
    "StaleSnapshot",
    "ChecksumMismatch",
    "requires_recovery",
    "recovery_action",
];

const REQUIRED_MIGRATION_SCHEMA_ANCHORS: &[&str] = &[
    "pub(crate) enum VmMigrationPhase",
    "pub(crate) struct VmMigrationIntent",
    "pub(crate) enum VmMigrationOutcome",
    "completed_migration_outcomes",
    "MigrationRolledBack",
    "already completed with incompatible outcome",
];

const REQUIRED_MIGRATION_FAILURE_ANCHORS: &[&str] = &[
    "timeout_migration_at_tick",
    "partial_commit_migration_at_tick",
];

const REQUIRED_TIMER_SCHEMA_ANCHORS: &[&str] = &[
    "pub(crate) struct VmTimerSnapshot",
    "owner: VmProcessId",
    "deadline_tick: u64",
    "kind: VmTimerKind",
    "snapshots",
];

const REQUIRED_RESOURCE_SCHEMA_ANCHORS: &[&str] = &[
    "pub(crate) struct VmResourceSnapshot",
    "owner: VmProcessId",
    "kind: String",
    "label: String",
    "transfer_policy: VmResourceTransferPolicy",
    "stale native resource handle",
];

const REQUIRED_PERSISTENT_ACTOR_SCHEMA_ANCHORS: &[&str] = &[
    "pub(crate) struct VmPersistentActorSchemaKey",
    "pub(crate) struct VmPersistentActorSchemaDescriptor",
    "pub(crate) struct VmPersistentActorMigrationEdge",
    "pub(crate) enum VmPersistentActorMigrationGuard",
    "pub(crate) enum VmPersistentActorMigrationEffect",
    "pub(crate) enum VmPersistentActorSchemaError",
    "pub(crate) struct VmPersistentActorMigrationGraph",
    "validate_event_migration_sequence",
    "DuplicateSchemaId",
    "MissingMigrationEdge",
    "MigrationGraphCycle",
    "AmbiguousMigrationEdge",
    "NondeterministicMigrationGuard",
    "SideEffectfulMigration",
    "WallClockDependentMigration",
    "RequiredFieldLost",
    "UnknownEventConstructorVariant",
    "IncompatibleMailboxPayloadSchema",
    "StalePackageSchemaVersion",
    "OutOfOrderEventMigration",
];

const REQUIRED_SCHEMA_TEST_ANCHORS: &[(&str, &[&str])] = &[
    (
        "std/vm/PersistentActorTest.terl",
        &[
            "schema_declaration_is_source_visible",
            "PersistentActor.schema(",
            "SchemaDeclaration",
            "package_event_variant_schema_id_is_source_visible",
            "PersistentActor.event_variant_schema(",
            "EventVariantSchemaId",
            "durable_adapter_schema_metadata_is_source_visible",
            "PersistentActor.durable_adapter_schema(",
            "DurableAdapterSchemaMetadata",
            "migration_rollback_after_failed_schema_migration_is_source_visible",
            "PersistentActor.migration_rollback(",
            "MigrationRollbackPlan",
            "package_migration_registration_is_source_visible",
            "PersistentActor.register_package_migration(",
            "PackageMigrationRegistration",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/persistent_actor_schema_test.rs",
        &[
            "vm_persistent_actor_schema_plans_deterministic_migration_chain",
            "vm_persistent_actor_schema_rejects_duplicate_missing_and_cyclic_migrations",
            "vm_persistent_actor_schema_rejects_unsafe_migration_guards_and_effects",
            "vm_persistent_actor_schema_rejects_lossy_event_mailbox_and_package_changes",
            "vm_persistent_actor_schema_rejects_out_of_order_event_migration_sequences",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/distributed_state_test.rs",
        &[
            "vm_distributed_state_reports_conflicts_with_versions_and_policy",
            "vm_distributed_state_exports_and_imports_deterministic_snapshots",
            "vm_distributed_state_rejects_invalid_scopes_versions_and_snapshots",
            "snapshot contains duplicate state scope",
            "state version sequence must be non-zero",
            "state version node id must be non-empty",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
        &[
            "vm_distributed_storage_rejects_stale_snapshot_replay",
            "vm_distributed_storage_detects_corrupt_snapshot_checksum",
            "vm_distributed_storage_recovered_snapshots_preserve_sequence_watermark",
            "vm_distributed_storage_reopen_preserves_snapshots_and_sequence_watermark",
            "vm_distributed_storage_rejects_invalid_policy_and_snapshot_descriptors",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/distributed_scheduler/distributed_scheduler_test/migration.rs",
        &[
            "vm_distributed_scheduler_replays_duplicate_timeout_rollbacks_without_duplicate_envelopes",
            "vm_distributed_scheduler_rolls_back_storage_timeout_idempotently",
            "vm_distributed_scheduler_rolls_back_storage_partial_write_idempotently",
            "vm_distributed_scheduler_rejects_invalid_partial_commit_inputs",
            "vm_distributed_scheduler_rejects_invalid_migration_timeout_inputs",
        ],
    ),
    (
        "std/vm/DistributedStateTest.terl",
        &[
            "write_outcomes_are_explicit_and_versioned",
            "conflict_and_policy_mismatch_are_typed",
            "restore(snapshot)",
        ],
    ),
    (
        "std/vm/DistributedStorageTest.terl",
        &[
            "reopen_preserves_snapshots_and_sequence_watermark",
            "compaction_and_checksum_metadata_are_typed",
            "assert_equal(\"stale_snapshot\", stale_kind)",
            "assert_equal(\"reject_replay\", DistributedStorage.recovery_action(stale))",
        ],
    ),
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-persistent-actor-schema-check: vm-persistent-actor-store-check",
    "$(MAKE) vm-distributed-state-check",
    "$(MAKE) vm-distributed-transport-check",
    "runtime::vm::persistent_actor_schema::persistent_actor_schema_test::vm_persistent_actor_schema_plans_deterministic_migration_chain",
    "runtime::vm::persistent_actor_schema::persistent_actor_schema_test::vm_persistent_actor_schema_rejects_duplicate_missing_and_cyclic_migrations",
    "runtime::vm::persistent_actor_schema::persistent_actor_schema_test::vm_persistent_actor_schema_rejects_unsafe_migration_guards_and_effects",
    "runtime::vm::persistent_actor_schema::persistent_actor_schema_test::vm_persistent_actor_schema_rejects_lossy_event_mailbox_and_package_changes",
    "runtime::vm::persistent_actor_schema::persistent_actor_schema_test::vm_persistent_actor_schema_rejects_out_of_order_event_migration_sequences",
    "vm_persistent_actor_schema_test",
    "vm-persistent-actor-schema",
];

const SCHEMA_IDS: &[&str] = &[
    "persistent actor schema key: actor schema id plus monotonic version",
    "source-visible persistent actor schema declaration descriptor",
    "source-visible package event variant schema id descriptor",
    "source-visible durable adapter schema metadata descriptor",
    "source-visible failed migration rollback plan descriptor",
    "distributed state version: sequence plus writer node id",
    "distributed storage checkpoint: checkpoint id plus monotonic sequence",
    "distributed storage checksum: deterministic checkpoint integrity marker",
    "migration identity: actor id plus migration sequence",
    "timer snapshot identity: timer id plus owner and deadline",
    "resource snapshot identity: resource id plus owner, kind, label, policy",
];

const MIGRATION_GRAPH_CASES: &[&str] = &[
    "persistent actor chain: rename, default, tombstone, event variant, mailbox migration",
    "source-visible package migration registration descriptor",
    "compatible replay: identical distributed state version replays",
    "stale storage replay: incoming sequence below local watermark is rejected",
    "checksum repair: corrupt snapshot cannot load as valid actor state",
    "storage timeout rollback: migration rollback is idempotent",
    "partial write rollback: duplicate rollback does not duplicate envelopes",
    "incompatible terminal replay: different terminal outcome is rejected",
];

const COMPATIBILITY_MATRIX: &[&str] = &[
    "added field with default: accepted by VM migration planner when explicit",
    "renamed field with explicit mapping: accepted by VM migration planner when type-compatible",
    "removed field with tombstone: rejected until retention/compaction API exists",
    "enum or union constructor change: accepted when package event variant schema ids are explicit",
    "binary storage type-width change: rejected until binary descriptor ids exist",
    "same schema and same version: accepted as deterministic replay",
    "mailbox payload change: accepted by VM migration planner when explicitly migrated",
];

const REJECTED_MIGRATION_CASES: &[&str] = &[
    "duplicate schema id",
    "unknown actor state schema id",
    "missing migration edge",
    "migration graph cycle",
    "ambiguous migration edge",
    "nondeterministic migration guard",
    "side-effectful migration",
    "wall-clock-dependent migration",
    "unknown event constructor variant",
    "incompatible mailbox payload schema",
    "stale package schema version",
    "required field lost without default, rename, or tombstone",
    "out-of-order event migration sequence",
];

const REPLAY_TRACES: &[&str] = &[
    "distributed state snapshot exports deterministic scope order",
    "distributed state snapshot rejects duplicate scopes",
    "distributed storage snapshot preserves sequence watermark after reopen",
    "stale snapshot replay returns typed recovery action",
    "corrupt snapshot load returns checksum mismatch recovery action",
];

const ROLLBACK_OUTCOMES: &[&str] = &[
    "storage flush timeout rolls migration back idempotently",
    "storage partial write rolls migration back idempotently",
    "duplicate timeout rollback keeps event and envelope counts stable",
    "duplicate partial commit rollback keeps event and envelope counts stable",
    "late incompatible rollback is rejected before actor state is accepted",
    "source-visible persistent actor migration rollback plan can be typechecked",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmPersistentActorSchemaSummary {
    pub schema_id_count: usize,
    pub migration_graph_case_count: usize,
    pub compatibility_matrix_count: usize,
    pub rejected_migration_case_count: usize,
    pub report_path: PathBuf,
}

pub fn run_vm_persistent_actor_schema(
    root: &Path,
) -> QualityResult<VmPersistentActorSchemaSummary> {
    let mut diagnostics = Vec::new();
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/distributed_state.rs",
        REQUIRED_STATE_SCHEMA_ANCHORS,
        "VM persistent actor state schema foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/distributed_storage.rs",
        REQUIRED_STORAGE_SCHEMA_ANCHORS,
        "VM persistent actor storage schema foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/distributed_scheduler/mod.rs",
        REQUIRED_MIGRATION_SCHEMA_ANCHORS,
        "VM persistent actor migration graph foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/distributed_scheduler/fault.rs",
        REQUIRED_MIGRATION_FAILURE_ANCHORS,
        "VM persistent actor migration failure foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/timer.rs",
        REQUIRED_TIMER_SCHEMA_ANCHORS,
        "VM persistent actor timer schema foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/resource.rs",
        REQUIRED_RESOURCE_SCHEMA_ANCHORS,
        "VM persistent actor resource schema foundation",
    )?);
    diagnostics.extend(validate_required_terms(
        root,
        "crates/terlan/src/runtime/vm/persistent_actor_schema.rs",
        REQUIRED_PERSISTENT_ACTOR_SCHEMA_ANCHORS,
        "VM persistent actor schema migration planner",
    )?);
    for (relative, anchors) in REQUIRED_SCHEMA_TEST_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM persistent actor schema adversarial tests",
        )?);
    }
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_no_placeholder_report_entries());
    if !diagnostics.is_empty() {
        return Err(render_failure("vm-persistent-actor-schema", &diagnostics));
    }

    let report_path = root.join(REPORT_PATH);
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!(
                "{}: failed to create report directory: {err}",
                parent.display()
            )
        })?;
    }
    let report = json!({
        "schema": "terlan-vm-persistent-actor-schema-report-v1",
        "schemaIds": SCHEMA_IDS,
        "migrationGraphCases": MIGRATION_GRAPH_CASES,
        "compatibilityMatrix": COMPATIBILITY_MATRIX,
        "rejectedMigrationCases": REJECTED_MIGRATION_CASES,
        "replayBeforeAfterTraces": REPLAY_TRACES,
        "rollbackOutcomes": ROLLBACK_OUTCOMES
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM persistent actor schema report: {err}"))?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmPersistentActorSchemaSummary {
        schema_id_count: SCHEMA_IDS.len(),
        migration_graph_case_count: MIGRATION_GRAPH_CASES.len(),
        compatibility_matrix_count: COMPATIBILITY_MATRIX.len(),
        rejected_migration_case_count: REJECTED_MIGRATION_CASES.len(),
        report_path,
    })
}

fn validate_required_terms(
    root: &Path,
    relative: &str,
    terms: &[&str],
    label: &str,
) -> QualityResult<Vec<String>> {
    let text = fs::read_to_string(root.join(relative))
        .map_err(|err| format!("{relative}: failed to read {label}: {err}"))?;
    Ok(terms
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("{relative}: missing {label} anchor `{term}`"))
        .collect())
}

fn validate_makefile(root: &Path) -> QualityResult<Vec<String>> {
    let text = fs::read_to_string(root.join("Makefile"))
        .map_err(|err| format!("Makefile: failed to read persistent actor schema gate: {err}"))?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing persistent actor schema gate term `{term}`"))
        .collect())
}

pub(crate) fn validate_no_placeholder_report_entries() -> Vec<String> {
    let mut diagnostics = Vec::new();
    for (label, entries) in [
        ("schema ids", SCHEMA_IDS),
        ("migration graph cases", MIGRATION_GRAPH_CASES),
        ("compatibility matrix", COMPATIBILITY_MATRIX),
        ("rejected migration cases", REJECTED_MIGRATION_CASES),
        ("replay before/after traces", REPLAY_TRACES),
        ("rollback outcomes", ROLLBACK_OUTCOMES),
    ] {
        diagnostics.extend(validate_entries_for_placeholder_terms(label, entries));
    }
    diagnostics
}

pub(crate) fn validate_entries_for_placeholder_terms(label: &str, entries: &[&str]) -> Vec<String> {
    entries
        .iter()
        .flat_map(|entry| {
            let lower = entry.to_ascii_lowercase();
            PLACEHOLDER_REPORT_TERMS
                .iter()
                .filter(move |term| lower.contains(**term))
                .map(move |term| {
                    format!(
                        "VM persistent actor schema {label} entry `{entry}` uses placeholder term `{term}`"
                    )
                })
        })
        .collect()
}

fn render_failure(label: &str, diagnostics: &[String]) -> String {
    let mut message = format!("[{label}] failures:");
    for diagnostic in diagnostics {
        message.push_str("\n  - ");
        message.push_str(diagnostic);
    }
    message
}

#[cfg(test)]
#[path = "vm_persistent_actor_schema_test.rs"]
mod vm_persistent_actor_schema_test;
