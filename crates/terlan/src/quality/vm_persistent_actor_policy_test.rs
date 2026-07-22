use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    run_vm_persistent_actor_policy, validate_entries_for_placeholder_terms,
    validate_no_placeholder_report_entries,
};

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
            "terlan-vm-persistent-actor-policy-{name}-{}-{unique}",
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
            "crates/terlan/src/runtime/vm/resource.rs",
            r#"
VmResourceTransferPolicy OwnerOnly transfer_policy VmResourceSnapshot
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/table.rs",
            r#"
VmTableAccess OwnerOnly PublicRead PublicReadWrite table_access_diagnostic
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_storage.rs",
            r#"
VmDistributedStoragePolicy can_cluster_replicate Unsupported StorageUnavailable
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/http_router.rs",
            r#"
dispatch_with_middleware_policy
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/process.rs",
            r#"
VmProcessSource resource_handles spawn_root
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/persistent_actor_policy.rs",
            r#"
VmPersistentActorPolicyRole VmPersistentActorPolicyOperation
authorize_persistent_actor_operation storage_adapter_bypass_denied
secret_bearing_access_denied operation_denied_by_default
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/table_test.rs",
            r#"
table_store_owner_only_rejects_non_owner_reads_and_writes
table_store_public_read_allows_reads_but_rejects_non_owner_writes
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/resource_test.rs",
            r#"
resource_table_rejects_wrong_owner_access_transfer_and_release
resource_table_transfers_transferable_resource_between_live_processes
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/distributed_storage_test.rs",
            r#"
vm_distributed_storage_cluster_capability_requires_cluster_mode_and_availability
vm_distributed_storage_reports_unsupported_cluster_replication_for_local_mode
"#,
        )?;
        self.write(
            "crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs",
            r#"
vm_persistent_actor_policy_allows_owner_append_with_audit_trace
vm_persistent_actor_policy_allows_owner_lifecycle_operations
vm_persistent_actor_policy_denies_wrong_owner_and_forged_actor_id
vm_persistent_actor_policy_records_denied_audit_trace_fields
vm_persistent_actor_policy_denies_owner_sensitive_operations_by_default
vm_persistent_actor_policy_allows_operator_schema_migration_only
vm_persistent_actor_policy_scopes_resource_handle_recovery
vm_persistent_actor_policy_scopes_debugger_and_denies_secret_telemetry
vm_persistent_actor_policy_denies_debugger_privilege_escalation
vm_persistent_actor_policy_denies_model_sync_permission_drift
vm_persistent_actor_policy_denies_support_export_and_storage_adapter_bypass
vm_persistent_actor_policy_denies_storage_adapter_export_bypass
vm_persistent_actor_policy_denies_support_bundle_overread
vm_persistent_actor_policy_rejects_package_downgrade_and_wrong_family_restore
"#,
        )?;
        self.write("Makefile", COMPLETE_MAKEFILE)
    }
}

impl Drop for TestRepo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

const COMPLETE_MAKEFILE: &str = r#"
vm-persistent-actor-policy-check: vm-persistent-actor-telemetry-check
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_policy::persistent_actor_policy_test::vm_persistent_actor_policy_allows_owner_append_with_audit_trace -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_policy::persistent_actor_policy_test::vm_persistent_actor_policy_allows_owner_lifecycle_operations -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_policy::persistent_actor_policy_test::vm_persistent_actor_policy_denies_wrong_owner_and_forged_actor_id -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_policy::persistent_actor_policy_test::vm_persistent_actor_policy_records_denied_audit_trace_fields -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_policy::persistent_actor_policy_test::vm_persistent_actor_policy_denies_owner_sensitive_operations_by_default -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_policy::persistent_actor_policy_test::vm_persistent_actor_policy_allows_operator_schema_migration_only -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_policy::persistent_actor_policy_test::vm_persistent_actor_policy_scopes_resource_handle_recovery -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_policy::persistent_actor_policy_test::vm_persistent_actor_policy_scopes_debugger_and_denies_secret_telemetry -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_policy::persistent_actor_policy_test::vm_persistent_actor_policy_denies_debugger_privilege_escalation -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_policy::persistent_actor_policy_test::vm_persistent_actor_policy_denies_model_sync_permission_drift -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_policy::persistent_actor_policy_test::vm_persistent_actor_policy_denies_support_export_and_storage_adapter_bypass -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_policy::persistent_actor_policy_test::vm_persistent_actor_policy_denies_storage_adapter_export_bypass -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_policy::persistent_actor_policy_test::vm_persistent_actor_policy_denies_support_bundle_overread -- --exact
	bash scripts/run_exact_cargo_test.sh -p terlan --bin terlan-vm runtime::vm::persistent_actor_policy::persistent_actor_policy_test::vm_persistent_actor_policy_rejects_package_downgrade_and_wrong_family_restore -- --exact
	$(RUST_TEST) --locked -p terlan --bin terlan-quality vm_persistent_actor_policy_test
	$(CARGO) run -p terlan --bin terlan-quality --quiet -- vm-persistent-actor-policy
