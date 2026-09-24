//! Storage descriptors and reply projection; database execution stays outside the owner.

use super::*;
use crate::terlan_native_boundary::handle::NativeBoundaryHandle;
use crate::terlan_native_boundary::storage_reply::StorageFailure;
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm as Term};

mod completion;
mod operations;
mod outcomes;
mod proofs;
mod resource_validation;
mod volatile;
use outcomes::Outcome;
use proofs::Proof;

/// One namespace prevents forged kind labels from aliasing another live resource.
pub(super) enum Resource {
    Snapshot(Checkpoint),
    Mode(&'static str),
    Policy(Policy),
    Adapter(Adapter),
    Outcome(Outcome),
    Proof(Proof),
}

impl Resource {
    /// Counts payload/view bytes and reserves each local store's full bounded capacity.
    pub(super) fn budget_bytes(&self) -> usize {
        let dynamic = match self {
            Self::Snapshot(checkpoint) => checkpoint.id.len() + checkpoint.payload.len(),
            Self::Policy(policy) => policy.name.len(),
            Self::Adapter(adapter) => {
                adapter.policy.name.len()
                    + resource_validation::RESERVED_BYTES
                    + if adapter.local.is_some() {
                        super::budget::LOCAL_BYTES
                            + super::budget::LOCAL_CHECKPOINTS
                                * (std::mem::size_of::<Checkpoint>()
                                    + std::mem::size_of::<String>()
                                    + 1024
                                    + 64)
                    } else {
                        0
                    }
            }
            Self::Outcome(outcome) => outcome.budget_bytes(),
            Self::Proof(Proof::Isolation { id, .. }) => id.len(),
            _ => 0,
        };
        std::mem::size_of::<Self>() + 64 + dynamic
    }

    pub(super) fn kind(&self) -> &'static str {
        match self {
            Self::Snapshot(_) => "Snapshot",
            Self::Mode(_) => "Mode",
            Self::Policy(_) => "Policy",
            Self::Adapter(_) => "Adapter",
            Self::Outcome(_) => "Outcome",
            Self::Proof(proof) => proof.kind(),
        }
    }
}

#[derive(Clone)]
pub(super) struct Policy {
    name: String,
    mode: &'static str,
    available: bool,
}

pub(super) struct Adapter {
    policy: Policy,
    open: bool,
    sequence: i64,
    flushed: i64,
    batch: (i64, i64, i64),
    local: Option<terlan_storage::LocalCheckpointStore>,
    identity: Option<[u8; 32]>,
    registered_resources: std::collections::BTreeMap<String, [u8; 32]>,
    resource_validation: (i64, i64),
}

/// Closed operations that may be translated into supervisor-authorized storage RPC.
pub(in super::super) enum Operation {
    Resources(resource_validation::Probe),
    Status(StatusProjection),
    Append {
        first: i64,
        last: i64,
        count: i64,
        checkpoint_id: String,
        checksum: u32,
    },
    Migrate {
        next: i64,
    },
    Compact,
    Flush,
    Load {
        id: String,
        proof: bool,
    },
    Close,
}

pub(in super::super) enum StatusProjection {
    Open,
    Atomic,
    Schema,
    Cas,
    Require,
}

/// Owner-retained projection authority; no source handle crosses the worker wire.
pub(in super::super) struct Pending {
    adapter: NativeBoundaryHandle,
    operation: Operation,
    name: &'static str,
}

/// Fully validated request for exactly one configured backend.
pub(in super::super) struct Request {
    pub(in super::super) backend: String,
    pub(in super::super) operation: &'static str,
    pub(in super::super) arguments: Vec<Term>,
    pub(in super::super) pending: Pending,
}

/// Resource checks retain one parked actor while probing authorized workers in turn.
pub(in super::super) enum Step {
    Complete(ReplValue),
    Continue(Request),
}

impl VmDistributedStorageRuntime {
    /// Records bindings only after the supervisor successfully starts every worker.
    pub(in super::super) fn bind(
        &mut self,
        bindings: impl Iterator<Item = (String, Option<[u8; 32]>)>,
    ) {
        self.bindings.extend(bindings);
    }

