//! Actor-owned fault monitors and immutable local scheduling plans.

use super::{PureNativeCapabilityRequest, ReplValue, VmRuntimeError, VmRuntimeResult};
use crate::runtime::vm::coordination_membership::{
    VmClusterNodeSnapshot as Node, VmClusterNodeState,
};
use crate::runtime::vm::distributed_scheduler::fault::{
    distributed_fault_compatibility, VmDistributedCompatibilityOutcome,
    VmDistributedFailureEnvelope as Failure, VmDistributedFailureKind as FailureKind,
    VmDistributedFaultPolicy as FaultPolicy, VmDistributedFaultState as State,
    VmDistributedFaultTransition as Transition, VmDistributedHeartbeatObservation,
};
use crate::runtime::vm::distributed_scheduler::{
    VmDistributedScheduler as Scheduler, VmMigrationIntent as Migration,
    VmMigrationOutcome as Outcome, VmMigrationPhase as Phase, VmPlacementDecision as Placement,
    VmPlacementFallback, VmPlacementPolicy as Policy, VmSchedulerEvent as Event,
    VmSchedulingLimits,
};
use crate::terlan_native_boundary::handle::NativeBoundaryHandle;
use crate::terlan_native_boundary::resource::{ResourceError, ResourceRegistry};
use std::collections::HashMap;

mod fault;
mod fault_descriptors;
mod scheduler;
mod values;
use values::*;

#[cfg(test)]
mod tests;

/// A single registry prevents cross-kind aliases, including Fault/Scheduler Policy.
#[derive(Clone)]
enum Resource {
    Node(Node),
    Scheduler(Box<Scheduler>),
    Policy(Policy),
    Placement(Placement),
    PlacementResult(ReplValue, ReplValue),
    Migration(Migration),
    MigrationResult(ReplValue, ReplValue),
    Outcome(Outcome),
    OutcomeResult(ReplValue, ReplValue),
    Event(Event),
    FaultPolicy(FaultPolicy),
    Monitor(Box<Scheduler>),
    State(String, State),
    Transition(Transition),
    Recovery(String, u64),
    Failure(Failure),
    Rollback(Failure),
}

impl Resource {
    fn identity(&self) -> (&'static str, &'static str) {
        match self {
            Self::Node(_) => ("Scheduler", "Node"),
            Self::Scheduler(_) => ("Scheduler", "Scheduler"),
            Self::Policy(_) => ("Scheduler", "Policy"),
            Self::Placement(_) => ("Scheduler", "Placement"),
            Self::PlacementResult(..) => ("Scheduler", "PlacementResult"),
            Self::Migration(_) => ("Scheduler", "Migration"),
            Self::MigrationResult(..) => ("Scheduler", "MigrationResult"),
            Self::Outcome(_) => ("Scheduler", "MigrationOutcome"),
            Self::OutcomeResult(..) => ("Scheduler", "MigrationOutcomeResult"),
            Self::Event(_) => ("Scheduler", "Event"),
            Self::FaultPolicy(_) => ("Fault", "Policy"),
            Self::Monitor(_) => ("Fault", "Monitor"),
            Self::State(..) => ("Fault", "State"),
            Self::Transition(_) => ("Fault", "Transition"),
            Self::Recovery(..) => ("Fault", "Recovery"),
            Self::Failure(_) => ("Fault", "Failure"),
            Self::Rollback(_) => ("Fault", "Rollback"),
        }
    }
}

/// Immutable observations retain value identity when returned by replay.
#[derive(Clone, Eq, PartialEq, Hash)]
enum DescriptorKey {
    Transition(Transition),
    Event(Event),
    State(String, State),
}

/// Resources never leave the VM owner or imply live actor/network migration.
#[derive(Default)]
pub(super) struct VmSchedulingRuntime {
    resources: ResourceRegistry<Resource>,
    descriptors: HashMap<(u64, DescriptorKey), NativeBoundaryHandle>,
}

impl VmSchedulingRuntime {
    pub(super) fn call(
        &mut self,
        owner: u64,
        request: &PureNativeCapabilityRequest,
    ) -> VmRuntimeResult<ReplValue> {
        let args = request
            .package_arguments
            .as_deref()
            .ok_or("error[vm.scheduling.arguments]: expected typed arguments")?;
        if let Some(operation) = request.operation.strip_prefix("std.vm.fault.") {
            self.call_fault(owner, operation, args)
        } else if let Some(operation) = request.operation.strip_prefix("std.vm.scheduler.") {
            self.call_scheduler(owner, operation, args)
        } else {
            Err(operation_error())
        }
    }

    fn insert(&mut self, owner: u64, value: Resource) -> VmRuntimeResult<ReplValue> {
        let (module, kind) = value.identity();
        let identity = format!("std.vm.{module}.{kind}");
        let key = match &value {
            Resource::Transition(value) => Some(DescriptorKey::Transition(value.clone())),
            Resource::Event(value) => Some(DescriptorKey::Event(value.clone())),
            Resource::State(node, state) => Some(DescriptorKey::State(node.clone(), *state)),
            _ => None,
        }
        .map(|key| (owner, key));
        if let Some(handle) = key.as_ref().and_then(|key| self.descriptors.get(key)) {
            return super::direct_std::native_handle_value(owner, *handle, &identity);
        }
        let handle = self
            .resources
            .insert_for_owner(owner, value)
            .map_err(resource_error)?;
        if let Some(key) = key {
            self.descriptors.insert(key, handle);
        }
        super::direct_std::native_handle_value(owner, handle, &identity)
    }

    fn handle(&self, owner: u64, value: &ReplValue) -> VmRuntimeResult<NativeBoundaryHandle> {
        let ReplValue::Record { name, fields } = value else {
            return Err(kind_error());
        };
        let (handle, claimed_kind, claimed_owner) = super::direct_std::native_handle(fields)
            .ok_or("error[vm.scheduling.handle]: incomplete resource handle")??;
        let resource = self
            .resources
            .get_for_owner(handle, owner)
            .map_err(resource_error)?;
        let (module, kind) = resource.identity();
        if claimed_owner != owner.to_string()
            || name != kind
            || claimed_kind != format!("std.vm.{module}.{kind}")
        {
            return Err(
                "error[vm.scheduling.handle]: foreign owner or invalid resource identity".into(),
            );
        }
        Ok(handle)
    }

    fn resource(&self, owner: u64, value: &ReplValue) -> VmRuntimeResult<&Resource> {
        self.resources
            .get_for_owner(self.handle(owner, value)?, owner)
            .map_err(resource_error)
    }

    fn monitor_mut(&mut self, owner: u64, value: &ReplValue) -> VmRuntimeResult<&mut Scheduler> {
        let handle = self.handle(owner, value)?;
        match self
            .resources
            .get_mut_for_owner(handle, owner)
            .map_err(resource_error)?
        {
            Resource::Monitor(monitor) => Ok(monitor),
            _ => Err(kind_error()),
        }
    }

    pub(super) fn close_owner(&mut self, owner: u64) {
        self.resources.dispose_owner(owner);
        self.descriptors
            .retain(|(candidate, _), _| *candidate != owner);
    }
}

fn kind_error() -> VmRuntimeError {
    "error[vm.scheduling.kind]: unexpected resource kind".into()
}
fn operation_error() -> VmRuntimeError {
    "error[vm.scheduling.operation]: unsupported operation or arity".into()
}
fn resource_error(error: ResourceError) -> VmRuntimeError {
    format!("error[{}]: {}", error.code(), error.message()).into()
}
