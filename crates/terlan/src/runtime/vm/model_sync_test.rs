use super::{
    invalidate_live_template_subscribers_from_model_events,
    project_model_sync_row_from_adapter_fields, replay_dom_patches_for_template_bindings,
    validate_model_sync_permission_drift, validate_non_postgres_model_sync_adapter_contracts,
    ReplValue, VmDomPatchOperationKind, VmDomPatchTemplateBinding, VmInMemoryModelSyncStore,
    VmModelSyncAdapterCapability, VmModelSyncAdapterContract, VmModelSyncChange,
    VmModelSyncChangeKind, VmModelSyncFieldPermission, VmModelSyncKey, VmModelSyncOutcome,
    VmModelSyncPermissionOperation, VmModelSyncPermissionPolicy, VmModelSyncProjectedFieldType,
    VmModelSyncRowFieldProjection, VmModelSyncRowProjection, VmModelSyncStoreAdapter,
    VmModelSyncTemplateInvalidation, VmModelSyncTemplateSubscription, VmModelSyncVersion,
    VmSyncableModelDeclaration,
};

use std::collections::BTreeMap;

#[test]
fn vm_model_sync_store_applies_updates_and_exports_deterministic_snapshot() {
    let mut store = VmInMemoryModelSyncStore::new();
    let alice = key("User", "alice");
    let bob = key("User", "bob");

    let created = store.put(
        bob.clone(),
        None,
        ReplValue::String("Bob".to_string()),
        version(1, "node-a"),
    );
    assert_change(created, VmModelSyncChangeKind::Created, 1);
    let created = store.put(
        alice.clone(),
        None,
        ReplValue::String("Alice".to_string()),
        version(1, "node-a"),
    );
    assert_change(created, VmModelSyncChangeKind::Created, 2);
    let updated = store.put(
        alice.clone(),
        Some(version(1, "node-a")),
        ReplValue::String("Alice Smith".to_string()),
        version(2, "node-a"),
    );
    assert_change(updated, VmModelSyncChangeKind::Updated, 3);

    let snapshot = store.export_snapshot();

    assert_eq!(snapshot[0].key, alice);
    assert_eq!(snapshot[1].key, bob);
    assert_eq!(
        store.get(&snapshot[0].key).map(|row| &row.value),
        Some(&ReplValue::String("Alice Smith".to_string()))
    );
}

#[test]
fn vm_model_sync_store_rejects_stale_versions_without_mutation() {
    let mut store = VmInMemoryModelSyncStore::new();
    let key = key("Counter", "one");
    store.put(key.clone(), None, ReplValue::Int(1), version(5, "node-a"));

    let stale = store.put(
        key.clone(),
        Some(version(4, "node-b")),
        ReplValue::Int(2),
        version(6, "node-b"),
    );

    assert_eq!(
        stale,
        VmModelSyncOutcome::Conflict {
            key: key.clone(),
            current_version: version(5, "node-a"),
            incoming_version: version(6, "node-b"),
        }
    );
    assert_eq!(
        store.get(&key).map(|row| &row.value),
        Some(&ReplValue::Int(1))
    );
    assert_eq!(store.changes_since(0).len(), 1);
}

#[test]
fn vm_model_sync_store_emits_delete_tombstones_and_change_streams() {
    let mut store = VmInMemoryModelSyncStore::new();
    let key = key("Session", "s1");
    store.put(
        key.clone(),
        None,
        ReplValue::String("open".to_string()),
        version(1, "node-a"),
    );

    let deleted = store.delete(key.clone(), version(1, "node-a"), version(2, "node-a"));

    assert_change(deleted, VmModelSyncChangeKind::Deleted, 2);
    assert_eq!(store.get(&key), None);
    let changes = store.changes_since(1);
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].kind, VmModelSyncChangeKind::Deleted);
    assert_eq!(changes[0].value, None);
}

#[test]
fn vm_model_sync_store_rejects_invalid_keys_and_versions() {
    assert_eq!(
        VmModelSyncKey::new("", "id").expect_err("empty model should fail"),
        "error[vm_model_sync]: model name must be non-empty"
    );
    assert_eq!(
        VmModelSyncKey::new("User", "").expect_err("empty id should fail"),
        "error[vm_model_sync]: model id must be non-empty"
    );
    assert_eq!(
        VmModelSyncVersion::new(0, "node-a").expect_err("zero sequence should fail"),
        "error[vm_model_sync]: model version sequence must be non-zero"
    );
    assert_eq!(
        VmModelSyncVersion::new(1, "").expect_err("empty writer should fail"),
        "error[vm_model_sync]: model version writer id must be non-empty"
    );
}

