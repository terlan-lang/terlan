use super::ReplValue;
#[cfg(test)]
use std::collections::{BTreeMap, BTreeSet};

/// Stable VM-owned identity for one syncable model row.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmModelSyncKey {
    pub(crate) model: String,
    pub(crate) id: String,
}

impl VmModelSyncKey {
    #[cfg(test)]
    pub(crate) fn new(model: impl Into<String>, id: impl Into<String>) -> Result<Self, String> {
        let model = model.into();
        let id = id.into();
        if model.is_empty() {
            return Err("error[vm_model_sync]: model name must be non-empty".to_string());
        }
        if id.is_empty() {
            return Err("error[vm_model_sync]: model id must be non-empty".to_string());
        }
        Ok(Self { model, id })
    }
}

/// Monotonic optimistic-concurrency version for a syncable model row.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmModelSyncVersion {
    pub(crate) sequence: u64,
    pub(crate) writer_id: String,
}

impl VmModelSyncVersion {
    #[cfg(test)]
    pub(crate) fn new(sequence: u64, writer_id: impl Into<String>) -> Result<Self, String> {
        let writer_id = writer_id.into();
        if sequence == 0 {
            return Err(
                "error[vm_model_sync]: model version sequence must be non-zero".to_string(),
            );
        }
        if writer_id.is_empty() {
            return Err(
                "error[vm_model_sync]: model version writer id must be non-empty".to_string(),
            );
        }
        Ok(Self {
            sequence,
            writer_id,
        })
    }
}

/// Source-facing model change kind emitted after committed updates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmModelSyncChangeKind {
    #[cfg(test)]
    Created,
    #[cfg(test)]
    Updated,
    #[cfg(test)]
    Deleted,
}

impl VmModelSyncChangeKind {
    #[cfg(test)]
    pub(crate) fn kind(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Updated => "updated",
            Self::Deleted => "deleted",
        }
    }
}

/// Typed committed model change emitted to subscribers/templates.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmModelSyncChange {
    pub(crate) sequence: u64,
    pub(crate) key: VmModelSyncKey,
    pub(crate) version: VmModelSyncVersion,
    pub(crate) kind: VmModelSyncChangeKind,
    pub(crate) value: Option<ReplValue>,
}

/// Current stored row for a syncable model.
#[derive(Clone, Debug, PartialEq)]
#[cfg(test)]
pub(crate) struct VmModelSyncRow {
    pub(crate) key: VmModelSyncKey,
    pub(crate) version: VmModelSyncVersion,
    pub(crate) value: ReplValue,
}

/// Typed optimistic-concurrency outcome for model writes.
#[derive(Clone, Debug, PartialEq)]
#[cfg(test)]
pub(crate) enum VmModelSyncOutcome {
    Applied(VmModelSyncChange),
    Replayed(VmModelSyncRow),
    Deleted(VmModelSyncChange),
    Missing(VmModelSyncKey),
    Conflict {
        key: VmModelSyncKey,
        current_version: VmModelSyncVersion,
        incoming_version: VmModelSyncVersion,
    },
}

/// VM-owned scalar type expected while projecting database rows to sync rows.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum VmModelSyncProjectedFieldType {
    Int,
    String,
    Bool,
    Atom,
}

#[cfg(test)]
impl VmModelSyncProjectedFieldType {
    fn accepts(self, value: &ReplValue) -> bool {
        match self {
            Self::Int => matches!(value, ReplValue::Int(_)),
            Self::String => matches!(value, ReplValue::String(_)),
            Self::Bool => matches!(value, ReplValue::Bool(_)),
            Self::Atom => matches!(value, ReplValue::Atom(_)),
        }
    }
}

/// Explicit field mapping from an adapter row into a syncable model record.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmModelSyncRowFieldProjection {
    pub(crate) row_field: String,
    pub(crate) model_field: String,
    pub(crate) field_type: VmModelSyncProjectedFieldType,
}

