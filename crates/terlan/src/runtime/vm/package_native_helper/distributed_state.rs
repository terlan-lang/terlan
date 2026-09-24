//! Actor-owned state operations using the VM's shared conflict/checkpoint logic.

use super::{PureNativeCapabilityRequest, ReplValue, VmRuntimeError, VmRuntimeResult};
use crate::runtime::vm::distributed_state::{
    VmDistributedStateConflict as Conflict, VmDistributedStateEntry as Entry,
    VmDistributedStatePolicy as Policy, VmDistributedStateScope as Scope,
    VmDistributedStateStore as Store, VmDistributedStateVersion as Version,
    VmDistributedStateWriteOutcome as Outcome,
};
use crate::terlan_native_boundary::handle::NativeBoundaryHandle;
use crate::terlan_native_boundary::resource::{ResourceError, ResourceRegistry};

#[cfg(test)]
#[path = "distributed_state_test.rs"]
mod tests;

const PREFIX: &str = "std.vm.DistributedState.";

/// A single handle namespace prevents one descriptor kind aliasing another.
enum Resource {
    Scope(Scope),
    Version(Version),
    Policy(Policy),
    Store(Store),
    Entry(Entry),
    Outcome(Outcome),
    Conflict(Conflict),
    Snapshot(Vec<Entry>),
}

impl Resource {
    fn kind(&self) -> &'static str {
        match self {
            Self::Scope(_) => "Scope",
            Self::Version(_) => "Version",
            Self::Policy(_) => "Policy",
            Self::Store(_) => "Store",
            Self::Entry(_) => "Entry",
            Self::Outcome(_) => "Outcome",
            Self::Conflict(_) => "Conflict",
            Self::Snapshot(_) => "Snapshot",
        }
    }
}

/// State resources remain on the execution owner and never invoke worker RPC.
#[derive(Default)]
pub(super) struct VmDistributedStateRuntime {
    resources: ResourceRegistry<Resource>,
}

impl VmDistributedStateRuntime {
    /// Executes a checked native operation with the calling actor's authority.
    pub(super) fn call(
        &mut self,
        owner: u64,
        request: &PureNativeCapabilityRequest,
    ) -> VmRuntimeResult<ReplValue> {
        let args = request.package_arguments.as_deref().ok_or(
            "error[vm.distributed_state.arguments]: state operations require typed arguments",
        )?;
        let resource = match (request.operation.as_str(), args) {
            ("std.vm.distributed_state.scope", [namespace, key]) => {
                Resource::Scope(Scope::new(text(namespace)?, text(key)?)?)
            }
            ("std.vm.distributed_state.version", [sequence, node]) => {
                let ReplValue::Int(sequence) = sequence else {
                    return Err("error[vm.distributed_state.version]: expected Int".into());
                };
                let sequence = u64::try_from(*sequence)
                    .map_err(|_| "error[vm.distributed_state.version]: negative sequence")?;
                Resource::Version(Version::new(sequence, text(node)?)?)
            }
            ("std.vm.distributed_state.policy", [name]) => Resource::Policy(match text(name)? {
                "winner_takes_all" => Policy::WinnerTakesAll,
                "last_writer_wins" => Policy::LastWriterWins,
                "merge" => Policy::Merge,
                "explicit_user_resolution" => Policy::ExplicitUserResolution,
                _ => return Err("error[vm.distributed_state.policy]: unknown policy".into()),
            }),
            ("std.vm.distributed_state.store", []) => Resource::Store(Store::new()),
            ("std.vm.distributed_state.write", [store, scope, node, value, version, policy]) => {
                let Resource::Scope(scope) = self.resource(owner, scope, "Scope")? else {
                    unreachable!("checked resource kind")
                };
                let scope = scope.clone();
                let Resource::Version(version) = self.resource(owner, version, "Version")? else {
                    unreachable!("checked resource kind")
                };
                let version = version.clone();
                let Resource::Policy(policy) = self.resource(owner, policy, "Policy")? else {
                    unreachable!("checked resource kind")
                };
                let policy = *policy;
                let handle = self.handle(owner, store, "Store")?;
                let Resource::Store(store) = self
                    .resources
                    .get_mut_for_owner(handle, owner)
                    .map_err(resource_error)?
                else {
                    unreachable!("checked resource kind")
                };
                Resource::Outcome(store.write(
                    scope,
                    text(node)?,
                    value.clone(),
                    version,
                    policy,
                )?)
            }
            ("std.vm.distributed_state.get", [store, scope]) => {
                let Resource::Store(store) = self.resource(owner, store, "Store")? else {
                    unreachable!("checked resource kind")
                };
                let Resource::Scope(scope) = self.resource(owner, scope, "Scope")? else {
                    unreachable!("checked resource kind")
                };
                Resource::Entry(
                    store.get(scope).cloned().ok_or(
                        "error[vm.distributed_state.missing]: no entry exists for the scope",
                    )?,
                )
            }
            ("std.vm.distributed_state.export_snapshot", [store]) => {
                let Resource::Store(store) = self.resource(owner, store, "Store")? else {
                    unreachable!("checked resource kind")
                };
                Resource::Snapshot(store.export_snapshot())
            }
            ("std.vm.distributed_state.restore", [snapshot]) => {
                let Resource::Snapshot(entries) = self.resource(owner, snapshot, "Snapshot")?
                else {
                    unreachable!("checked resource kind")
                };
                Resource::Store(Store::import_snapshot(entries.clone())?)
            }
            ("std.vm.distributed_state.conflict", [outcome]) => {
                let Resource::Outcome(Outcome::Conflict(conflict)) =
                    self.resource(owner, outcome, "Outcome")?
                else {
                    return Err(
                        "error[vm.distributed_state.conflict]: outcome is not a conflict".into(),
                    );
                };
                Resource::Conflict(conflict.clone())
            }
            ("std.vm.distributed_state.kind", [outcome]) => {
                let Resource::Outcome(outcome) = self.resource(owner, outcome, "Outcome")? else {
                    unreachable!("checked resource kind")
                };
                return Ok(ReplValue::String(
                    match outcome {
                        Outcome::Applied(_) => "applied",
                        Outcome::Replayed(_) => "replayed",
                        Outcome::Conflict(_) => "conflict",
                        Outcome::PolicyMismatch { .. } => "policy_mismatch",
                    }
                    .into(),
                ));
            }
            ("std.vm.distributed_state.entry_owner", [entry]) => {
                let Resource::Entry(entry) = self.resource(owner, entry, "Entry")? else {
                    unreachable!("checked resource kind")
                };
                return Ok(ReplValue::String(entry.owner_node_id.clone()));
            }
            ("std.vm.distributed_state.entry_sequence", [entry]) => {
                let Resource::Entry(entry) = self.resource(owner, entry, "Entry")? else {
                    unreachable!("checked resource kind")
                };
                return sequence_value(entry.version.sequence);
            }
            (
                operation @ ("std.vm.distributed_state.conflict_local_sequence"
                | "std.vm.distributed_state.conflict_incoming_sequence"),
                [conflict],
            ) => {
                let Resource::Conflict(conflict) = self.resource(owner, conflict, "Conflict")?
                else {
                    unreachable!("checked resource kind")
                };
                return sequence_value(if operation.ends_with(".conflict_local_sequence") {
                    conflict.local_version.sequence
                } else {
                    conflict.incoming_version.sequence
                });
            }
            _ => {
                return Err(format!(
                    "error[vm.distributed_state.operation]: unsupported operation or arity `{}`/{}",
                    request.operation,
                    args.len()
                )
                .into())
            }
        };
        self.insert(owner, resource)
    }