#[test]
fn vm_model_sync_declares_syncable_model_without_orm_identity_map() {
    let contract = portable_adapter_contract("users-memory", "vm");

    let declaration =
        VmSyncableModelDeclaration::new("User", contract.clone(), "User").expect("declaration");

    assert_eq!(declaration.name, "User");
    assert_eq!(declaration.key_model, "User");
    assert_eq!(declaration.adapter_contract, contract);
}

#[test]
fn vm_model_sync_rejects_invalid_syncable_model_declarations() {
    let contract = portable_adapter_contract("users-memory", "vm");

    assert_eq!(
        VmSyncableModelDeclaration::new("", contract.clone(), "User")
            .expect_err("empty model should fail"),
        "error[vm_model_sync]: syncable model name must be non-empty"
    );
    assert_eq!(
        VmSyncableModelDeclaration::new("User", contract.clone(), "")
            .expect_err("empty key model should fail"),
        "error[vm_model_sync]: syncable model key model must be non-empty"
    );
    assert_eq!(
        VmSyncableModelDeclaration::new("User", contract, "Account")
            .expect_err("mismatched key model should fail"),
        "error[vm_model_sync]: syncable model `User` cannot use key model `Account`"
    );
}

#[test]
fn vm_model_sync_replays_dom_patches_against_typed_template_bindings() {
    let mut store = VmInMemoryModelSyncStore::new();
    let key = key("User", "alice");
    let change = match store.put(
        key,
        None,
        ReplValue::Record {
            name: "User".to_string(),
            fields: vec![
                ("name".to_string(), ReplValue::String("Alice".to_string())),
                ("score".to_string(), ReplValue::Int(7)),
            ],
        },
        version(1, "node-a"),
    ) {
        VmModelSyncOutcome::Applied(change) => change,
        other => panic!("expected applied model change, got {other:?}"),
    };
    let bindings = vec![
        binding("User", "name", "#user-{id} .name"),
        binding("User", "score", "#user-{id} .score"),
        binding("Session", "name", "#session-{id} .name"),
    ];

    let patches =
        replay_dom_patches_for_template_bindings(&change, &bindings).expect("replay patches");

    assert_eq!(patches.len(), 2);
    assert_eq!(patches[0].sequence, 1);
    assert_eq!(patches[0].selector, "#user-alice .name");
    assert_eq!(patches[0].kind, VmDomPatchOperationKind::ReplaceText);
    assert_eq!(patches[0].value.as_deref(), Some("Alice"));
    assert_eq!(patches[1].selector, "#user-alice .score");
    assert_eq!(patches[1].value.as_deref(), Some("7"));
}

#[test]
fn vm_model_sync_replays_delete_as_binding_removal_patch() {
    let mut store = VmInMemoryModelSyncStore::new();
    let key = key("User", "alice");
    store.put(
        key.clone(),
        None,
        ReplValue::Record {
            name: "User".to_string(),
            fields: vec![("name".to_string(), ReplValue::String("Alice".to_string()))],
        },
        version(1, "node-a"),
    );
    let change = match store.delete(key, version(1, "node-a"), version(2, "node-a")) {
        VmModelSyncOutcome::Deleted(change) => change,
        other => panic!("expected deleted model change, got {other:?}"),
    };

    let patches = replay_dom_patches_for_template_bindings(
        &change,
        &[binding("User", "name", "#user-{id} .name")],
    )
    .expect("replay delete patch");

    assert_eq!(patches.len(), 1);
    assert_eq!(patches[0].sequence, 2);
    assert_eq!(patches[0].selector, "#user-alice .name");
    assert_eq!(patches[0].kind, VmDomPatchOperationKind::RemoveBinding);
    assert_eq!(patches[0].value, None);
}

#[test]
fn vm_model_sync_rejects_dom_patch_replay_for_missing_template_binding_field() {
    let mut store = VmInMemoryModelSyncStore::new();
    let change = match store.put(
        key("User", "alice"),
        None,
        ReplValue::Record {
            name: "User".to_string(),
            fields: vec![("name".to_string(), ReplValue::String("Alice".to_string()))],
        },
        version(1, "node-a"),
    ) {
        VmModelSyncOutcome::Applied(change) => change,
        other => panic!("expected applied model change, got {other:?}"),
    };

    let error = replay_dom_patches_for_template_bindings(
        &change,
        &[binding("User", "missing", "#user-{id} .missing")],
    )
    .expect_err("missing field should fail");

    assert_eq!(
        error,
        "error[vm_dom_patch]: model `User` field `missing` is missing"
    );
}