#[cfg(test)]
impl VmModelSyncRowFieldProjection {
    pub(crate) fn new(
        row_field: impl Into<String>,
        model_field: impl Into<String>,
        field_type: VmModelSyncProjectedFieldType,
    ) -> Result<Self, String> {
        let row_field = row_field.into();
        let model_field = model_field.into();
        if row_field.is_empty() {
            return Err("error[vm_model_sync]: row projection field must be non-empty".to_string());
        }
        if model_field.is_empty() {
            return Err(
                "error[vm_model_sync]: model projection field must be non-empty".to_string(),
            );
        }
        Ok(Self {
            row_field,
            model_field,
            field_type,
        })
    }
}

/// Deterministic adapter-row projection into a VM-owned syncable model row.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmModelSyncRowProjection {
    pub(crate) model: String,
    pub(crate) id_field: String,
    pub(crate) version_sequence_field: String,
    pub(crate) version_writer_field: String,
    pub(crate) fields: Vec<VmModelSyncRowFieldProjection>,
}

#[cfg(test)]
impl VmModelSyncRowProjection {
    pub(crate) fn new(
        model: impl Into<String>,
        id_field: impl Into<String>,
        version_sequence_field: impl Into<String>,
        version_writer_field: impl Into<String>,
        fields: Vec<VmModelSyncRowFieldProjection>,
    ) -> Result<Self, String> {
        let model = model.into();
        let id_field = id_field.into();
        let version_sequence_field = version_sequence_field.into();
        let version_writer_field = version_writer_field.into();
        if model.is_empty() {
            return Err("error[vm_model_sync]: row projection model must be non-empty".to_string());
        }
        if id_field.is_empty() {
            return Err(
                "error[vm_model_sync]: row projection id field must be non-empty".to_string(),
            );
        }
        if version_sequence_field.is_empty() {
            return Err(
                "error[vm_model_sync]: row projection version sequence field must be non-empty"
                    .to_string(),
            );
        }
        if version_writer_field.is_empty() {
            return Err(
                "error[vm_model_sync]: row projection version writer field must be non-empty"
                    .to_string(),
            );
        }
        if fields.is_empty() {
            return Err(
                "error[vm_model_sync]: row projection fields must be non-empty".to_string(),
            );
        }
        let mut model_fields = BTreeSet::new();
        for field in &fields {
            if !model_fields.insert(field.model_field.as_str()) {
                return Err(format!(
                    "error[vm_model_sync]: row projection model field `{}` is duplicated",
                    field.model_field
                ));
            }
        }
        Ok(Self {
            model,
            id_field,
            version_sequence_field,
            version_writer_field,
            fields,
        })
    }
}

/// Projects an adapter row into a typed sync row without ORM identity maps.
#[cfg(test)]
pub(crate) fn project_model_sync_row_from_adapter_fields(
    projection: &VmModelSyncRowProjection,
    row: &BTreeMap<String, ReplValue>,
) -> Result<VmModelSyncRow, String> {
    let id = required_string_row_field(row, &projection.id_field)?;
    let sequence = required_positive_sequence_row_field(row, &projection.version_sequence_field)?;
    let writer_id = required_string_row_field(row, &projection.version_writer_field)?;
    let mut fields = Vec::with_capacity(projection.fields.len());
    for field in &projection.fields {
        let value = row.get(&field.row_field).ok_or_else(|| {
            format!(
                "error[vm_model_sync]: row field `{}` is missing",
                field.row_field
            )
        })?;
        if !field.field_type.accepts(value) {
            return Err(format!(
                "error[vm_model_sync]: row field `{}` expected `{:?}`",
                field.row_field, field.field_type
            ));
        }
        fields.push((field.model_field.clone(), value.clone()));
    }
    Ok(VmModelSyncRow {
        key: VmModelSyncKey::new(&projection.model, id)?,
        version: VmModelSyncVersion::new(sequence, writer_id)?,
        value: ReplValue::Record {
            name: projection.model.clone(),
            fields,
        },
    })
}