    /// Borrows an actor-authorized logical snapshot for durable encoding.
    pub(super) fn snapshot_entries(
        &self,
        owner: u64,
        value: &ReplValue,
    ) -> VmRuntimeResult<&[Entry]> {
        match self.resource(owner, value, "Snapshot")? {
            Resource::Snapshot(entries) => Ok(entries),
            _ => Err("error[vm.distributed_state.kind]: expected Snapshot".into()),
        }
    }

    /// Creates a new mutable store only after validating every restored entry.
    pub(super) fn restore_entries(
        &mut self,
        owner: u64,
        entries: Vec<Entry>,
    ) -> VmRuntimeResult<ReplValue> {
        self.insert(owner, Resource::Store(Store::import_snapshot(entries)?))
    }

    fn insert(&mut self, owner: u64, resource: Resource) -> VmRuntimeResult<ReplValue> {
        let kind = format!("{PREFIX}{}", resource.kind());
        let handle = self
            .resources
            .insert_for_owner(owner, resource)
            .map_err(resource_error)?;
        super::direct_std::native_handle_value(owner, handle, &kind)
    }

    fn handle(
        &self,
        owner: u64,
        value: &ReplValue,
        expected: &str,
    ) -> VmRuntimeResult<NativeBoundaryHandle> {
        let ReplValue::Record { name, fields } = value else {
            return Err(
                "error[vm.distributed_state.handle]: expected an opaque state resource".into(),
            );
        };
        let (handle, kind, claimed_owner) = super::direct_std::native_handle(fields)
            .ok_or("error[vm.distributed_state.handle]: incomplete state handle")??;
        if name != expected
            || kind.strip_prefix(PREFIX) != Some(expected)
            || claimed_owner != owner.to_string()
        {
            return Err(
                "error[vm.distributed_state.handle]: foreign owner or invalid identity".into(),
            );
        }
        if self
            .resources
            .get_for_owner(handle, owner)
            .map_err(resource_error)?
            .kind()
            != expected
        {
            return Err(
                "error[vm.distributed_state.kind]: stored resource has a different kind".into(),
            );
        }
        Ok(handle)
    }

    fn resource(
        &self,
        owner: u64,
        value: &ReplValue,
        expected: &str,
    ) -> VmRuntimeResult<&Resource> {
        self.resources
            .get_for_owner(self.handle(owner, value, expected)?, owner)
            .map_err(resource_error)
    }

    /// Revokes all descriptors belonging to a completed or failed actor.
    pub(super) fn close_owner(&mut self, owner: u64) {
        self.resources.dispose_owner(owner);
    }
}

fn resource_error(error: ResourceError) -> VmRuntimeError {
    format!("error[{}]: {}", error.code(), error.message()).into()
}

fn sequence_value(sequence: u64) -> VmRuntimeResult<ReplValue> {
    Ok(ReplValue::Int(i64::try_from(sequence).map_err(|_| {
        "error[vm.distributed_state.sequence]: sequence exceeds Terlan Int"
    })?))
}

fn text(value: &ReplValue) -> VmRuntimeResult<&str> {
    match value {
        ReplValue::String(value) => Ok(value),
        ReplValue::StringBytes(value) => std::str::from_utf8(value)
            .map_err(|_| "error[vm.distributed_state.text]: expected valid UTF-8".into()),
        _ => Err("error[vm.distributed_state.text]: expected String".into()),
    }
}