#[test]
fn vm_model_sync_invalidates_live_template_subscribers_from_committed_events() {
    let mut store = VmInMemoryModelSyncStore::new();
    let key = key("User", "alice");
    let created = match store.put(
        key.clone(),
        None,
        ReplValue::Record {
            name: "User".to_string(),
            fields: vec![("name".to_string(), ReplValue::String("Alice".to_string()))],
        },
        version(1, "node-a"),
    ) {
        VmModelSyncOutcome::Applied(change) => change,
        other => panic!("expected created model change, got {other:?}"),
    };
    let updated = match store.put(
        key.clone(),
        Some(version(1, "node-a")),
        ReplValue::Record {
            name: "User".to_string(),
            fields: vec![(
                "name".to_string(),
                ReplValue::String("Alice Smith".to_string()),
            )],
        },
        version(2, "node-a"),
    ) {
        VmModelSyncOutcome::Applied(change) => change,
        other => panic!("expected updated model change, got {other:?}"),
    };
    let deleted = match store.delete(key, version(2, "node-a"), version(3, "node-a")) {
        VmModelSyncOutcome::Deleted(change) => change,
        other => panic!("expected deleted model change, got {other:?}"),
    };
    let subscriptions = vec![
        model_subscription("User", "summary", "user.summary"),
        model_subscription("User", "detail", "user.detail"),
        model_subscription("Session", "session", "session.summary"),
    ];

    let invalidations = invalidate_live_template_subscribers_from_model_events(
        &[created, updated, deleted],
        &subscriptions,
    );

    assert_eq!(
        invalidations,
        vec![
            invalidation(1, "User", "alice", "summary", "user.summary", "created"),
            invalidation(1, "User", "alice", "detail", "user.detail", "created"),
            invalidation(2, "User", "alice", "summary", "user.summary", "updated"),
            invalidation(2, "User", "alice", "detail", "user.detail", "updated"),
            invalidation(3, "User", "alice", "summary", "user.summary", "deleted"),
            invalidation(3, "User", "alice", "detail", "user.detail", "deleted"),
        ]
    );
}

#[test]
fn vm_model_sync_rejects_invalid_live_template_subscription_identity() {
    assert_eq!(
        VmModelSyncTemplateSubscription::new("", "subscriber", "template")
            .expect_err("empty model should fail"),
        "error[vm_model_sync]: template subscription model must be non-empty"
    );
    assert_eq!(
        VmModelSyncTemplateSubscription::new("User", "", "template")
            .expect_err("empty subscriber should fail"),
        "error[vm_model_sync]: template subscription subscriber id must be non-empty"
    );
    assert_eq!(
        VmModelSyncTemplateSubscription::new("User", "subscriber", "")
            .expect_err("empty template should fail"),
        "error[vm_model_sync]: template subscription template id must be non-empty"
    );
}

#[test]
fn vm_model_sync_projects_adapter_row_into_typed_model_row() {
    let projection = row_projection(
        "User",
        "id",
        "version_sequence",
        "version_writer",
        vec![
            row_field(
                "display_name",
                "name",
                VmModelSyncProjectedFieldType::String,
            ),
            row_field("score", "score", VmModelSyncProjectedFieldType::Int),
            row_field("active", "active", VmModelSyncProjectedFieldType::Bool),
            row_field("role", "role", VmModelSyncProjectedFieldType::Atom),
        ],
    );
    let row = adapter_row(&[
        ("id", ReplValue::String("alice".to_string())),
        ("version_sequence", ReplValue::Int(3)),
        ("version_writer", ReplValue::String("node-a".to_string())),
        ("display_name", ReplValue::String("Alice".to_string())),
        ("score", ReplValue::Int(7)),
        ("active", ReplValue::Bool(true)),
        ("role", ReplValue::Atom("admin".to_string())),
    ]);

    let projected =
        project_model_sync_row_from_adapter_fields(&projection, &row).expect("row should project");

    assert_eq!(projected.key, key("User", "alice"));
    assert_eq!(projected.version, version(3, "node-a"));
    assert_eq!(projected.value, user_record_full("Alice", 7, true, "admin"));
}