"#;

#[test]
fn vm_persistent_actor_policy_writes_report_for_current_foundation() {
    let repo = TestRepo::new("complete").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");

    let summary = run_vm_persistent_actor_policy(repo.root()).expect("quality check");

    assert_eq!(summary.role_count, 9);
    assert_eq!(summary.operation_count, 11);
    assert_eq!(summary.deterministic_policy_decision_count, 9);
    assert_eq!(summary.adversarial_policy_case_count, 8);
    assert_eq!(summary.rejected_policy_path_count, 10);
    let report = fs::read_to_string(summary.report_path).expect("read report");
    assert!(report.contains("terlan-vm-persistent-actor-policy-report-v1"));
    assert!(report.contains("denyByDefaultOperations"));
    assert!(report.contains("storage adapter direct restore denied"));
    assert!(report.contains("owner append allow audit"));
    assert!(report.contains("storage adapter bypass denial"));
    assert!(report.contains("real persistent actor authorization runtime"));
    assert!(!report.to_ascii_lowercase().contains("placeholder"));
}

#[test]
fn vm_persistent_actor_policy_rejects_missing_resource_policy_anchor() {
    let repo = TestRepo::new("missing-resource").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo.root().join("crates/terlan/src/runtime/vm/resource.rs");
    let source = fs::read_to_string(&path).expect("resource source");
    repo.write(
        "crates/terlan/src/runtime/vm/resource.rs",
        &source.replace("VmResourceTransferPolicy", ""),
    )
    .expect("rewrite resource source");

    let error = run_vm_persistent_actor_policy(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("VmResourceTransferPolicy"));
}

#[test]
fn vm_persistent_actor_policy_rejects_missing_non_owner_fixture_anchor() {
    let repo = TestRepo::new("missing-table-fixture").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/table_test.rs");
    let source = fs::read_to_string(&path).expect("table test source");
    repo.write(
        "crates/terlan/src/runtime/vm/table_test.rs",
        &source.replace(
            "table_store_owner_only_rejects_non_owner_reads_and_writes",
            "",
        ),
    )
    .expect("rewrite table test source");

    let error = run_vm_persistent_actor_policy(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("owner_only_rejects_non_owner"));
}

#[test]
fn vm_persistent_actor_policy_rejects_missing_lifecycle_anchor() {
    let repo = TestRepo::new("missing-lifecycle").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs");
    let source = fs::read_to_string(&path).expect("persistent actor policy test source");
    repo.write(
        "crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs",
        &source.replace(
            "vm_persistent_actor_policy_allows_owner_lifecycle_operations",
            "",
        ),
    )
    .expect("rewrite persistent actor policy test source");

    let error = run_vm_persistent_actor_policy(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("allows_owner_lifecycle_operations"));
}

#[test]
fn vm_persistent_actor_policy_rejects_missing_denied_audit_anchor() {
    let repo = TestRepo::new("missing-denied-audit").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs");
    let source = fs::read_to_string(&path).expect("persistent actor policy test source");
    repo.write(
        "crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs",
        &source.replace(
            "vm_persistent_actor_policy_records_denied_audit_trace_fields",
            "",
        ),
    )
    .expect("rewrite persistent actor policy test source");

    let error = run_vm_persistent_actor_policy(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("records_denied_audit_trace_fields"));
}

#[test]
fn vm_persistent_actor_policy_rejects_missing_owner_sensitive_anchor() {
    let repo = TestRepo::new("missing-owner-sensitive").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs");
    let source = fs::read_to_string(&path).expect("persistent actor policy test source");
    repo.write(
        "crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs",
        &source.replace(
            "vm_persistent_actor_policy_denies_owner_sensitive_operations_by_default",
            "",
        ),
    )
    .expect("rewrite persistent actor policy test source");

    let error = run_vm_persistent_actor_policy(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("owner_sensitive_operations_by_default"));
}