#[cfg(test)]
fn required_string_row_field(
    row: &BTreeMap<String, ReplValue>,
    field: &str,
) -> Result<String, String> {
    match row.get(field) {
        Some(ReplValue::String(value)) if !value.is_empty() => Ok(value.clone()),
        Some(_) => Err(format!(
            "error[vm_model_sync]: row field `{field}` expected non-empty `String`"
        )),
        None => Err(format!(
            "error[vm_model_sync]: row field `{field}` is missing"
        )),
    }
}

#[cfg(test)]
fn required_positive_sequence_row_field(
    row: &BTreeMap<String, ReplValue>,
    field: &str,
) -> Result<u64, String> {
    match row.get(field) {
        Some(ReplValue::Int(value)) if *value > 0 => Ok(*value as u64),
        Some(_) => Err(format!(
            "error[vm_model_sync]: row field `{field}` expected positive `Int`"
        )),
        None => Err(format!(
            "error[vm_model_sync]: row field `{field}` is missing"
        )),
    }
}

/// Permission operation checked against model-sync changes before publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum VmModelSyncPermissionOperation {
    Read,
    Write,
    Delete,
    Subscribe,
}

/// Field-level permission grant for a syncable model.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmModelSyncFieldPermission {
    pub(crate) field: String,
    pub(crate) operations: Vec<VmModelSyncPermissionOperation>,
}

#[cfg(test)]
impl VmModelSyncFieldPermission {
    pub(crate) fn new(
        field: impl Into<String>,
        operations: Vec<VmModelSyncPermissionOperation>,
    ) -> Result<Self, String> {
        let field = field.into();
        if field.is_empty() {
            return Err(
                "error[vm_model_sync]: field permission name must be non-empty".to_string(),
            );
        }
        if operations.is_empty() {
            return Err(
                "error[vm_model_sync]: field permission operations must be non-empty".to_string(),
            );
        }
        Ok(Self { field, operations })
    }

    fn allows(&self, operation: VmModelSyncPermissionOperation) -> bool {
        self.operations.contains(&operation)
    }
}

/// Model-level permission policy used to detect permission drift in changes.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmModelSyncPermissionPolicy {
    pub(crate) model: String,
    pub(crate) operations: Vec<VmModelSyncPermissionOperation>,
    pub(crate) fields: Vec<VmModelSyncFieldPermission>,
}

#[cfg(test)]
impl VmModelSyncPermissionPolicy {
    pub(crate) fn new(
        model: impl Into<String>,
        operations: Vec<VmModelSyncPermissionOperation>,
        fields: Vec<VmModelSyncFieldPermission>,
    ) -> Result<Self, String> {
        let model = model.into();
        if model.is_empty() {
            return Err("error[vm_model_sync]: permission model must be non-empty".to_string());
        }
        if operations.is_empty() {
            return Err(
                "error[vm_model_sync]: permission operations must be non-empty".to_string(),
            );
        }
        Ok(Self {
            model,
            operations,
            fields,
        })
    }

    fn allows_model_operation(&self, operation: VmModelSyncPermissionOperation) -> bool {
        self.operations.contains(&operation)
    }

    fn allows_field_operation(
        &self,
        field: &str,
        operation: VmModelSyncPermissionOperation,
    ) -> bool {
        self.fields
            .iter()
            .find(|permission| permission.field == field)
            .is_some_and(|permission| permission.allows(operation))
    }
}