#[test]
fn vm_model_sync_row_projection_rejects_missing_adapter_field() {
    let projection = row_projection(
        "User",
        "id",
        "version_sequence",
        "version_writer",
        vec![row_field(
            "score",
            "score",
            VmModelSyncProjectedFieldType::Int,
        )],
    );
    let row = adapter_row(&[
        ("id", ReplValue::String("alice".to_string())),
        ("version_sequence", ReplValue::Int(3)),
        ("version_writer", ReplValue::String("node-a".to_string())),
    ]);

    let error = project_model_sync_row_from_adapter_fields(&projection, &row)
        .expect_err("missing field should fail");

    assert_eq!(error, "error[vm_model_sync]: row field `score` is missing");
}

#[test]
fn vm_model_sync_row_projection_rejects_type_mismatch() {
    let projection = row_projection(
        "User",
        "id",
        "version_sequence",
        "version_writer",
        vec![row_field(
            "score",
            "score",
            VmModelSyncProjectedFieldType::Int,
        )],
    );
    let row = adapter_row(&[
        ("id", ReplValue::String("alice".to_string())),
        ("version_sequence", ReplValue::Int(3)),
        ("version_writer", ReplValue::String("node-a".to_string())),
        ("score", ReplValue::String("seven".to_string())),
    ]);

    let error = project_model_sync_row_from_adapter_fields(&projection, &row)
        .expect_err("type mismatch should fail");

    assert_eq!(
        error,
        "error[vm_model_sync]: row field `score` expected `Int`"
    );
}

#[test]
fn vm_model_sync_row_projection_rejects_invalid_version_sequence() {
    let projection = row_projection(
        "User",
        "id",
        "version_sequence",
        "version_writer",
        vec![row_field(
            "score",
            "score",
            VmModelSyncProjectedFieldType::Int,
        )],
    );
    let row = adapter_row(&[
        ("id", ReplValue::String("alice".to_string())),
        ("version_sequence", ReplValue::Int(0)),
        ("version_writer", ReplValue::String("node-a".to_string())),
        ("score", ReplValue::Int(7)),
    ]);

    let error = project_model_sync_row_from_adapter_fields(&projection, &row)
        .expect_err("zero version should fail");

    assert_eq!(
        error,
        "error[vm_model_sync]: row field `version_sequence` expected positive `Int`"
    );
}

#[test]
fn vm_model_sync_row_projection_rejects_duplicate_model_fields() {
    let error = VmModelSyncRowProjection::new(
        "User",
        "id",
        "version_sequence",
        "version_writer",
        vec![
            row_field("first_name", "name", VmModelSyncProjectedFieldType::String),
            row_field(
                "display_name",
                "name",
                VmModelSyncProjectedFieldType::String,
            ),
        ],
    )
    .expect_err("duplicate model field should fail");

    assert_eq!(
        error,
        "error[vm_model_sync]: row projection model field `name` is duplicated"
    );
}

#[test]
fn vm_model_sync_permission_policy_accepts_allowed_model_and_field_changes() {
    let changes = vec![
        model_change(
            1,
            "User",
            "alice",
            VmModelSyncChangeKind::Created,
            Some(user_record("Alice", 7)),
        ),
        model_change(2, "User", "alice", VmModelSyncChangeKind::Deleted, None),
    ];
    let policies = vec![permission_policy(
        "User",
        vec![
            VmModelSyncPermissionOperation::Read,
            VmModelSyncPermissionOperation::Write,
            VmModelSyncPermissionOperation::Delete,
            VmModelSyncPermissionOperation::Subscribe,
        ],
        vec![
            field_permission("name", vec![VmModelSyncPermissionOperation::Write]),
            field_permission("score", vec![VmModelSyncPermissionOperation::Write]),
        ],
    )];

    validate_model_sync_permission_drift(&changes, &policies)
        .expect("allowed changes should not drift permissions");
}

#[test]
fn vm_model_sync_permission_policy_rejects_missing_model_policy() {
    let changes = vec![model_change(
        1,
        "User",
        "alice",
        VmModelSyncChangeKind::Created,
        Some(user_record("Alice", 7)),
    )];

    let error = validate_model_sync_permission_drift(&changes, &[])
        .expect_err("missing model policy should fail");

    assert_eq!(
        error,
        "error[vm_model_sync]: model `User` has no permission policy"
    );
}

