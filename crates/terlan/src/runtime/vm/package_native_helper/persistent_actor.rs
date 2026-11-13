//! VM-owned persistent-actor contract descriptors.

use super::descriptor::VmDescriptorRuntime;
use super::{PureNativeCapabilityRequest, ReplValue, VmRuntimeResult};

const PREFIX: &str = "std.vm.PersistentActor.";

/// Persistent-actor declarations are immutable, structurally interned values.
#[derive(Default)]
pub(super) struct VmPersistentActorRuntime {
    descriptors: VmDescriptorRuntime,
}

impl VmPersistentActorRuntime {
    pub(super) fn call(
        &mut self,
        owner: u64,
        request: &PureNativeCapabilityRequest,
    ) -> VmRuntimeResult<ReplValue> {
        let args = request
            .package_arguments
            .as_deref()
            .ok_or("error[vm.persistent_actor.arguments]: operations require typed arguments")?;
        let operation = request.operation.as_str();
        if operation == "std.vm.persistent_actor.compatible_schema" {
            let [expected, actual] = args else {
                return unsupported(operation, args.len());
            };
            let expected = self.descriptors.get(owner, expected, kind("SchemaId"))?;
            let actual = self.descriptors.get(owner, actual, kind("SchemaId"))?;
            return Ok(ReplValue::Bool(expected.arguments == actual.arguments));
        }
        let (descriptor_kind, expected_arity) =
            constructor(operation).ok_or_else(|| unsupported_message(operation, args.len()))?;
        if args.len() != expected_arity {
            return unsupported(operation, args.len());
        }
        validate(operation, args)?;
        self.descriptors
            .insert(owner, kind(descriptor_kind), args.to_vec())
    }

    pub(super) fn close_owner(&mut self, owner: u64) {
        self.descriptors.close_owner(owner);
    }
}

fn constructor(operation: &str) -> Option<(&'static str, usize)> {
    Some(match operation {
        "std.vm.persistent_actor.actor_id" => ("ActorId", 2),
        "std.vm.persistent_actor.schema_id" => ("SchemaId", 2),
        "std.vm.persistent_actor.schema" => ("SchemaDeclaration", 4),
        "std.vm.persistent_actor.event_variant_schema" => ("EventVariantSchemaId", 5),
        "std.vm.persistent_actor.durable_adapter_schema" => ("DurableAdapterSchemaMetadata", 5),
        "std.vm.persistent_actor.persistent_actor" => ("PersistentActorDeclaration", 3),
        "std.vm.persistent_actor.retention_policy" => ("RetentionPolicy", 3),
        "std.vm.persistent_actor.family_retention_defaults" => ("ActorFamilyRetentionDefaults", 3),
        "std.vm.persistent_actor.audit_retention" => ("AuditRetentionPlan", 3),
        "std.vm.persistent_actor.package_retention_policy" => ("PackageRetentionPolicyBinding", 3),
        "std.vm.persistent_actor.model_sync_retention_continuity" => {
            ("ModelSyncRetentionContinuityPlan", 4)
        }
        "std.vm.persistent_actor.redaction_policy" => ("RedactionPolicy", 3),
        "std.vm.persistent_actor.snapshot" => ("SnapshotPlan", 4),
        "std.vm.persistent_actor.replay" => ("ReplayPlan", 2),
        "std.vm.persistent_actor.resource_checkpoint" => ("ResourceCheckpoint", 2),
        "std.vm.persistent_actor.restore_resource" => ("ResourceRestorePlan", 2),
        "std.vm.persistent_actor.mailbox_checkpoint" => ("MailboxCheckpoint", 2),
        "std.vm.persistent_actor.restore_mailbox" => ("MailboxRestorePlan", 2),
        "std.vm.persistent_actor.migration_rollback" => ("MigrationRollbackPlan", 5),
        "std.vm.persistent_actor.register_package_migration" => ("PackageMigrationRegistration", 5),
        "std.vm.persistent_actor.timer_checkpoint" => ("TimerCheckpoint", 3),
        "std.vm.persistent_actor.restore_timer" => ("TimerRestorePlan", 2),
        "std.vm.persistent_actor.package_store" => ("PackageStoreBinding", 3),
        _ => return None,
    })
}

