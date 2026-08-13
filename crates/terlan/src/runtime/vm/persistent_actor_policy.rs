#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(crate) enum VmPersistentActorPolicyRole {
    ActorOwner,
    ActorFamilyOwner,
    ProductionOperator,
    Debugger,
    SupportBundleExporter,
    ModelSyncSubscriber,
    StorageAdapter,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(crate) enum VmPersistentActorPolicyOperation {
    Append,
    Snapshot,
    Checkpoint,
    Replay,
    Compaction,
    Export,
    Restore,
    SchemaMigration,
    Inspection,
    TelemetryAccess,
    ResourceHandleRecovery,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct VmPersistentActorPolicy {
    pub(crate) policy_id: String,
    pub(crate) actor_id: String,
    pub(crate) actor_family: String,
    pub(crate) owner_id: String,
    pub(crate) package_version: String,
    pub(crate) allow_debugger_actor_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct VmPersistentActorPolicyRequest {
    pub(crate) role: VmPersistentActorPolicyRole,
    pub(crate) operation: VmPersistentActorPolicyOperation,
    pub(crate) requester_id: String,
    pub(crate) actor_id: String,
    pub(crate) actor_family: String,
    pub(crate) package_version: String,
    pub(crate) secret_bearing: bool,
    pub(crate) via_storage_adapter: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct VmPersistentActorPolicyAudit {
    pub(crate) operation: VmPersistentActorPolicyOperation,
    pub(crate) actor_id: String,
    pub(crate) actor_family: String,
    pub(crate) requester_role: VmPersistentActorPolicyRole,
    pub(crate) policy_id: String,
    pub(crate) decision: String,
    pub(crate) denial_reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct VmPersistentActorPolicyDecision {
    pub(crate) allowed: bool,
    pub(crate) audit: VmPersistentActorPolicyAudit,
}

#[cfg(test)]
pub(crate) fn authorize_persistent_actor_operation(
    policy: &VmPersistentActorPolicy,
    request: &VmPersistentActorPolicyRequest,
) -> VmPersistentActorPolicyDecision {
    let denial = persistent_actor_policy_denial(policy, request);
    VmPersistentActorPolicyDecision {
        allowed: denial.is_none(),
        audit: VmPersistentActorPolicyAudit {
            operation: request.operation.clone(),
            actor_id: request.actor_id.clone(),
            actor_family: request.actor_family.clone(),
            requester_role: request.role.clone(),
            policy_id: policy.policy_id.clone(),
            decision: if denial.is_none() {
                "allow".to_string()
            } else {
                "deny".to_string()
            },
            denial_reason: denial,
        },
    }
}

#[cfg(test)]
pub(crate) fn default_orders_policy() -> VmPersistentActorPolicy {
    VmPersistentActorPolicy {
        policy_id: "persistent-actor-policy:v1".to_string(),
        actor_id: "actor-1".to_string(),
        actor_family: "orders".to_string(),
        owner_id: "owner-1".to_string(),
        package_version: "orders@1".to_string(),
        allow_debugger_actor_id: Some("actor-1".to_string()),
    }
}

#[cfg(test)]
pub(crate) fn owner_append_request() -> VmPersistentActorPolicyRequest {
    VmPersistentActorPolicyRequest {
        role: VmPersistentActorPolicyRole::ActorOwner,
        operation: VmPersistentActorPolicyOperation::Append,
        requester_id: "owner-1".to_string(),
        actor_id: "actor-1".to_string(),
        actor_family: "orders".to_string(),
        package_version: "orders@1".to_string(),
        secret_bearing: false,
        via_storage_adapter: false,
    }
}

#[cfg(test)]
fn persistent_actor_policy_denial(
    policy: &VmPersistentActorPolicy,
    request: &VmPersistentActorPolicyRequest,
) -> Option<String> {
    if request.via_storage_adapter {
        return Some("storage_adapter_bypass_denied".to_string());
    }
    if request.actor_id != policy.actor_id {
        return Some("forged_or_cross_actor_id".to_string());
    }
    if request.actor_family != policy.actor_family {
        return Some("wrong_actor_family".to_string());
    }
    if request.package_version != policy.package_version {
        return Some("package_version_mismatch".to_string());
    }
    if request.secret_bearing {
        return Some("secret_bearing_access_denied".to_string());
    }

    match (&request.role, &request.operation) {
        (VmPersistentActorPolicyRole::ActorOwner, operation)
            if request.requester_id == policy.owner_id && owner_operation_allowed(operation) =>
        {
            None
        }
        (
            VmPersistentActorPolicyRole::ActorFamilyOwner,
            VmPersistentActorPolicyOperation::Restore,
        ) => Some("restore_requires_operator_approval".to_string()),
        (
            VmPersistentActorPolicyRole::ProductionOperator,
            VmPersistentActorPolicyOperation::Restore,
        ) => None,
        (
            VmPersistentActorPolicyRole::ProductionOperator,
            VmPersistentActorPolicyOperation::SchemaMigration,
        ) => None,
        (VmPersistentActorPolicyRole::Debugger, VmPersistentActorPolicyOperation::Inspection)
            if policy.allow_debugger_actor_id.as_deref() == Some(request.actor_id.as_str()) =>
        {
            None
        }
        (
            VmPersistentActorPolicyRole::SupportBundleExporter,
            VmPersistentActorPolicyOperation::Export,
        ) => Some("support_bundle_export_requires_redaction_policy".to_string()),
        (
            VmPersistentActorPolicyRole::ModelSyncSubscriber,
            VmPersistentActorPolicyOperation::TelemetryAccess,
        ) => None,
        _ => Some("operation_denied_by_default".to_string()),
    }
}

#[cfg(test)]
fn owner_operation_allowed(operation: &VmPersistentActorPolicyOperation) -> bool {
    matches!(
        operation,
        VmPersistentActorPolicyOperation::Append
            | VmPersistentActorPolicyOperation::Snapshot
            | VmPersistentActorPolicyOperation::Checkpoint
            | VmPersistentActorPolicyOperation::Replay
            | VmPersistentActorPolicyOperation::Compaction
            | VmPersistentActorPolicyOperation::ResourceHandleRecovery
    )
}

#[cfg(test)]
#[path = "persistent_actor_policy_test.rs"]
#[cfg(test)]
mod persistent_actor_policy_test;
