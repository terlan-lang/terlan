use super::*;

/// Verifies embedded std summaries include the VM distributed storage contract.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when the VM distributed storage source façade is available
///   through the embedded summary list with opaque descriptors and lifecycle
///   receiver methods.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so distributed storage source
///   scenarios can typecheck without file-system summary discovery.
#[test]
fn embedded_std_interfaces_include_vm_distributed_storage_contract() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let storage = interfaces
        .get("std.vm.DistributedStorage")
        .expect("embedded VM DistributedStorage interface");
    assert!(storage.opaque_types.contains("Mode"));
    assert!(storage.opaque_types.contains("Policy"));
    assert!(storage.opaque_types.contains("Adapter"));
    assert!(storage.opaque_types.contains("AtomicAppendProof"));
    assert!(storage.opaque_types.contains("SnapshotIsolationProof"));
    assert!(storage.opaque_types.contains("DurableFlushProof"));
    assert!(storage.opaque_types.contains("TransactionalBatchProof"));
    assert!(storage.opaque_types.contains("SchemaMigrationProof"));
    assert!(storage
        .opaque_types
        .contains("ResourceHandleValidationProof"));
    assert!(storage.opaque_types.contains("CompareAndSwapToken"));
    assert!(storage.opaque_types.contains("Snapshot"));
    assert!(storage.opaque_types.contains("Outcome"));
    assert!(storage
        .functions
        .contains_key(&("local_only".to_string(), 0)));
    assert!(storage.functions.contains_key(&("durable".to_string(), 0)));
    assert!(storage.functions.contains_key(&("cluster".to_string(), 0)));
    assert!(storage
        .functions
        .contains_key(&("force_local".to_string(), 0)));
    assert!(storage.functions.contains_key(&("policy".to_string(), 3)));
    assert!(storage
        .functions
        .contains_key(&("policy_name".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("policy_mode_kind".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("policy_available".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("policy_can_cluster_replicate".to_string(), 1)));
    assert!(storage.functions.contains_key(&("adapter".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("checkpoint".to_string(), 3)));
    assert!(storage
        .functions
        .contains_key(&("proof_sequence".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("isolation_checkpoint_id".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("isolation_sequence".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("isolation_checksum".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("durable_flush_sequence".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("batch_first_sequence".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("batch_last_sequence".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("batch_committed_count".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("schema_version".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("schema_sequence".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("resource_handle_count".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("resource_handle_sequence".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("expected_sequence".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("actual_sequence".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("expected_schema".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("actual_schema".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("missing_resource_handle".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("validated_resource_count".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("loaded_snapshot".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("is_failure".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("is_success".to_string(), 1)));
    assert!(storage.functions.contains_key(&("kind".to_string(), 1)));
    assert!(storage.functions.contains_key(&("reason".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("retained_snapshots".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("requires_recovery".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("recovery_action".to_string(), 1)));
    assert!(storage.functions.contains_key(&("sequence".to_string(), 1)));
    assert!(storage.functions.contains_key(&("checksum".to_string(), 1)));
    assert!(storage
        .functions
        .contains_key(&("expected_checksum".to_string(), 1)));

    for method in [
        "open",
        "append",
        "flush",
        "compact",
        "load_snapshot",
        "close",
        "require_atomic_append",
        "atomic_append_proof",
        "require_snapshot_isolation",
        "snapshot_isolation_proof",
        "require_durable_flush",
        "durable_flush_proof",
        "require_transactional_batch",
        "transactional_batch_proof",
        "transactional_batch_append",
        "require_schema_migration",
        "schema_migration_proof",
        "migrate_schema",
        "require_resource_handle_validation",
        "resource_handle_validation_proof",
        "register_resource_handle",
        "validate_resource_handles",
        "require_cluster_replication",
        "compare_and_swap_token",
        "compare_and_swap_append",
        "policy_name",
        "policy_mode_kind",
        "policy_available",
        "can_cluster_replicate",
    ] {
        let function = storage
            .function_overloads
            .values()
            .flatten()
            .find(|function| function.name == method && function.receiver_method)
            .unwrap_or_else(|| panic!("DistributedStorage.{method} receiver method"));
        assert!(function.receiver_method);
        assert!(!function.receiver_mutable);
    }
}

/// Verifies embedded std summaries include the VM model-sync contract.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when the source-facing optimistic concurrency API is
///   available through the embedded summary list with typed descriptors.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so model-sync source scenarios
///   can typecheck without file-system summary discovery.
#[test]
fn embedded_std_interfaces_include_vm_model_sync_contract() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let model_sync = interfaces
        .get("std.vm.ModelSync")
        .expect("embedded VM ModelSync interface");
    assert!(model_sync.opaque_types.contains("Key"));
    assert!(model_sync.opaque_types.contains("Version"));
    assert!(model_sync.opaque_types.contains("Write"));
    assert!(model_sync.opaque_types.contains("Delete"));
    assert!(model_sync.opaque_types.contains("Conflict"));
    assert!(model_sync.opaque_types.contains("Capability"));
    assert!(model_sync.opaque_types.contains("AdapterContract"));
    assert!(model_sync.opaque_types.contains("PersistentActorAdapter"));
    assert!(model_sync.opaque_types.contains("PackageStoreAdapter"));
    assert!(model_sync.functions.contains_key(&("key".to_string(), 2)));
    assert!(model_sync
        .functions
        .contains_key(&("version".to_string(), 2)));
    assert!(model_sync
        .functions
        .contains_key(&("initial_version".to_string(), 1)));
    assert!(model_sync
        .functions
        .contains_key(&("next_version".to_string(), 2)));
    assert!(model_sync.functions.contains_key(&("write".to_string(), 4)));
    assert!(model_sync
        .functions
        .contains_key(&("delete".to_string(), 3)));
    assert!(model_sync
        .functions
        .contains_key(&("conflict".to_string(), 3)));
    assert!(model_sync.functions.contains_key(&("stale".to_string(), 2)));
    assert!(model_sync
        .functions
        .contains_key(&("can_apply".to_string(), 2)));
    assert!(model_sync
        .functions
        .contains_key(&("capability".to_string(), 1)));
    assert!(model_sync
        .functions
        .contains_key(&("adapter_contract".to_string(), 3)));
    assert!(model_sync
        .functions
        .contains_key(&("persistent_actor_adapter".to_string(), 2)));
    assert!(model_sync
        .functions
        .contains_key(&("package_store_adapter".to_string(), 2)));
}

/// Verifies embedded std summaries include the VM persistent actor contract.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when typed actor and snapshot schema descriptors are
///   available through the embedded summary list.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so persistent actor source
///   tests can typecheck without file-system summary discovery.
#[test]
fn embedded_std_interfaces_include_vm_persistent_actor_contract() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let persistent_actor = interfaces
        .get("std.vm.PersistentActor")
        .expect("embedded VM PersistentActor interface");
    assert!(persistent_actor.opaque_types.contains("ActorId"));
    assert!(persistent_actor.opaque_types.contains("SchemaId"));
    assert!(persistent_actor.opaque_types.contains("SchemaDeclaration"));
    assert!(persistent_actor
        .opaque_types
        .contains("ActorFamilyRetentionDefaults"));
    assert!(persistent_actor.opaque_types.contains("AuditRetentionPlan"));
    assert!(persistent_actor
        .opaque_types
        .contains("DurableAdapterSchemaMetadata"));
    assert!(persistent_actor
        .opaque_types
        .contains("EventVariantSchemaId"));
    assert!(persistent_actor.opaque_types.contains("RetentionPolicy"));
    assert!(persistent_actor.opaque_types.contains("RedactionPolicy"));
    assert!(persistent_actor.opaque_types.contains("SnapshotPlan"));
    assert!(persistent_actor.traits.contains_key("Persistable"));
    assert!(persistent_actor.opaque_types.contains("ReplayPlan"));
    assert!(persistent_actor.opaque_types.contains("ResourceCheckpoint"));
    assert!(persistent_actor
        .opaque_types
        .contains("ResourceRestorePlan"));
    assert!(persistent_actor.opaque_types.contains("MailboxCheckpoint"));
    assert!(persistent_actor.opaque_types.contains("MailboxRestorePlan"));
    assert!(persistent_actor
        .opaque_types
        .contains("MigrationRollbackPlan"));
    assert!(persistent_actor
        .opaque_types
        .contains("ModelSyncRetentionContinuityPlan"));
    assert!(persistent_actor
        .opaque_types
        .contains("PackageMigrationRegistration"));
    assert!(persistent_actor
        .opaque_types
        .contains("PackageRetentionPolicyBinding"));
    assert!(persistent_actor.opaque_types.contains("TimerCheckpoint"));
    assert!(persistent_actor.opaque_types.contains("TimerRestorePlan"));
    assert!(persistent_actor
        .opaque_types
        .contains("PackageStoreBinding"));
    assert!(persistent_actor
        .functions
        .contains_key(&("actor_id".to_string(), 2)));
    assert!(persistent_actor
        .functions
        .contains_key(&("audit_retention".to_string(), 3)));
    assert!(persistent_actor
        .functions
        .contains_key(&("schema_id".to_string(), 2)));
    assert!(persistent_actor
        .functions
        .contains_key(&("schema".to_string(), 4)));
    assert!(persistent_actor
        .functions
        .contains_key(&("event_variant_schema".to_string(), 5)));
    assert!(persistent_actor
        .functions
        .contains_key(&("durable_adapter_schema".to_string(), 5)));
    assert!(persistent_actor
        .functions
        .contains_key(&("retention_policy".to_string(), 3)));
    assert!(persistent_actor
        .functions
        .contains_key(&("family_retention_defaults".to_string(), 3)));
    assert!(persistent_actor
        .functions
        .contains_key(&("redaction_policy".to_string(), 3)));
    let snapshot = persistent_actor
        .functions
        .get(&("snapshot".to_string(), 4))
        .expect("PersistentActor.snapshot function");
    assert_eq!(snapshot.generic_params, vec!["State"]);
    assert_eq!(snapshot.generic_bounds, vec!["Persistable[State]"]);
    assert!(persistent_actor
        .functions
        .contains_key(&("replay".to_string(), 2)));
    assert!(persistent_actor
        .functions
        .contains_key(&("resource_checkpoint".to_string(), 2)));
    assert!(persistent_actor
        .functions
        .contains_key(&("restore_resource".to_string(), 2)));
    assert!(persistent_actor
        .functions
        .contains_key(&("mailbox_checkpoint".to_string(), 2)));
    assert!(persistent_actor
        .functions
        .contains_key(&("restore_mailbox".to_string(), 2)));
    assert!(persistent_actor
        .functions
        .contains_key(&("migration_rollback".to_string(), 5)));
    assert!(persistent_actor
        .functions
        .contains_key(&("model_sync_retention_continuity".to_string(), 4)));
    assert!(persistent_actor
        .functions
        .contains_key(&("register_package_migration".to_string(), 5)));
    assert!(persistent_actor
        .functions
        .contains_key(&("timer_checkpoint".to_string(), 3)));
    assert!(persistent_actor
        .functions
        .contains_key(&("restore_timer".to_string(), 2)));
    assert!(persistent_actor
        .functions
        .contains_key(&("package_store".to_string(), 3)));
    assert!(persistent_actor
        .functions
        .contains_key(&("package_retention_policy".to_string(), 3)));
    assert!(persistent_actor
        .functions
        .contains_key(&("compatible_schema".to_string(), 2)));
}

/// Verifies embedded std summaries include the core effect descriptor contract.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when `std.core.Effect` is available through embedded
///   summaries with the completed descriptor type and core composition
///   helpers.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so purity and comprehension
///   design slices can typecheck `Effect[T]` without scanning std summaries.
#[test]
fn embedded_std_interfaces_include_core_effect_contract() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let effect = interfaces
        .get("std.core.Effect")
        .expect("embedded std.core.Effect interface");
    assert!(effect.public_types.contains("Pure"));
    assert!(effect.public_types.contains("Effect"));
    assert!(effect.type_bodies.contains_key("Pure"));
    assert!(effect.type_bodies.contains_key("Effect"));
    assert!(effect.functions.contains_key(&("succeed".to_string(), 1)));
    assert!(effect.functions.contains_key(&("map".to_string(), 2)));
    assert!(effect.functions.contains_key(&("flat_map".to_string(), 2)));
    assert!(effect.functions.contains_key(&("run".to_string(), 1)));
}

/// Verifies embedded std summaries include the core guard-result contract.
///
/// Inputs:
/// - Compiler-embedded std interface summaries.
///
/// Output:
/// - Test passes when `std.core.GuardResult` is available through embedded
///   summaries with pure guard-result constructors and composition helpers.
///
/// Transformation:
/// - Exercises normal embedded summary parsing so comprehension guard planning
///   can typecheck the source-facing guard-result contract without scanning
///   std summaries.
#[test]
fn embedded_std_interfaces_include_core_guard_result_contract() {
    let mut interfaces = HashMap::new();

    load_embedded_std_interfaces(&mut interfaces);

    let guard_result = interfaces
        .get("std.core.GuardResult")
        .expect("embedded std.core.GuardResult interface");
    assert!(guard_result.public_types.contains("Pure"));
    assert!(guard_result.public_types.contains("Completed"));
    assert!(guard_result.type_bodies.contains_key("Pure"));
    assert!(guard_result.type_bodies.contains_key("Completed"));
    let guard_result_trait = guard_result
        .traits
        .get("GuardResult")
        .expect("GuardResult trait contract");
    assert!(guard_result_trait.methods.contains_key("into_guard"));
    assert!(guard_result
        .functions
        .contains_key(&("from_bool".to_string(), 1)));
    assert!(guard_result
        .functions
        .contains_key(&("accept".to_string(), 0)));
    assert!(guard_result
        .functions
        .contains_key(&("reject".to_string(), 0)));
    assert!(guard_result
        .functions
        .contains_key(&("value".to_string(), 1)));
    assert!(guard_result
        .functions
        .contains_key(&("both".to_string(), 2)));
    assert!(guard_result
        .functions
        .contains_key(&("either".to_string(), 2)));
}