/// Validates that committed model changes remain inside the model and field
/// permission surface declared for the affected syncable models.
#[cfg(test)]
pub(crate) fn validate_model_sync_permission_drift(
    changes: &[VmModelSyncChange],
    policies: &[VmModelSyncPermissionPolicy],
) -> Result<(), String> {
    for change in changes {
        let operation = permission_operation_for_change(change.kind);
        let policy = policies
            .iter()
            .find(|policy| policy.model == change.key.model)
            .ok_or_else(|| {
                format!(
                    "error[vm_model_sync]: model `{}` has no permission policy",
                    change.key.model
                )
            })?;
        if !policy.allows_model_operation(operation) {
            return Err(format!(
                "error[vm_model_sync]: model `{}` denies `{:?}`",
                change.key.model, operation
            ));
        }
        if operation == VmModelSyncPermissionOperation::Write {
            let Some(ReplValue::Record { fields, .. }) = change.value.as_ref() else {
                continue;
            };
            for (field, _) in fields {
                if !policy.allows_field_operation(field, operation) {
                    return Err(format!(
                        "error[vm_model_sync]: model `{}` field `{}` denies `{:?}`",
                        change.key.model, field, operation
                    ));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
fn permission_operation_for_change(kind: VmModelSyncChangeKind) -> VmModelSyncPermissionOperation {
    match kind {
        VmModelSyncChangeKind::Created | VmModelSyncChangeKind::Updated => {
            VmModelSyncPermissionOperation::Write
        }
        VmModelSyncChangeKind::Deleted => VmModelSyncPermissionOperation::Delete,
    }
}

/// VM-owned template binding for replaying model changes into DOM patches.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmDomPatchTemplateBinding {
    pub(crate) model: String,
    pub(crate) field: String,
    pub(crate) selector_template: String,
}

#[cfg(test)]
impl VmDomPatchTemplateBinding {
    pub(crate) fn new(
        model: impl Into<String>,
        field: impl Into<String>,
        selector_template: impl Into<String>,
    ) -> Result<Self, String> {
        let model = model.into();
        let field = field.into();
        let selector_template = selector_template.into();
        if model.is_empty() {
            return Err("error[vm_dom_patch]: binding model must be non-empty".to_string());
        }
        if field.is_empty() {
            return Err("error[vm_dom_patch]: binding field must be non-empty".to_string());
        }
        if selector_template.is_empty() {
            return Err("error[vm_dom_patch]: binding selector must be non-empty".to_string());
        }
        Ok(Self {
            model,
            field,
            selector_template,
        })
    }

    fn selector_for_key(&self, key: &VmModelSyncKey) -> String {
        self.selector_template.replace("{id}", &key.id)
    }
}

/// DOM patch operation kind emitted by typed template binding replay.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum VmDomPatchOperationKind {
    ReplaceText,
    RemoveBinding,
}

/// Deterministic DOM patch operation generated from a typed model change.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmDomPatchOperation {
    pub(crate) sequence: u64,
    pub(crate) selector: String,
    pub(crate) kind: VmDomPatchOperationKind,
    pub(crate) value: Option<String>,
}

/// Typed live-template subscription that depends on committed model changes.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmModelSyncTemplateSubscription {
    pub(crate) model: String,
    pub(crate) subscriber_id: String,
    pub(crate) template_id: String,
}

#[cfg(test)]
impl VmModelSyncTemplateSubscription {
    pub(crate) fn new(
        model: impl Into<String>,
        subscriber_id: impl Into<String>,
        template_id: impl Into<String>,
    ) -> Result<Self, String> {
        let model = model.into();
        let subscriber_id = subscriber_id.into();
        let template_id = template_id.into();
        if model.is_empty() {
            return Err(
                "error[vm_model_sync]: template subscription model must be non-empty".to_string(),
            );
        }
        if subscriber_id.is_empty() {
            return Err(
                "error[vm_model_sync]: template subscription subscriber id must be non-empty"
                    .to_string(),
            );
        }
        if template_id.is_empty() {
            return Err(
                "error[vm_model_sync]: template subscription template id must be non-empty"
                    .to_string(),
            );
        }
        Ok(Self {
            model,
            subscriber_id,
            template_id,
        })
    }
}

/// Deterministic invalidation emitted after a committed model event.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmModelSyncTemplateInvalidation {
    pub(crate) sequence: u64,
    pub(crate) model: String,
    pub(crate) model_id: String,
    pub(crate) subscriber_id: String,
    pub(crate) template_id: String,
    pub(crate) change_kind: &'static str,
}