#[test]
fn vm_persistent_actor_policy_rejects_missing_operator_schema_anchor() {
    let repo = TestRepo::new("missing-operator-schema").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs");
    let source = fs::read_to_string(&path).expect("persistent actor policy test source");
    repo.write(
        "crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs",
        &source.replace(
            "vm_persistent_actor_policy_allows_operator_schema_migration_only",
            "",
        ),
    )
    .expect("rewrite persistent actor policy test source");

    let error = run_vm_persistent_actor_policy(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("operator_schema_migration"));
}

#[test]
fn vm_persistent_actor_policy_rejects_missing_resource_recovery_anchor() {
    let repo = TestRepo::new("missing-resource-recovery").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs");
    let source = fs::read_to_string(&path).expect("persistent actor policy test source");
    repo.write(
        "crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs",
        &source.replace(
            "vm_persistent_actor_policy_scopes_resource_handle_recovery",
            "",
        ),
    )
    .expect("rewrite persistent actor policy test source");

    let error = run_vm_persistent_actor_policy(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("scopes_resource_handle_recovery"));
}

#[test]
fn vm_persistent_actor_policy_rejects_missing_model_sync_drift_anchor() {
    let repo = TestRepo::new("missing-model-sync-drift").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs");
    let source = fs::read_to_string(&path).expect("persistent actor policy test source");
    repo.write(
        "crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs",
        &source.replace(
            "vm_persistent_actor_policy_denies_model_sync_permission_drift",
            "",
        ),
    )
    .expect("rewrite persistent actor policy test source");

    let error = run_vm_persistent_actor_policy(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("model_sync_permission_drift"));
}

#[test]
fn vm_persistent_actor_policy_rejects_missing_debugger_escalation_anchor() {
    let repo = TestRepo::new("missing-debugger-escalation").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs");
    let source = fs::read_to_string(&path).expect("persistent actor policy test source");
    repo.write(
        "crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs",
        &source.replace(
            "vm_persistent_actor_policy_denies_debugger_privilege_escalation",
            "",
        ),
    )
    .expect("rewrite persistent actor policy test source");

    let error = run_vm_persistent_actor_policy(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("debugger_privilege_escalation"));
}

#[test]
fn vm_persistent_actor_policy_rejects_missing_support_overread_anchor() {
    let repo = TestRepo::new("missing-support-overread").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs");
    let source = fs::read_to_string(&path).expect("persistent actor policy test source");
    repo.write(
        "crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs",
        &source.replace(
            "vm_persistent_actor_policy_denies_support_bundle_overread",
            "",
        ),
    )
    .expect("rewrite persistent actor policy test source");

    let error = run_vm_persistent_actor_policy(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("support_bundle_overread"));
}

#[test]
fn vm_persistent_actor_policy_rejects_missing_adapter_export_bypass_anchor() {
    let repo = TestRepo::new("missing-adapter-export-bypass").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    let path = repo
        .root()
        .join("crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs");
    let source = fs::read_to_string(&path).expect("persistent actor policy test source");
    repo.write(
        "crates/terlan/src/runtime/vm/persistent_actor_policy_test.rs",
        &source.replace(
            "vm_persistent_actor_policy_denies_storage_adapter_export_bypass",
            "",
        ),
    )
    .expect("rewrite persistent actor policy test source");

    let error = run_vm_persistent_actor_policy(repo.root()).expect_err("anchor should fail");

    assert!(error.contains("storage_adapter_export_bypass"));
}

#[test]
fn vm_persistent_actor_policy_rejects_missing_make_gate_term() {
    let repo = TestRepo::new("missing-gate").expect("fixture");
    repo.write_complete_fixture().expect("write fixture");
    repo.write(
        "Makefile",
        &COMPLETE_MAKEFILE.replace("vm_persistent_actor_policy_test", ""),
    )
    .expect("rewrite makefile");

    let error = run_vm_persistent_actor_policy(repo.root()).expect_err("gate should fail");

    assert!(error.contains("vm_persistent_actor_policy_test"));
}

#[test]
fn vm_persistent_actor_policy_rejects_placeholder_report_entries() {
    let diagnostics = validate_no_placeholder_report_entries();

    assert!(
        diagnostics.is_empty(),
        "VM persistent actor policy report evidence must not contain placeholder labels: {diagnostics:?}"
    );

    let injected = validate_entries_for_placeholder_terms(
        "privilege escalation attempts",
        &["todo debugger privilege escalation"],
    );
    assert!(
        injected
            .iter()
            .any(|diagnostic| diagnostic.contains("placeholder term")),
        "expected injected placeholder diagnostic: {injected:?}"
    );
}