    pub(super) fn local_call(
        &mut self,
        owner: u64,
        operation: &str,
        args: &[ReplValue],
    ) -> VmRuntimeResult<ReplValue> {
        let operation = operation
            .strip_prefix("std.vm.distributed_storage.")
            .unwrap_or(operation);
        if let Some(value) = self.resource_validation_call(owner, operation, args)? {
            return Ok(value);
        }
        if let Some(value) = self.read_observation(owner, operation, args)? {
            return Ok(value);
        }
        if let Some(value) = self.volatile_call(owner, operation, args)? {
            return Ok(value);
        }
        if let Some(value) = self.local_outcome(owner, operation, args)? {
            return Ok(value);
        }
        let resource = match (operation, args) {
            ("durable", []) => Resource::Mode("durable"),
            ("local_only", []) => Resource::Mode("local_only"),
            ("cluster", []) => Resource::Mode("cluster"),
            ("force_local", []) => Resource::Policy(Policy { name: "force-local".into(), mode: "local_only", available: true }),
            ("policy", [name, mode, ReplValue::Bool(requested)]) => {
                let name = text(name)?.to_string();
                let Resource::Mode(mode) = self.resource(owner, mode, "Mode")? else { unreachable!("checked kind") };
                let available = *requested && (*mode == "local_only" || (*mode == "durable" && self.bindings.contains_key(&name)));
                Resource::Policy(Policy { name, mode, available })
            }
            ("adapter", [policy]) => {
                let Resource::Policy(policy) = self.resource(owner, policy, "Policy")? else { unreachable!("checked kind") };
                let local = if policy.available && policy.mode == "local_only" {
                    Some(terlan_storage::LocalCheckpointStore::new(super::budget::LOCAL_BYTES, super::budget::LOCAL_CHECKPOINTS).map_err(storage_error)?)
                } else { None };
                let identity = if policy.mode == "durable" {
                    self.bindings.get(&policy.name).copied().flatten()
                } else { None };
                Resource::Adapter(Adapter { policy: policy.clone(), open: false, sequence: 0, flushed: 0, batch: (0, 0, 0), local, identity, registered_resources: Default::default(), resource_validation: (0, 0) })
            }
            ("policy_name" | "policy_mode_kind" | "policy_available" | "policy_can_cluster_replicate", [policy]) => {
                let Resource::Policy(policy) = self.resource(owner, policy, "Policy")? else { unreachable!("checked kind") };
                return Ok(policy_field(policy, operation));
            }
            ("adapter_policy_name" | "adapter_policy_mode_kind" | "adapter_policy_available" | "can_cluster_replicate", [adapter]) => {
                let Resource::Adapter(adapter) = self.resource(owner, adapter, "Adapter")? else { unreachable!("checked kind") };
                return Ok(policy_field(&adapter.policy, operation.strip_prefix("adapter_").unwrap_or(operation)));
            }
            ("require_cluster_replication", [adapter]) | ("replicate_snapshot", [adapter, _]) => {
                if operation == "replicate_snapshot" {
                    self.snapshot(owner, &args[1])?;
                }
                let Resource::Adapter(adapter) = self.resource(owner, adapter, "Adapter")? else { unreachable!("checked kind") };
                // A rejection is not a local append or a replication receipt.
                Resource::Outcome(Outcome::new("unsupported", "cluster_replicate", adapter.policy.mode, adapter.sequence))
            }
            _ => return Err(format!("error[vm.distributed_storage.operation]: unsupported operation or arity `{operation}`/{}", args.len()).into()),
        };
        self.insert(owner, resource)
    }
}

fn invalid_reply() -> VmRuntimeError {
    "error[vm.distributed_storage.reply]: invalid or unsupported worker reply".into()
}

fn policy_field(policy: &Policy, field: &str) -> ReplValue {
    match field {
        "policy_name" => ReplValue::String(policy.name.clone()),
        "policy_mode_kind" => ReplValue::String(policy.mode.into()),
        "policy_available" => ReplValue::Bool(policy.available),
        _ => ReplValue::Bool(false), // Replication requires independently verified peer support.
    }
}