#[test]
fn vm_model_sync_permission_policy_rejects_field_level_drift() {
    let changes = vec![model_change(
        1,
        "User",
        "alice",
        VmModelSyncChangeKind::Updated,
        Some(user_record("Alice", 7)),
    )];
    let policies = vec![permission_policy(
        "User",
        vec![VmModelSyncPermissionOperation::Write],
        vec![field_permission(
            "name",
            vec![VmModelSyncPermissionOperation::Write],
        )],
    )];

    let error = validate_model_sync_permission_drift(&changes, &policies)
        .expect_err("missing field permission should fail");

    assert_eq!(
        error,
        "error[vm_model_sync]: model `User` field `score` denies `Write`"
    );
}

#[test]
fn vm_model_sync_permission_policy_rejects_denied_delete_operation() {
    let changes = vec![model_change(
        1,
        "User",
        "alice",
        VmModelSyncChangeKind::Deleted,
        None,
    )];
    let policies = vec![permission_policy(
        "User",
        vec![VmModelSyncPermissionOperation::Write],
        vec![field_permission(
            "name",
            vec![VmModelSyncPermissionOperation::Write],
        )],
    )];

    let error = validate_model_sync_permission_drift(&changes, &policies)
        .expect_err("denied delete should fail");

    assert_eq!(error, "error[vm_model_sync]: model `User` denies `Delete`");
}

#[test]
fn vm_model_sync_validates_non_postgres_adapter_portability_contracts() {
    let mut postgres_capabilities = portable_capabilities().to_vec();
    postgres_capabilities.extend([
        VmModelSyncAdapterCapability::TypedRowDecode,
        VmModelSyncAdapterCapability::TransactionRollback,
    ]);
    let contracts = vec![
        adapter_contract(
            "vm-in-memory",
            "vm-memory",
            portable_capabilities().to_vec(),
        ),
        adapter_contract(
            "distributed-state",
            "vm-distributed-state",
            portable_capabilities().to_vec(),
        ),
        adapter_contract("postgres", "postgres", postgres_capabilities),
    ];

    validate_non_postgres_model_sync_adapter_contracts(&contracts)
        .expect("non-Postgres contracts should be portable");
}

#[test]
fn vm_model_sync_rejects_non_postgres_adapter_missing_portable_capability() {
    let mut capabilities = portable_capabilities().to_vec();
    capabilities.retain(|capability| *capability != VmModelSyncAdapterCapability::ChangeStream);
    let contracts = vec![adapter_contract("package-store", "package", capabilities)];

    let error = validate_non_postgres_model_sync_adapter_contracts(&contracts)
        .expect_err("missing portable capability should fail");

    assert_eq!(
        error,
        "error[vm_model_sync]: adapter `package-store` is missing portable capability `ChangeStream`"
    );
}

#[test]
fn vm_model_sync_rejects_non_postgres_adapter_leaking_postgres_capability() {
    let mut capabilities = portable_capabilities().to_vec();
    capabilities.push(VmModelSyncAdapterCapability::TypedRowDecode);
    let contracts = vec![adapter_contract("actor-store", "actor", capabilities)];

    let error = validate_non_postgres_model_sync_adapter_contracts(&contracts)
        .expect_err("Postgres-only capability should fail");

    assert_eq!(
        error,
        "error[vm_model_sync]: adapter `actor-store` leaks Postgres-only capability `TypedRowDecode`"
    );
}

fn assert_change(outcome: VmModelSyncOutcome, kind: VmModelSyncChangeKind, sequence: u64) {
    match outcome {
        VmModelSyncOutcome::Applied(change) | VmModelSyncOutcome::Deleted(change) => {
            assert_eq!(change.kind, kind);
            assert_eq!(change.kind.kind(), kind.kind());
            assert_eq!(change.sequence, sequence);
        }
        other => panic!("expected committed model-sync change, got {other:?}"),
    }
}

fn portable_capabilities() -> &'static [VmModelSyncAdapterCapability] {
    &[
        VmModelSyncAdapterCapability::TypedKey,
        VmModelSyncAdapterCapability::OptimisticVersion,
        VmModelSyncAdapterCapability::Put,
        VmModelSyncAdapterCapability::Delete,
        VmModelSyncAdapterCapability::Snapshot,
        VmModelSyncAdapterCapability::ChangeStream,
    ]
}

fn permission_policy(
    model: &str,
    operations: Vec<VmModelSyncPermissionOperation>,
    fields: Vec<VmModelSyncFieldPermission>,
) -> VmModelSyncPermissionPolicy {
    VmModelSyncPermissionPolicy::new(model, operations, fields)
        .expect("permission policy should be valid")
}

