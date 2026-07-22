use super::{
    authorize_persistent_actor_operation, default_orders_policy, owner_append_request,
    VmPersistentActorPolicyOperation, VmPersistentActorPolicyRole,
};

#[test]
fn vm_persistent_actor_policy_allows_owner_append_with_audit_trace() {
    let policy = default_orders_policy();
    let decision = authorize_persistent_actor_operation(&policy, &owner_append_request());

    assert!(decision.allowed);
    assert_eq!(decision.audit.decision, "allow");
    assert_eq!(decision.audit.actor_id, "actor-1");
    assert_eq!(decision.audit.policy_id, "persistent-actor-policy:v1");
    assert_eq!(decision.audit.denial_reason, None);
}

#[test]
fn vm_persistent_actor_policy_allows_owner_lifecycle_operations() {
    let policy = default_orders_policy();
    for operation in [
        VmPersistentActorPolicyOperation::Snapshot,
        VmPersistentActorPolicyOperation::Checkpoint,
        VmPersistentActorPolicyOperation::Replay,
        VmPersistentActorPolicyOperation::Compaction,
    ] {
        let mut request = owner_append_request();
        request.operation = operation;

        let decision = authorize_persistent_actor_operation(&policy, &request);

        assert!(decision.allowed);
        assert_eq!(decision.audit.decision, "allow");
        assert_eq!(decision.audit.denial_reason, None);
    }
}

#[test]
fn vm_persistent_actor_policy_denies_wrong_owner_and_forged_actor_id() {
    let policy = default_orders_policy();
    let mut wrong_owner = owner_append_request();
    wrong_owner.requester_id = "intruder".to_string();

    let wrong_owner_decision = authorize_persistent_actor_operation(&policy, &wrong_owner);
    assert!(!wrong_owner_decision.allowed);
    assert_eq!(
        wrong_owner_decision.audit.denial_reason,
        Some("operation_denied_by_default".to_string())
    );

    let mut forged_actor = owner_append_request();
    forged_actor.actor_id = "actor-2".to_string();
    let forged_actor_decision = authorize_persistent_actor_operation(&policy, &forged_actor);
    assert!(!forged_actor_decision.allowed);
    assert_eq!(
        forged_actor_decision.audit.denial_reason,
        Some("forged_or_cross_actor_id".to_string())
    );
}

#[test]
fn vm_persistent_actor_policy_records_denied_audit_trace_fields() {
    let policy = default_orders_policy();
    let mut wrong_owner = owner_append_request();
    wrong_owner.requester_id = "intruder".to_string();

    let decision = authorize_persistent_actor_operation(&policy, &wrong_owner);

    assert!(!decision.allowed);
    assert_eq!(
        decision.audit.operation,
        VmPersistentActorPolicyOperation::Append
    );
    assert_eq!(decision.audit.actor_id, "actor-1");
    assert_eq!(decision.audit.actor_family, "orders");
    assert_eq!(
        decision.audit.requester_role,
        VmPersistentActorPolicyRole::ActorOwner
    );
    assert_eq!(decision.audit.policy_id, "persistent-actor-policy:v1");
    assert_eq!(decision.audit.decision, "deny");
    assert_eq!(
        decision.audit.denial_reason,
        Some("operation_denied_by_default".to_string())
    );
}

#[test]
fn vm_persistent_actor_policy_denies_owner_sensitive_operations_by_default() {
    let policy = default_orders_policy();
    for operation in [
        VmPersistentActorPolicyOperation::Export,
        VmPersistentActorPolicyOperation::Restore,
        VmPersistentActorPolicyOperation::SchemaMigration,
    ] {
        let mut request = owner_append_request();
        request.operation = operation;

        let decision = authorize_persistent_actor_operation(&policy, &request);

        assert!(!decision.allowed);
        assert_eq!(
            decision.audit.denial_reason,
            Some("operation_denied_by_default".to_string())
        );
    }
}