/// Converts committed model changes into live-template invalidations.
#[cfg(test)]
pub(crate) fn invalidate_live_template_subscribers_from_model_events(
    changes: &[VmModelSyncChange],
    subscriptions: &[VmModelSyncTemplateSubscription],
) -> Vec<VmModelSyncTemplateInvalidation> {
    let mut invalidations = Vec::new();
    for change in changes {
        for subscription in subscriptions
            .iter()
            .filter(|subscription| subscription.model == change.key.model)
        {
            invalidations.push(VmModelSyncTemplateInvalidation {
                sequence: change.sequence,
                model: change.key.model.clone(),
                model_id: change.key.id.clone(),
                subscriber_id: subscription.subscriber_id.clone(),
                template_id: subscription.template_id.clone(),
                change_kind: change.kind.kind(),
            });
        }
    }
    invalidations
}

/// Replays one typed model change against template bindings.
#[cfg(test)]
pub(crate) fn replay_dom_patches_for_template_bindings(
    change: &VmModelSyncChange,
    bindings: &[VmDomPatchTemplateBinding],
) -> Result<Vec<VmDomPatchOperation>, String> {
    let mut operations = Vec::new();
    for binding in bindings
        .iter()
        .filter(|binding| binding.model == change.key.model)
    {
        let selector = binding.selector_for_key(&change.key);
        match change.kind {
            VmModelSyncChangeKind::Created | VmModelSyncChangeKind::Updated => {
                let value = change.value.as_ref().ok_or_else(|| {
                    "error[vm_dom_patch]: non-delete change is missing a value".to_string()
                })?;
                let field_value =
                    dom_patch_record_field(value, &binding.field).ok_or_else(|| {
                        format!(
                            "error[vm_dom_patch]: model `{}` field `{}` is missing",
                            change.key.model, binding.field
                        )
                    })?;
                operations.push(VmDomPatchOperation {
                    sequence: change.sequence,
                    selector,
                    kind: VmDomPatchOperationKind::ReplaceText,
                    value: Some(dom_patch_text_value(field_value)?),
                });
            }
            VmModelSyncChangeKind::Deleted => operations.push(VmDomPatchOperation {
                sequence: change.sequence,
                selector,
                kind: VmDomPatchOperationKind::RemoveBinding,
                value: None,
            }),
        }
    }
    Ok(operations)
}

#[cfg(test)]
fn dom_patch_record_field<'a>(value: &'a ReplValue, field: &str) -> Option<&'a ReplValue> {
    match value {
        ReplValue::Record { fields, .. } => fields
            .iter()
            .find_map(|(name, value)| (name == field).then_some(value)),
        _ => None,
    }
}

#[cfg(test)]
fn dom_patch_text_value(value: &ReplValue) -> Result<String, String> {
    match value {
        ReplValue::Unit => Ok("".to_string()),
        ReplValue::Int(value) => Ok(value.to_string()),
        ReplValue::Float(value) | ReplValue::String(value) | ReplValue::Atom(value) => {
            Ok(value.clone())
        }
        ReplValue::Bool(value) => Ok(value.to_string()),
        _ => Err("error[vm_dom_patch]: bound template value must be scalar".to_string()),
    }
}

/// Adapter contract for VM-owned syncable model stores.
#[cfg(test)]
pub(crate) trait VmModelSyncStoreAdapter {
    fn get(&self, key: &VmModelSyncKey) -> Option<&VmModelSyncRow>;
    fn put(
        &mut self,
        key: VmModelSyncKey,
        expected_version: Option<VmModelSyncVersion>,
        value: ReplValue,
        next_version: VmModelSyncVersion,
    ) -> VmModelSyncOutcome;
    fn delete(
        &mut self,
        key: VmModelSyncKey,
        expected_version: VmModelSyncVersion,
        tombstone_version: VmModelSyncVersion,
    ) -> VmModelSyncOutcome;
    fn export_snapshot(&self) -> Vec<VmModelSyncRow>;
    fn changes_since(&self, sequence: u64) -> Vec<VmModelSyncChange>;
}

/// Portable capability required from a model-sync adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum VmModelSyncAdapterCapability {
    TypedKey,
    OptimisticVersion,
    Put,
    Delete,
    Snapshot,
    ChangeStream,
    TypedRowDecode,
    TransactionRollback,
}