fn validate(operation: &str, args: &[ReplValue]) -> VmRuntimeResult<()> {
    for value in args {
        if matches!(value, ReplValue::String(text) if text.is_empty()) {
            return Err(format!(
                "error[vm.persistent_actor.value]: `{operation}` requires non-empty text"
            )
            .into());
        }
    }
    for index in integer_indexes(operation) {
        if matches!(args.get(*index), Some(ReplValue::Int(value)) if *value < 0) {
            return Err(format!(
                "error[vm.persistent_actor.value]: `{operation}` requires nonnegative integers"
            )
            .into());
        }
    }
    Ok(())
}

fn integer_indexes(operation: &str) -> &'static [usize] {
    match operation {
        "std.vm.persistent_actor.schema_id" => &[1],
        "std.vm.persistent_actor.event_variant_schema" => &[4],
        "std.vm.persistent_actor.durable_adapter_schema" => &[4],
        "std.vm.persistent_actor.retention_policy" => &[0, 1, 2],
        "std.vm.persistent_actor.model_sync_retention_continuity" => &[3],
        "std.vm.persistent_actor.snapshot" => &[2],
        "std.vm.persistent_actor.mailbox_checkpoint" => &[1],
        "std.vm.persistent_actor.migration_rollback" => &[3],
        "std.vm.persistent_actor.timer_checkpoint" => &[2],
        _ => &[],
    }
}

fn kind(name: &str) -> &str {
    match name {
        "ActorId" => "std.vm.PersistentActor.ActorId",
        "SchemaId" => "std.vm.PersistentActor.SchemaId",
        "SchemaDeclaration" => "std.vm.PersistentActor.SchemaDeclaration",
        "EventVariantSchemaId" => "std.vm.PersistentActor.EventVariantSchemaId",
        "DurableAdapterSchemaMetadata" => "std.vm.PersistentActor.DurableAdapterSchemaMetadata",
        "PersistentActorDeclaration" => "std.vm.PersistentActor.PersistentActorDeclaration",
        "RetentionPolicy" => "std.vm.PersistentActor.RetentionPolicy",
        "ActorFamilyRetentionDefaults" => "std.vm.PersistentActor.ActorFamilyRetentionDefaults",
        "AuditRetentionPlan" => "std.vm.PersistentActor.AuditRetentionPlan",
        "PackageRetentionPolicyBinding" => "std.vm.PersistentActor.PackageRetentionPolicyBinding",
        "ModelSyncRetentionContinuityPlan" => {
            "std.vm.PersistentActor.ModelSyncRetentionContinuityPlan"
        }
        "RedactionPolicy" => "std.vm.PersistentActor.RedactionPolicy",
        "SnapshotPlan" => "std.vm.PersistentActor.SnapshotPlan",
        "ReplayPlan" => "std.vm.PersistentActor.ReplayPlan",
        "ResourceCheckpoint" => "std.vm.PersistentActor.ResourceCheckpoint",
        "ResourceRestorePlan" => "std.vm.PersistentActor.ResourceRestorePlan",
        "MailboxCheckpoint" => "std.vm.PersistentActor.MailboxCheckpoint",
        "MailboxRestorePlan" => "std.vm.PersistentActor.MailboxRestorePlan",
        "MigrationRollbackPlan" => "std.vm.PersistentActor.MigrationRollbackPlan",
        "PackageMigrationRegistration" => "std.vm.PersistentActor.PackageMigrationRegistration",
        "TimerCheckpoint" => "std.vm.PersistentActor.TimerCheckpoint",
        "TimerRestorePlan" => "std.vm.PersistentActor.TimerRestorePlan",
        "PackageStoreBinding" => "std.vm.PersistentActor.PackageStoreBinding",
        _ => PREFIX,
    }
}

fn unsupported(operation: &str, arity: usize) -> VmRuntimeResult<ReplValue> {
    Err(unsupported_message(operation, arity).into())
}

fn unsupported_message(operation: &str, arity: usize) -> String {
    format!(
        "error[vm.persistent_actor.operation]: unsupported operation or arity `{operation}`/{arity}"
    )
}

#[cfg(test)]
#[path = "persistent_actor_test.rs"]
mod tests;