#[test]
fn vm_persistent_actor_policy_allows_operator_schema_migration_only() {
    let policy = default_orders_policy();
    let mut operator = owner_append_request();
    operator.role = VmPersistentActorPolicyRole::ProductionOperator;
    operator.operation = VmPersistentActorPolicyOperation::SchemaMigration;
    operator.requester_id = "operator-1".to_string();

    assert!(authorize_persistent_actor_operation(&policy, &operator).allowed);

    let mut family_owner_restore = owner_append_request();
    family_owner_restore.role = VmPersistentActorPolicyRole::ActorFamilyOwner;
    family_owner_restore.operation = VmPersistentActorPolicyOperation::Restore;
    family_owner_restore.requester_id = "family-owner-1".to_string();

    let decision = authorize_persistent_actor_operation(&policy, &family_owner_restore);

    assert!(!decision.allowed);
    assert_eq!(
        decision.audit.denial_reason,
        Some("restore_requires_operator_approval".to_string())
    );
}

#[test]
fn vm_persistent_actor_policy_scopes_resource_handle_recovery() {
    let policy = default_orders_policy();
    let mut owner_recovery = owner_append_request();
    owner_recovery.operation = VmPersistentActorPolicyOperation::ResourceHandleRecovery;

    assert!(authorize_persistent_actor_operation(&policy, &owner_recovery).allowed);

    let mut debugger_recovery = owner_append_request();
    debugger_recovery.role = VmPersistentActorPolicyRole::Debugger;
    debugger_recovery.operation = VmPersistentActorPolicyOperation::ResourceHandleRecovery;
    debugger_recovery.requester_id = "debugger-1".to_string();

    let decision = authorize_persistent_actor_operation(&policy, &debugger_recovery);

    assert!(!decision.allowed);
    assert_eq!(
        decision.audit.denial_reason,
        Some("operation_denied_by_default".to_string())
    );
}

#[test]
fn vm_persistent_actor_policy_scopes_debugger_and_denies_secret_telemetry() {
    let policy = default_orders_policy();
    let mut debugger = owner_append_request();
    debugger.role = VmPersistentActorPolicyRole::Debugger;
    debugger.operation = VmPersistentActorPolicyOperation::Inspection;
    debugger.requester_id = "debugger-1".to_string();
    assert!(authorize_persistent_actor_operation(&policy, &debugger).allowed);

    let mut secret_telemetry = owner_append_request();
    secret_telemetry.role = VmPersistentActorPolicyRole::ModelSyncSubscriber;
    secret_telemetry.operation = VmPersistentActorPolicyOperation::TelemetryAccess;
    secret_telemetry.secret_bearing = true;
    let decision = authorize_persistent_actor_operation(&policy, &secret_telemetry);
    assert!(!decision.allowed);
    assert_eq!(
        decision.audit.denial_reason,
        Some("secret_bearing_access_denied".to_string())
    );
}

#[test]
fn vm_persistent_actor_policy_denies_debugger_privilege_escalation() {
    let policy = default_orders_policy();
    let mut debugger = owner_append_request();
    debugger.role = VmPersistentActorPolicyRole::Debugger;
    debugger.operation = VmPersistentActorPolicyOperation::Restore;
    debugger.requester_id = "debugger-1".to_string();

    let restore_decision = authorize_persistent_actor_operation(&policy, &debugger);

    assert!(!restore_decision.allowed);
    assert_eq!(
        restore_decision.audit.denial_reason,
        Some("operation_denied_by_default".to_string())
    );
}