fn field_permission(
    field: &str,
    operations: Vec<VmModelSyncPermissionOperation>,
) -> VmModelSyncFieldPermission {
    VmModelSyncFieldPermission::new(field, operations).expect("field permission should be valid")
}

fn row_projection(
    model: &str,
    id_field: &str,
    version_sequence_field: &str,
    version_writer_field: &str,
    fields: Vec<VmModelSyncRowFieldProjection>,
) -> VmModelSyncRowProjection {
    VmModelSyncRowProjection::new(
        model,
        id_field,
        version_sequence_field,
        version_writer_field,
        fields,
    )
    .expect("row projection should be valid")
}

fn row_field(
    row_field: &str,
    model_field: &str,
    field_type: VmModelSyncProjectedFieldType,
) -> VmModelSyncRowFieldProjection {
    VmModelSyncRowFieldProjection::new(row_field, model_field, field_type)
        .expect("row field projection should be valid")
}

fn adapter_row(fields: &[(&str, ReplValue)]) -> BTreeMap<String, ReplValue> {
    fields
        .iter()
        .map(|(field, value)| ((*field).to_string(), value.clone()))
        .collect()
}

fn adapter_contract(
    name: &'static str,
    storage_family: &'static str,
    capabilities: Vec<VmModelSyncAdapterCapability>,
) -> VmModelSyncAdapterContract {
    VmModelSyncAdapterContract::new(name, storage_family, capabilities)
        .expect("adapter contract should be valid")
}

fn portable_adapter_contract(
    name: &'static str,
    storage_family: &'static str,
) -> VmModelSyncAdapterContract {
    adapter_contract(
        name,
        storage_family,
        vec![
            VmModelSyncAdapterCapability::TypedKey,
            VmModelSyncAdapterCapability::OptimisticVersion,
            VmModelSyncAdapterCapability::Put,
            VmModelSyncAdapterCapability::Delete,
            VmModelSyncAdapterCapability::Snapshot,
            VmModelSyncAdapterCapability::ChangeStream,
        ],
    )
}

fn key(model: &str, id: &str) -> VmModelSyncKey {
    VmModelSyncKey::new(model, id).expect("key should be valid")
}

fn version(sequence: u64, writer_id: &str) -> VmModelSyncVersion {
    VmModelSyncVersion::new(sequence, writer_id).expect("version should be valid")
}

fn binding(model: &str, field: &str, selector: &str) -> VmDomPatchTemplateBinding {
    VmDomPatchTemplateBinding::new(model, field, selector).expect("binding should be valid")
}

fn user_record(name: &str, score: i64) -> ReplValue {
    ReplValue::Record {
        name: "User".to_string(),
        fields: vec![
            ("name".to_string(), ReplValue::String(name.to_string())),
            ("score".to_string(), ReplValue::Int(score)),
        ],
    }
}

fn user_record_full(name: &str, score: i64, active: bool, role: &str) -> ReplValue {
    ReplValue::Record {
        name: "User".to_string(),
        fields: vec![
            ("name".to_string(), ReplValue::String(name.to_string())),
            ("score".to_string(), ReplValue::Int(score)),
            ("active".to_string(), ReplValue::Bool(active)),
            ("role".to_string(), ReplValue::Atom(role.to_string())),
        ],
    }
}

fn model_change(
    sequence: u64,
    model: &str,
    id: &str,
    kind: VmModelSyncChangeKind,
    value: Option<ReplValue>,
) -> VmModelSyncChange {
    VmModelSyncChange {
        sequence,
        key: key(model, id),
        version: version(sequence, "node-a"),
        kind,
        value,
    }
}

fn model_subscription(
    model: &str,
    subscriber_id: &str,
    template_id: &str,
) -> VmModelSyncTemplateSubscription {
    VmModelSyncTemplateSubscription::new(model, subscriber_id, template_id)
        .expect("subscription should be valid")
}

fn invalidation(
    sequence: u64,
    model: &str,
    model_id: &str,
    subscriber_id: &str,
    template_id: &str,
    change_kind: &'static str,
) -> VmModelSyncTemplateInvalidation {
    VmModelSyncTemplateInvalidation {
        sequence,
        model: model.to_string(),
        model_id: model_id.to_string(),
        subscriber_id: subscriber_id.to_string(),
        template_id: template_id.to_string(),
        change_kind,
    }
}
