//! Actor-owned logical checkpoints; blocking database work belongs to workers.

use super::distributed_state::VmDistributedStateRuntime;
use super::{PureNativeCapabilityRequest, ReplValue, VmRuntimeError, VmRuntimeResult};
use crate::runtime::vm::term_format::{decode_tetf_checkpoint, encode_tetf_checkpoint};
use crate::terlan_native_boundary::resource::{ResourceError, ResourceRegistry};
use terlan_storage::{Checkpoint, MAX_CHECKPOINT_BYTES};

mod budget;
mod lifecycle;
use lifecycle::Resource;
pub(super) use lifecycle::{Pending, Request, Step};

/// Checkpoints are immutable, bounded bytes in a distinct owner-checked namespace.
#[derive(Default)]
pub(super) struct VmDistributedStorageRuntime {
    resources: ResourceRegistry<Resource>,
    bindings: std::collections::BTreeMap<String, Option<[u8; 32]>>,
    budget: budget::Budget,
}

impl VmDistributedStorageRuntime {
    /// Executes portable checkpoint operations without claiming backend persistence.
    pub(super) fn call(
        &mut self,
        owner: u64,
        request: &PureNativeCapabilityRequest,
        declared_atoms: &[String],
        state: &mut VmDistributedStateRuntime,
    ) -> VmRuntimeResult<ReplValue> {
        let args = request.package_arguments.as_deref().ok_or(
            "error[vm.distributed_storage.arguments]: storage operations require typed arguments",
        )?;
        match (request.operation.as_str(), args) {
            ("std.vm.distributed_storage.checkpoint", [id, ReplValue::Int(sequence), snapshot]) => {
                self.create_checkpoint(owner, (id, *sequence, 1, snapshot), declared_atoms, state)
            }
            (
                "std.vm.distributed_storage.checkpoint_with_schema",
                [id, ReplValue::Int(sequence), ReplValue::Int(schema), snapshot],
            ) => {
                let schema = u32::try_from(*schema).map_err(|_| {
                    "error[vm.distributed_storage.schema]: schema outside supported range"
                })?;
                self.create_checkpoint(
                    owner,
                    (id, *sequence, schema, snapshot),
                    declared_atoms,
                    state,
                )
            }
            ("std.vm.distributed_storage.checkpoint_schema", [snapshot]) => Ok(ReplValue::Int(
                i64::from(self.snapshot(owner, snapshot)?.schema),
            )),
            ("std.vm.distributed_storage.restore", [snapshot]) => {
                let checkpoint = self.snapshot(owner, snapshot)?;
                checkpoint.validate().map_err(storage_error)?;
                let entries = decode_tetf_checkpoint(&checkpoint.payload, declared_atoms)?;
                state.restore_entries(owner, entries)
            }
            _ => self.local_call(owner, &request.operation, args),
        }
    }

    fn create_checkpoint(
        &mut self,
        owner: u64,
        (id, sequence, schema, snapshot): (&ReplValue, i64, u32, &ReplValue),
        declared_atoms: &[String],
        state: &VmDistributedStateRuntime,
    ) -> VmRuntimeResult<ReplValue> {
        let mut checkpoint = Checkpoint {
            id: text(id)?.into(),
            sequence: u64::try_from(sequence)
                .map_err(|_| "error[vm.distributed_storage.sequence]: negative sequence")?,
            schema,
            payload: Vec::new(),
        };
        checkpoint.validate().map_err(storage_error)?;
        checkpoint.payload = encode_tetf_checkpoint(
            state.snapshot_entries(owner, snapshot)?,
            declared_atoms,
            MAX_CHECKPOINT_BYTES,
        )?;
        self.insert(owner, Resource::Snapshot(checkpoint))
    }

    fn snapshot(&self, owner: u64, value: &ReplValue) -> VmRuntimeResult<&Checkpoint> {
        match self.resource(owner, value, "Snapshot")? {
            Resource::Snapshot(checkpoint) => Ok(checkpoint),
            _ => unreachable!("checked resource kind"),
        }
    }

    fn handle(
        &self,
        owner: u64,
        value: &ReplValue,
        expected: &str,
    ) -> VmRuntimeResult<crate::terlan_native_boundary::handle::NativeBoundaryHandle> {
        let ReplValue::Record { name, fields } = value else {
            return Err("error[vm.distributed_storage.handle]: expected storage resource".into());
        };
        let (handle, kind, claimed_owner) = super::direct_std::native_handle(fields)
            .ok_or("error[vm.distributed_storage.handle]: incomplete storage handle")??;
        if name != expected
            || kind != format!("std.vm.DistributedStorage.{expected}")
            || claimed_owner != owner.to_string()
        {
            return Err(
                "error[vm.distributed_storage.handle]: foreign owner or invalid identity".into(),
            );
        }
        let resource = self
            .resources
            .get_for_owner(handle, owner)
            .map_err(resource_error)?;
        if resource.kind() != expected {
            return Err("error[vm.distributed_storage.handle]: invalid resource kind".into());
        }
        Ok(handle)
    }

    fn resource(
        &self,
        owner: u64,
        value: &ReplValue,
        expected: &str,
    ) -> VmRuntimeResult<&Resource> {
        let handle = self.handle(owner, value, expected)?;
        self.resources
            .get_for_owner(handle, owner)
            .map_err(resource_error)
    }

    fn insert(&mut self, owner: u64, resource: Resource) -> VmRuntimeResult<ReplValue> {
        let bytes = resource.budget_bytes();
        self.budget.check(owner, bytes)?;
        let kind = format!("std.vm.DistributedStorage.{}", resource.kind());
        let handle = self
            .resources
            .insert_for_owner(owner, resource)
            .map_err(resource_error)?;
        self.budget.charge(owner, bytes);
        super::direct_std::native_handle_value(owner, handle, &kind)
    }

    /// Revokes immutable checkpoint descriptors when their actor exits.
    pub(super) fn close_owner(&mut self, owner: u64) {
        self.resources.dispose_owner(owner);
        self.budget.release(owner);
    }
}

fn text(value: &ReplValue) -> VmRuntimeResult<&str> {
    match value {
        ReplValue::String(value) => Ok(value),
        ReplValue::StringBytes(value) => std::str::from_utf8(value)
            .map_err(|_| "error[vm.distributed_storage.text]: expected UTF-8".into()),
        _ => Err("error[vm.distributed_storage.text]: expected String".into()),
    }
}

fn resource_error(error: ResourceError) -> VmRuntimeError {
    format!("error[{}]: {}", error.code(), error.message()).into()
}

fn storage_error(error: terlan_storage::StorageError) -> VmRuntimeError {
    format!("error[vm.distributed_storage.checkpoint]: {error}").into()
}

#[cfg(test)]
#[path = "distributed_storage_test.rs"]
mod tests;