#[test]
fn vm_persistent_actor_policy_denies_model_sync_permission_drift() {
    let policy = default_orders_policy();
    let mut subscriber = owner_append_request();
    subscriber.role = VmPersistentActorPolicyRole::ModelSyncSubscriber;
    subscriber.operation = VmPersistentActorPolicyOperation::TelemetryAccess;
    subscriber.requester_id = "model-sync-1".to_string();

    assert!(authorize_persistent_actor_operation(&policy, &subscriber).allowed);

    subscriber.package_version = "orders@0".to_string();
    let drift_decision = authorize_persistent_actor_operation(&policy, &subscriber);

    assert!(!drift_decision.allowed);
    assert_eq!(
        drift_decision.audit.denial_reason,
        Some("package_version_mismatch".to_string())
    );
}

#[test]
fn vm_persistent_actor_policy_denies_support_export_and_storage_adapter_bypass() {
    let policy = default_orders_policy();
    let mut support_export = owner_append_request();
    support_export.role = VmPersistentActorPolicyRole::SupportBundleExporter;
    support_export.operation = VmPersistentActorPolicyOperation::Export;
    let support_decision = authorize_persistent_actor_operation(&policy, &support_export);
    assert!(!support_decision.allowed);
    assert_eq!(
        support_decision.audit.denial_reason,
        Some("support_bundle_export_requires_redaction_policy".to_string())
    );

    let mut adapter_restore = owner_append_request();
    adapter_restore.role = VmPersistentActorPolicyRole::StorageAdapter;
    adapter_restore.operation = VmPersistentActorPolicyOperation::Restore;
    adapter_restore.via_storage_adapter = true;
    let adapter_decision = authorize_persistent_actor_operation(&policy, &adapter_restore);
    assert!(!adapter_decision.allowed);
    assert_eq!(
        adapter_decision.audit.denial_reason,
        Some("storage_adapter_bypass_denied".to_string())
    );
}

#[test]
fn vm_persistent_actor_policy_denies_storage_adapter_export_bypass() {
    let policy = default_orders_policy();
    let mut adapter_export = owner_append_request();
    adapter_export.role = VmPersistentActorPolicyRole::StorageAdapter;
    adapter_export.operation = VmPersistentActorPolicyOperation::Export;
    adapter_export.via_storage_adapter = true;

    let decision = authorize_persistent_actor_operation(&policy, &adapter_export);

    assert!(!decision.allowed);
    assert_eq!(
        decision.audit.denial_reason,
        Some("storage_adapter_bypass_denied".to_string())
    );
}

#[test]
fn vm_persistent_actor_policy_denies_support_bundle_overread() {
    let policy = default_orders_policy();
    let mut support_overread = owner_append_request();
    support_overread.role = VmPersistentActorPolicyRole::SupportBundleExporter;
    support_overread.operation = VmPersistentActorPolicyOperation::Export;
    support_overread.secret_bearing = true;

    let decision = authorize_persistent_actor_operation(&policy, &support_overread);

    assert!(!decision.allowed);
    assert_eq!(
        decision.audit.denial_reason,
        Some("secret_bearing_access_denied".to_string())
    );
}

#[test]
fn vm_persistent_actor_policy_rejects_package_downgrade_and_wrong_family_restore() {
    let policy = default_orders_policy();
    let mut downgrade = owner_append_request();
    downgrade.package_version = "orders@0".to_string();
    let downgrade_decision = authorize_persistent_actor_operation(&policy, &downgrade);
    assert!(!downgrade_decision.allowed);
    assert_eq!(
        downgrade_decision.audit.denial_reason,
        Some("package_version_mismatch".to_string())
    );

    let mut wrong_family_restore = owner_append_request();
    wrong_family_restore.role = VmPersistentActorPolicyRole::ProductionOperator;
    wrong_family_restore.operation = VmPersistentActorPolicyOperation::Restore;
    wrong_family_restore.actor_family = "payments".to_string();
    let restore_decision = authorize_persistent_actor_operation(&policy, &wrong_family_restore);
    assert!(!restore_decision.allowed);
    assert_eq!(
        restore_decision.audit.denial_reason,
        Some("wrong_actor_family".to_string())
    );
}
