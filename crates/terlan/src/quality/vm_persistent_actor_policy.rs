use std::fs;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::terlan_quality::QualityResult;

const REPORT_PATH: &str = "target/quality/vm-persistent-actor-policy-report.json";
const PLACEHOLDER_REPORT_TERMS: &[&str] = &["placeholder", "todo", "tbd"];

const REQUIRED_FOUNDATION_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/runtime/vm/resource.rs",
        &[
            "VmResourceTransferPolicy",
            "OwnerOnly",
            "transfer_policy",
            "VmResourceSnapshot",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/table.rs",
        &[
            "VmTableAccess",
            "OwnerOnly",
            "PublicRead",
            "PublicReadWrite",
            "table_access_diagnostic",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/distributed_storage.rs",
        &[
            "VmDistributedStoragePolicy",
            "can_cluster_replicate",
            "Unsupported",
            "StorageUnavailable",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/http_router.rs",
        &["dispatch_with_middleware_policy"],
    ),
    (
        "crates/terlan/src/runtime/vm/process.rs",
        &["VmProcessSource", "resource_handles", "spawn_root"],
    ),
    (
        "crates/terlan/src/runtime/vm/persistent_actor_policy.rs",
        &[
            "VmPersistentActorPolicyRole",
            "VmPersistentActorPolicyOperation",
            "authorize_persistent_actor_operation",
            "storage_adapter_bypass_denied",
            "secret_bearing_access_denied",
            "operation_denied_by_default",
        ],
    ),
];

const REQUIRED_TEST_ANCHORS: &[(&str, &[&str])] = &[
    (
        "crates/terlan/src/runtime/vm/table_test.rs",
        &[
            "table_store_owner_only_rejects_non_owner_reads_and_writes",
            "table_store_public_read_allows_reads_but_rejects_non_owner_writes",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/resource_test.rs",
        &[
            "resource_table_rejects_wrong_owner_access_transfer_and_release",
            "resource_table_transfers_transferable_resource_between_live_processes",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
        &[
            "vm_distributed_storage_cluster_capability_requires_cluster_mode_and_availability",
            "vm_distributed_storage_reports_unsupported_cluster_replication_for_local_mode",
        ],
    ),
    (
        "crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs",
        &[
            "vm_persistent_actor_policy_allows_owner_append_with_audit_trace",
            "vm_persistent_actor_policy_allows_owner_lifecycle_operations",
            "vm_persistent_actor_policy_denies_wrong_owner_and_forged_actor_id",
            "vm_persistent_actor_policy_records_denied_audit_trace_fields",
            "vm_persistent_actor_policy_denies_owner_sensitive_operations_by_default",
            "vm_persistent_actor_policy_allows_operator_schema_migration_only",
            "vm_persistent_actor_policy_scopes_resource_handle_recovery",
            "vm_persistent_actor_policy_scopes_debugger_and_denies_secret_telemetry",
            "vm_persistent_actor_policy_denies_debugger_privilege_escalation",
            "vm_persistent_actor_policy_denies_model_sync_permission_drift",
            "vm_persistent_actor_policy_denies_support_export_and_storage_adapter_bypass",
            "vm_persistent_actor_policy_denies_storage_adapter_export_bypass",
            "vm_persistent_actor_policy_denies_support_bundle_overread",
            "vm_persistent_actor_policy_rejects_package_downgrade_and_wrong_family_restore",
        ],
    ),
];

const REQUIRED_GATE_TERMS: &[&str] = &[
    "vm-persistent-actor-policy-check: vm-persistent-actor-telemetry-check",
    "vm_persistent_actor_policy_allows_owner_append_with_audit_trace",
    "vm_persistent_actor_policy_allows_owner_lifecycle_operations",
    "vm_persistent_actor_policy_denies_wrong_owner_and_forged_actor_id",
    "vm_persistent_actor_policy_records_denied_audit_trace_fields",
    "vm_persistent_actor_policy_denies_owner_sensitive_operations_by_default",
    "vm_persistent_actor_policy_allows_operator_schema_migration_only",
    "vm_persistent_actor_policy_scopes_resource_handle_recovery",
    "vm_persistent_actor_policy_scopes_debugger_and_denies_secret_telemetry",
    "vm_persistent_actor_policy_denies_debugger_privilege_escalation",
    "vm_persistent_actor_policy_denies_model_sync_permission_drift",
    "vm_persistent_actor_policy_denies_support_export_and_storage_adapter_bypass",
    "vm_persistent_actor_policy_denies_storage_adapter_export_bypass",
    "vm_persistent_actor_policy_denies_support_bundle_overread",
    "vm_persistent_actor_policy_rejects_package_downgrade_and_wrong_family_restore",
    "vm_persistent_actor_policy_test",
    "vm-persistent-actor-policy",
];

const POLICY_ROLES: &[&str] = &[
    "actor owner",
    "actor family owner",
    "package maintainer",
    "local developer",
    "production operator",
    "debugger",
    "support-bundle exporter",
    "model-sync subscriber",
    "storage adapter",
];

const POLICY_OPERATIONS: &[&str] = &[
    "append",
    "snapshot",
    "checkpoint",
    "replay",
    "compaction",
    "export",
    "restore",
    "schema migration",
    "inspection",
    "telemetry access",
    "resource-handle recovery",
];

const DENY_BY_DEFAULT_OPERATIONS: &[&str] = &[
    "restore",
    "export",
    "cross-actor inspection",
    "secret-bearing telemetry",
    "resource-handle recovery",
];

const ADVERSARIAL_POLICY_CASES: &[&str] = &[
    "forged actor id",
    "wrong owner",
    "package downgrade",
    "debugger privilege escalation",
    "support-bundle overread",
    "restore into another actor family",
    "model-sync permission drift",
    "adapter bypass attempt",
];

const ALLOWED_DENIED_OPERATION_FIXTURES: &[&str] = &[
    "owner append allowed",
    "non-owner append denied",
    "debugger inspect scoped",
    "support export redacted",
    "storage adapter direct restore denied",
];

const AUDIT_TRACE_FIELDS: &[&str] = &[
    "operation",
    "actor_id",
    "actor_family",
    "requester_role",
    "policy_id",
    "decision",
    "denial_reason",
];

const DETERMINISTIC_POLICY_DECISIONS: &[&str] = &[
    "owner append allow audit",
    "wrong owner deny-by-default audit",
    "forged actor id rejection",
    "debugger scoped inspection allow",
    "secret telemetry denial",
    "support bundle export redaction denial",
    "storage adapter bypass denial",
    "package downgrade rejection",
    "wrong actor family restore rejection",
];

const REDACTION_OUTCOMES: &[&str] = &[
    "secret-bearing telemetry denied",
    "resource handles expose stable ids only",
    "support bundle export requires redaction policy",
];

const ADAPTER_BYPASS_REJECTION_CASES: &[&str] = &[
    "adapter cannot restore without VM policy",
    "adapter cannot export without VM policy",
    "adapter unsupported capability remains typed",
];

const REJECTED_POLICY_PATHS: &[&str] = &[
    "real persistent actor authorization runtime",
    "policy check before append/snapshot/checkpoint/replay",
    "policy check before export/restore/schema migration",
    "policy check before telemetry subscription",
    "support-bundle exporter redaction policy",
    "debugger scoped access policy",
    "model-sync subscriber policy drift detection",
    "storage adapter bypass prevention",
    "stable audit event emission for denied operations",
    "privilege escalation fixture execution",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmPersistentActorPolicySummary {
    pub role_count: usize,
    pub operation_count: usize,
    pub deterministic_policy_decision_count: usize,
    pub adversarial_policy_case_count: usize,
    pub rejected_policy_path_count: usize,
    pub report_path: PathBuf,
}

pub fn run_vm_persistent_actor_policy(
    root: &Path,
) -> QualityResult<VmPersistentActorPolicySummary> {
    let mut diagnostics = Vec::new();
    for (relative, anchors) in REQUIRED_FOUNDATION_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM persistent actor policy foundation",
        )?);
    }
    for (relative, anchors) in REQUIRED_TEST_ANCHORS {
        diagnostics.extend(validate_required_terms(
            root,
            relative,
            anchors,
            "VM persistent actor policy fixture coverage",
        )?);
    }
    diagnostics.extend(validate_makefile(root)?);
    diagnostics.extend(validate_no_placeholder_report_entries());
    if !diagnostics.is_empty() {
        return Err(render_failure("vm-persistent-actor-policy", &diagnostics));
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
        "schema": "terlan-vm-persistent-actor-policy-report-v1",
        "policyMatrix": {
            "roles": POLICY_ROLES,
            "operations": POLICY_OPERATIONS,
            "denyByDefaultOperations": DENY_BY_DEFAULT_OPERATIONS
        },
        "allowedDeniedOperationFixtures": ALLOWED_DENIED_OPERATION_FIXTURES,
        "auditTraces": AUDIT_TRACE_FIELDS,
        "deterministicPolicyDecisions": DETERMINISTIC_POLICY_DECISIONS,
        "redactionOutcomes": REDACTION_OUTCOMES,
        "privilegeEscalationAttempts": ADVERSARIAL_POLICY_CASES,
        "adapterBypassRejectionCases": ADAPTER_BYPASS_REJECTION_CASES,
        "rejectedPolicyPaths": REJECTED_POLICY_PATHS
    });
    let report_text = serde_json::to_string_pretty(&report)
        .map_err(|err| format!("failed to serialize VM persistent actor policy report: {err}"))?;
    fs::write(&report_path, report_text)
        .map_err(|err| format!("{REPORT_PATH}: failed to write report: {err}"))?;

    Ok(VmPersistentActorPolicySummary {
        role_count: POLICY_ROLES.len(),
        operation_count: POLICY_OPERATIONS.len(),
        deterministic_policy_decision_count: DETERMINISTIC_POLICY_DECISIONS.len(),
        adversarial_policy_case_count: ADVERSARIAL_POLICY_CASES.len(),
        rejected_policy_path_count: REJECTED_POLICY_PATHS.len(),
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
        .map_err(|err| format!("Makefile: failed to read persistent actor policy gate: {err}"))?;
    Ok(REQUIRED_GATE_TERMS
        .iter()
        .filter(|term| !text.contains(**term))
        .map(|term| format!("Makefile: missing persistent actor policy gate term `{term}`"))
        .collect())
}

pub(crate) fn validate_no_placeholder_report_entries() -> Vec<String> {
    let mut diagnostics = Vec::new();
    for (label, entries) in [
        ("policy roles", POLICY_ROLES),
        ("policy operations", POLICY_OPERATIONS),
        ("deny-by-default operations", DENY_BY_DEFAULT_OPERATIONS),
        (
            "allowed/denied operation fixtures",
            ALLOWED_DENIED_OPERATION_FIXTURES,
        ),
        ("audit trace fields", AUDIT_TRACE_FIELDS),
        (
            "deterministic policy decisions",
            DETERMINISTIC_POLICY_DECISIONS,
        ),
        ("redaction outcomes", REDACTION_OUTCOMES),
        ("privilege escalation attempts", ADVERSARIAL_POLICY_CASES),
        (
            "adapter bypass rejection cases",
            ADAPTER_BYPASS_REJECTION_CASES,
        ),
        ("rejected policy paths", REJECTED_POLICY_PATHS),
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
                        "VM persistent actor policy {label} entry `{entry}` uses placeholder term `{term}`"
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
#[path = "vm_persistent_actor_policy_test.rs"]
mod vm_persistent_actor_policy_test;