/// Explicit model-sync adapter contract used to keep the public abstraction
/// portable across VM-owned stores and database-backed stores.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmModelSyncAdapterContract {
    pub(crate) name: &'static str,
    pub(crate) storage_family: &'static str,
    pub(crate) capabilities: Vec<VmModelSyncAdapterCapability>,
}

#[cfg(test)]
impl VmModelSyncAdapterContract {
    pub(crate) fn new(
        name: &'static str,
        storage_family: &'static str,
        capabilities: Vec<VmModelSyncAdapterCapability>,
    ) -> Result<Self, String> {
        if name.is_empty() {
            return Err(
                "error[vm_model_sync]: adapter contract name must be non-empty".to_string(),
            );
        }
        if storage_family.is_empty() {
            return Err(
                "error[vm_model_sync]: adapter contract storage family must be non-empty"
                    .to_string(),
            );
        }
        Ok(Self {
            name,
            storage_family,
            capabilities,
        })
    }

    fn has_capability(&self, capability: VmModelSyncAdapterCapability) -> bool {
        self.capabilities.contains(&capability)
    }

    fn is_postgres(&self) -> bool {
        self.storage_family == "postgres"
    }
}

/// Source-visible declaration for one syncable model family.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmSyncableModelDeclaration {
    pub(crate) name: String,
    pub(crate) adapter_contract: VmModelSyncAdapterContract,
    pub(crate) key_model: String,
}

#[cfg(test)]
impl VmSyncableModelDeclaration {
    pub(crate) fn new(
        name: impl Into<String>,
        adapter_contract: VmModelSyncAdapterContract,
        key_model: impl Into<String>,
    ) -> Result<Self, String> {
        let name = name.into();
        if name.is_empty() {
            return Err("error[vm_model_sync]: syncable model name must be non-empty".to_string());
        }
        let key_model = key_model.into();
        if key_model.is_empty() {
            return Err(
                "error[vm_model_sync]: syncable model key model must be non-empty".to_string(),
            );
        }
        if key_model != name {
            return Err(format!(
                "error[vm_model_sync]: syncable model `{name}` cannot use key model `{key_model}`"
            ));
        }
        Ok(Self {
            name,
            adapter_contract,
            key_model,
        })
    }
}

#[cfg(test)]
const PORTABLE_MODEL_SYNC_CAPABILITIES: &[VmModelSyncAdapterCapability] = &[
    VmModelSyncAdapterCapability::TypedKey,
    VmModelSyncAdapterCapability::OptimisticVersion,
    VmModelSyncAdapterCapability::Put,
    VmModelSyncAdapterCapability::Delete,
    VmModelSyncAdapterCapability::Snapshot,
    VmModelSyncAdapterCapability::ChangeStream,
];

#[cfg(test)]
const POSTGRES_ONLY_MODEL_SYNC_CAPABILITIES: &[VmModelSyncAdapterCapability] = &[
    VmModelSyncAdapterCapability::TypedRowDecode,
    VmModelSyncAdapterCapability::TransactionRollback,
];

/// Checks that non-Postgres model-sync adapters keep the same portable core
/// capability surface and do not leak Postgres-only behavior.
#[cfg(test)]
pub(crate) fn validate_non_postgres_model_sync_adapter_contracts(
    contracts: &[VmModelSyncAdapterContract],
) -> Result<(), String> {
    let mut saw_non_postgres_adapter = false;
    for contract in contracts {
        if contract.is_postgres() {
            continue;
        }
        saw_non_postgres_adapter = true;
        for capability in PORTABLE_MODEL_SYNC_CAPABILITIES {
            if !contract.has_capability(*capability) {
                return Err(format!(
                    "error[vm_model_sync]: adapter `{}` is missing portable capability `{:?}`",
                    contract.name, capability
                ));
            }
        }
        for capability in POSTGRES_ONLY_MODEL_SYNC_CAPABILITIES {
            if contract.has_capability(*capability) {
                return Err(format!(
                    "error[vm_model_sync]: adapter `{}` leaks Postgres-only capability `{:?}`",
                    contract.name, capability
                ));
            }
        }
    }
    if !saw_non_postgres_adapter {
        return Err(
            "error[vm_model_sync]: at least one non-Postgres adapter contract is required"
                .to_string(),
        );
    }
    Ok(())
}

/// Deterministic in-memory adapter used by the VM and adversarial tests.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg(test)]
pub(crate) struct VmInMemoryModelSyncStore {
    rows: BTreeMap<VmModelSyncKey, VmModelSyncRow>,
    changes: Vec<VmModelSyncChange>,
    next_change_sequence: u64,
}

#[cfg(test)]
impl VmInMemoryModelSyncStore {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self {
            rows: BTreeMap::new(),
            changes: Vec::new(),
            next_change_sequence: 1,
        }
    }

    #[cfg(test)]
    fn next_sequence(&mut self) -> u64 {
        let sequence = self.next_change_sequence;
        self.next_change_sequence += 1;
        sequence
    }

    #[cfg(test)]
    fn conflict(
        key: VmModelSyncKey,
        current_version: VmModelSyncVersion,
        incoming_version: VmModelSyncVersion,
    ) -> VmModelSyncOutcome {
        VmModelSyncOutcome::Conflict {
            key,
            current_version,
            incoming_version,
        }
    }
}

#[cfg(test)]
impl VmModelSyncStoreAdapter for VmInMemoryModelSyncStore {
    fn get(&self, key: &VmModelSyncKey) -> Option<&VmModelSyncRow> {
        self.rows.get(key)
    }

    fn put(
        &mut self,
        key: VmModelSyncKey,
        expected_version: Option<VmModelSyncVersion>,
        value: ReplValue,
        next_version: VmModelSyncVersion,
    ) -> VmModelSyncOutcome {
        match (self.rows.get(&key), expected_version) {
            (None, Some(expected)) => {
                return Self::conflict(key, expected, next_version);
            }
            (Some(current), Some(expected)) if current.version != expected => {
                return Self::conflict(key, current.version.clone(), next_version);
            }
            (Some(current), None) if current.version == next_version && current.value == value => {
                return VmModelSyncOutcome::Replayed(current.clone());
            }
            (Some(current), None) => {
                return Self::conflict(key, current.version.clone(), next_version);
            }
            _ => {}
        }

        let kind = if self.rows.contains_key(&key) {
            VmModelSyncChangeKind::Updated
        } else {
            VmModelSyncChangeKind::Created
        };
        let row = VmModelSyncRow {
            key: key.clone(),
            version: next_version.clone(),
            value: value.clone(),
        };
        self.rows.insert(key.clone(), row);
        let change = VmModelSyncChange {
            sequence: self.next_sequence(),
            key,
            version: next_version,
            kind,
            value: Some(value),
        };
        self.changes.push(change.clone());
        VmModelSyncOutcome::Applied(change)
    }

    fn delete(
        &mut self,
        key: VmModelSyncKey,
        expected_version: VmModelSyncVersion,
        tombstone_version: VmModelSyncVersion,
    ) -> VmModelSyncOutcome {
        let Some(current) = self.rows.get(&key) else {
            return VmModelSyncOutcome::Missing(key);
        };
        if current.version != expected_version {
            return Self::conflict(key, current.version.clone(), tombstone_version);
        }

        self.rows.remove(&key);
        let change = VmModelSyncChange {
            sequence: self.next_sequence(),
            key,
            version: tombstone_version,
            kind: VmModelSyncChangeKind::Deleted,
            value: None,
        };
        self.changes.push(change.clone());
        VmModelSyncOutcome::Deleted(change)
    }

    fn export_snapshot(&self) -> Vec<VmModelSyncRow> {
        self.rows.values().cloned().collect()
    }

    fn changes_since(&self, sequence: u64) -> Vec<VmModelSyncChange> {
        self.changes
            .iter()
            .filter(|change| change.sequence > sequence)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
#[path = "model_sync_test.rs"]
#[cfg(test)]
mod model_sync_test;
