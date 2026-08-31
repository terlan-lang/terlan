//! Accelerator policy over the VM's generic asynchronous capability path.
//!
//! This implementation is kept production-compiled so the 0.0.9 accelerator
//! assembly can wire the checked contract without introducing a feature-only
//! variant. It is not part of the 0.0.7 runtime closure yet.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "the 0.0.9 accelerator assembly has not wired its worker pool yet"
    )
)]

use std::collections::BTreeMap;

use crate::accelerator_contract::{
    AcceleratorDeviceId, AcceleratorResourceClass, AcceleratorResourceHandle, AcceleratorResourceId,
};
use crate::terlan_native_boundary::term::{NativeBoundaryReplyTerm, NativeBoundaryTerm};

use super::capability_worker::{
    VmCapabilityRequestContext, VmCapabilityWorkerEventPump, VmCapabilityWorkerEventPumpEvent,
    VmCapabilityWorkerParkedRequest,
};
use super::process::VmProcessId;
use super::{VmRuntimeError, VmRuntimeResult};

/// Stable identity allocated to one admitted asynchronous accelerator operation.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmAcceleratorOperationId(u64);

impl VmAcceleratorOperationId {
    /// Returns the runtime-local numeric operation identity.
    pub(crate) const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Ownership scopes charged while an accelerator operation remains pending.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmAcceleratorOperationScope {
    /// Actor that owns the generated continuation.
    pub(crate) actor: VmProcessId,
    /// Stable supervisor identity selected by the runtime hierarchy.
    pub(crate) supervisor: String,
    /// Stable application identity selected by the assembled image.
    pub(crate) application: String,
}

impl VmAcceleratorOperationScope {
    /// Validates and creates one complete operation scope.
    pub(crate) fn new(
        actor: VmProcessId,
        supervisor: impl Into<String>,
        application: impl Into<String>,
    ) -> VmRuntimeResult<Self> {
        let supervisor = supervisor.into();
        let application = application.into();
        validate_scope_name("supervisor", &supervisor)?;
        validate_scope_name("application", &application)?;
        Ok(Self {
            actor,
            supervisor,
            application,
        })
    }
}

/// One positive operation-count and device-memory limit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmAcceleratorScopeLimit {
    /// Maximum simultaneously outstanding operations.
    pub(crate) operations: u64,
    /// Maximum device bytes reserved by outstanding operations.
    pub(crate) device_bytes: u64,
}

impl VmAcceleratorScopeLimit {
    /// Creates one nonzero bounded scope limit.
    pub(crate) const fn new(operations: u64, device_bytes: u64) -> Result<Self, &'static str> {
        if operations == 0 {
            return Err("accelerator operation limit must be positive");
        }
        if device_bytes == 0 {
            return Err("accelerator device-memory limit must be positive");
        }
        Ok(Self {
            operations,
            device_bytes,
        })
    }
}

/// Hierarchical limits applied atomically before worker publication.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmAcceleratorBudgetLimits {
    /// Limit shared by work submitted to one stream generation.
    pub(crate) stream: VmAcceleratorScopeLimit,
    /// Limit shared by all streams targeting one device.
    pub(crate) device: VmAcceleratorScopeLimit,
    /// Limit for one actor.
    pub(crate) actor: VmAcceleratorScopeLimit,
    /// Limit shared by actors under one supervisor.
    pub(crate) supervisor: VmAcceleratorScopeLimit,
    /// Limit shared by one application.
    pub(crate) application: VmAcceleratorScopeLimit,
    /// Limit shared by the runtime.
    pub(crate) runtime: VmAcceleratorScopeLimit,
}

/// Current count and memory usage for one accounting scope.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct VmAcceleratorUsage {
    operations: u64,
    device_bytes: u64,
}

/// Metadata retained beside the generic worker payload.
#[derive(Debug)]
pub(crate) struct VmAcceleratorPending<Payload> {
    id: VmAcceleratorOperationId,
    payload: Payload,
}

/// Scheduler-independent metadata for one outstanding operation.
#[derive(Clone, Debug)]
struct VmAcceleratorOperationRecord {
    scope: VmAcceleratorOperationScope,
    operation: String,
    stream: AcceleratorResourceHandle,
    device_bytes: u64,
    deadline_tick: u64,
    assignment: VmCapabilityWorkerParkedRequest,
}

/// Terminal classification shared by completion, timeout, and cancellation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmAcceleratorTerminalKind {
    /// Worker returned a package result.
    Reply,
    /// Runtime cancelled the operation explicitly.
    Cancelled,
    /// Runtime deadline expired before completion.
    TimedOut,
    /// Owning actor exited or restarted.
    OwnerExited,
    /// Exact external worker generation failed.
    WorkerFailed,
    /// Runtime shutdown drained the operation.
    RuntimeShutdown,
}

/// One terminal operation returned to the owning shard for typed resumption.
#[derive(Debug)]
pub(crate) struct VmAcceleratorTerminal<Payload> {
    /// Runtime operation identity.
    pub(crate) id: VmAcceleratorOperationId,
    /// Actor that owns the parked continuation.
    pub(crate) owner: VmProcessId,
    /// Terminal path that consumed the operation.
    pub(crate) kind: VmAcceleratorTerminalKind,
    /// Worker reply or stable failure value injected through the normal resume path.
    pub(crate) reply: NativeBoundaryReplyTerm,
    /// Exact shard-owned continuation payload retained during external work.
    pub(crate) payload: Payload,
}

/// Pointer-free inspection row for one pending operation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmAcceleratorOperationSnapshot {
    /// Runtime operation identity.
    pub(crate) id: u64,
    /// Owning actor identity.
    pub(crate) owner: u64,
    /// Owning supervisor identity.
    pub(crate) supervisor: String,
    /// Owning application identity.
    pub(crate) application: String,
    /// Package operation name.
    pub(crate) operation: String,
    /// Pointer-free stream resource slot.
    pub(crate) stream_slot: u64,
    /// Stream generation used to reject stale handles.
    pub(crate) stream_generation: u64,
    /// Device bytes reserved until the operation becomes terminal.
    pub(crate) device_bytes: u64,
    /// Absolute runtime deadline tick.
    pub(crate) deadline_tick: u64,
}

/// Complete admission request retained by the VM while publishing one operation.
pub(crate) struct VmAcceleratorSubmission {
    scope: VmAcceleratorOperationScope,
    context: VmCapabilityRequestContext,
    operation: String,
    arguments: Vec<NativeBoundaryTerm>,
    stream: AcceleratorResourceHandle,
    device_bytes: u64,
    deadline_tick: u64,
}

impl VmAcceleratorSubmission {
    /// Groups scheduler ownership, capability input, and resource accounting.
    pub(crate) fn new(
        scope: VmAcceleratorOperationScope,
        context: VmCapabilityRequestContext,
        operation: impl Into<String>,
        arguments: Vec<NativeBoundaryTerm>,
        stream: AcceleratorResourceHandle,
        device_bytes: u64,
        deadline_tick: u64,
    ) -> Self {
        Self {
            scope,
            context,
            operation: operation.into(),
            arguments,
            stream,
            device_bytes,
            deadline_tick,
        }
    }
}

/// VM-owned accelerator orchestration over the canonical capability event pump.
pub(crate) struct VmAcceleratorOperationRuntime<Payload> {
    pump: VmCapabilityWorkerEventPump<VmAcceleratorPending<Payload>>,
    limits: VmAcceleratorBudgetLimits,
    next_id: u64,
    records: BTreeMap<VmAcceleratorOperationId, VmAcceleratorOperationRecord>,
    streams: BTreeMap<AcceleratorResourceId, VmAcceleratorUsage>,
    devices: BTreeMap<AcceleratorDeviceId, VmAcceleratorUsage>,
    actors: BTreeMap<VmProcessId, VmAcceleratorUsage>,
    supervisors: BTreeMap<String, VmAcceleratorUsage>,
    applications: BTreeMap<String, VmAcceleratorUsage>,
    runtime: VmAcceleratorUsage,
}

impl<Payload> VmAcceleratorOperationRuntime<Payload> {
    /// Creates an empty accelerator runtime around the generic worker event pump.
    pub(crate) fn new(
        pump: VmCapabilityWorkerEventPump<VmAcceleratorPending<Payload>>,
        limits: VmAcceleratorBudgetLimits,
    ) -> Self {
        Self {
            pump,
            limits,
            next_id: 0,
            records: BTreeMap::new(),
            streams: BTreeMap::new(),
            devices: BTreeMap::new(),
            actors: BTreeMap::new(),
            supervisors: BTreeMap::new(),
            applications: BTreeMap::new(),
            runtime: VmAcceleratorUsage::default(),
        }
    }

    /// Admits and publishes one already-parked accelerator operation atomically.
    pub(crate) fn submit(
        &mut self,
        submission: VmAcceleratorSubmission,
        payload: Payload,
    ) -> Result<VmAcceleratorOperationId, (VmRuntimeError, Payload)> {
        let VmAcceleratorSubmission {
            scope,
            context,
            operation,
            arguments,
            stream,
            device_bytes,
            deadline_tick,
        } = submission;
        if operation.trim().is_empty() {
            return Err((
                "error[accelerator.operation]: operation must not be empty".into(),
                payload,
            ));
        }
        if deadline_tick == 0 {
            return Err((
                "error[accelerator.deadline]: deadline must be nonzero".into(),
                payload,
            ));
        }
        if let Err(error) = validate_stream(&stream) {
            return Err((error, payload));
        }
        if let Err(error) = self.reserve(&scope, &stream, device_bytes) {
            return Err((error, payload));
        }
        let id = match self.next_id.checked_add(1) {
            Some(value) => VmAcceleratorOperationId(value),
            None => {
                self.release(&scope, &stream, device_bytes);
                return Err((
                    "error[accelerator.operation_id]: identity exhausted".into(),
                    payload,
                ));
            }
        };
        let pending = VmAcceleratorPending { id, payload };
        let assignment =
            match self
                .pump
                .submit(scope.actor, context, operation.clone(), arguments, pending)
            {
                Ok(assignment) => assignment,
                Err((error, pending)) => {
                    self.release(&scope, &stream, device_bytes);
                    return Err((error.into(), pending.payload));
                }
            };
        self.next_id = id.0;
        self.records.insert(
            id,
            VmAcceleratorOperationRecord {
                scope,
                operation,
                stream,
                device_bytes,
                deadline_tick,
                assignment,
            },
        );
        Ok(id)
    }

    /// Polls one worker event and returns only terminal owner payloads.
    pub(crate) fn poll(&mut self) -> VmRuntimeResult<Vec<VmAcceleratorTerminal<Payload>>> {
        let Some(event) = self.pump.poll()? else {
            return Ok(Vec::new());
        };
        match event {
            VmCapabilityWorkerEventPumpEvent::Completed { reply, payload, .. } => {
                Ok(vec![self.finish(
                    payload,
                    VmAcceleratorTerminalKind::Reply,
                    reply,
                )?])
            }
            VmCapabilityWorkerEventPumpEvent::WorkerLost {
                reason, pending, ..
            } => pending
                .into_iter()
                .map(|(_, payload)| {
                    let reply = error_reply("worker_failure", &reason);
                    self.finish(payload, VmAcceleratorTerminalKind::WorkerFailed, reply)
                })
                .collect(),
            VmCapabilityWorkerEventPumpEvent::Ignored { .. } => Ok(Vec::new()),
        }
    }

    /// Cancels one exact operation and recovers its continuation payload.
    pub(crate) fn cancel(
        &mut self,
        id: VmAcceleratorOperationId,
    ) -> VmRuntimeResult<VmAcceleratorTerminal<Payload>> {
        self.cancel_as(id, VmAcceleratorTerminalKind::Cancelled, "cancelled")
    }

    /// Cancels all operations whose deadline is at or before the supplied tick.
    pub(crate) fn expire(
        &mut self,
        now_tick: u64,
    ) -> Vec<VmRuntimeResult<VmAcceleratorTerminal<Payload>>> {
        let expired = self
            .records
            .iter()
            .filter_map(|(id, record)| (record.deadline_tick <= now_tick).then_some(*id))
            .collect::<Vec<_>>();
        expired
            .into_iter()
            .map(|id| self.cancel_as(id, VmAcceleratorTerminalKind::TimedOut, "timeout"))
            .collect()
    }

    /// Cancels all operations owned by an actor before exit or supervision restart.
    pub(crate) fn close_owner(
        &mut self,
        owner: VmProcessId,
    ) -> Vec<VmRuntimeResult<VmAcceleratorTerminal<Payload>>> {
        let owned = self
            .records
            .iter()
            .filter_map(|(id, record)| (record.scope.actor == owner).then_some(*id))
            .collect::<Vec<_>>();
        owned
            .into_iter()
            .map(|id| self.cancel_as(id, VmAcceleratorTerminalKind::OwnerExited, "owner_exit"))
            .collect()
    }

    /// Drains all operations and requests orderly shutdown from every worker.
    pub(crate) fn shutdown(&mut self) -> (Vec<VmAcceleratorTerminal<Payload>>, Vec<String>) {
        let (pending, errors) = self.pump.shutdown();
        let terminals = pending
            .into_iter()
            .filter_map(|(_, payload)| {
                self.finish(
                    payload,
                    VmAcceleratorTerminalKind::RuntimeShutdown,
                    error_reply("runtime_shutdown", "accelerator runtime stopped"),
                )
                .ok()
            })
            .collect();
        (terminals, errors)
    }

    /// Returns deterministic pointer-free state for runtime inspection bundles.
    pub(crate) fn snapshots(&self) -> Vec<VmAcceleratorOperationSnapshot> {
        self.records
            .iter()
            .map(|(id, record)| VmAcceleratorOperationSnapshot {
                id: id.as_u64(),
                owner: record.scope.actor.as_u64(),
                supervisor: record.scope.supervisor.clone(),
                application: record.scope.application.clone(),
                operation: record.operation.clone(),
                stream_slot: record.stream.id.slot,
                stream_generation: record.stream.id.generation,
                device_bytes: record.device_bytes,
                deadline_tick: record.deadline_tick,
            })
            .collect()
    }

    /// Returns total operation and byte usage for release evidence.
    pub(crate) const fn runtime_usage(&self) -> (u64, u64) {
        (self.runtime.operations, self.runtime.device_bytes)
    }

    fn cancel_as(
        &mut self,
        id: VmAcceleratorOperationId,
        kind: VmAcceleratorTerminalKind,
        code: &'static str,
    ) -> VmRuntimeResult<VmAcceleratorTerminal<Payload>> {
        let assignment = self
            .records
            .get(&id)
            .map(|record| record.assignment.clone())
            .ok_or_else(|| format!("error[accelerator.operation_missing]: {}", id.as_u64()))?;
        let payload = self.pump.cancel(&assignment).map_err(|(error, _)| error)?;
        self.finish(
            payload,
            kind,
            error_reply(code, "accelerator operation did not complete"),
        )
    }

    fn finish(
        &mut self,
        pending: VmAcceleratorPending<Payload>,
        kind: VmAcceleratorTerminalKind,
        reply: NativeBoundaryReplyTerm,
    ) -> VmRuntimeResult<VmAcceleratorTerminal<Payload>> {
        let record = self.records.remove(&pending.id).ok_or_else(|| {
            format!(
                "error[accelerator.stale_completion]: operation {} is not pending",
                pending.id.as_u64()
            )
        })?;
        self.release(&record.scope, &record.stream, record.device_bytes);
        Ok(VmAcceleratorTerminal {
            id: pending.id,
            owner: record.scope.actor,
            kind,
            reply,
            payload: pending.payload,
        })
    }

    fn reserve(
        &mut self,
        scope: &VmAcceleratorOperationScope,
        stream: &AcceleratorResourceHandle,
        device_bytes: u64,
    ) -> VmRuntimeResult<()> {
        let device = stream
            .address_space
            .device()
            .ok_or_else(|| "error[accelerator.stream]: stream must target a device".to_string())?;
        check_usage(
            "stream",
            self.streams.get(&stream.id).copied().unwrap_or_default(),
            self.limits.stream,
            device_bytes,
        )?;
        check_usage(
            "device",
            self.devices.get(device).copied().unwrap_or_default(),
            self.limits.device,
            device_bytes,
        )?;
        check_usage(
            "actor",
            self.actors.get(&scope.actor).copied().unwrap_or_default(),
            self.limits.actor,
            device_bytes,
        )?;
        check_usage(
            "supervisor",
            self.supervisors
                .get(&scope.supervisor)
                .copied()
                .unwrap_or_default(),
            self.limits.supervisor,
            device_bytes,
        )?;
        check_usage(
            "application",
            self.applications
                .get(&scope.application)
                .copied()
                .unwrap_or_default(),
            self.limits.application,
            device_bytes,
        )?;
        check_usage("runtime", self.runtime, self.limits.runtime, device_bytes)?;
        charge(self.streams.entry(stream.id).or_default(), device_bytes);
        charge(
            self.devices.entry(device.clone()).or_default(),
            device_bytes,
        );
        charge(self.actors.entry(scope.actor).or_default(), device_bytes);
        charge(
            self.supervisors
                .entry(scope.supervisor.clone())
                .or_default(),
            device_bytes,
        );
        charge(
            self.applications
                .entry(scope.application.clone())
                .or_default(),
            device_bytes,
        );
        charge(&mut self.runtime, device_bytes);
        Ok(())
    }

    fn release(
        &mut self,
        scope: &VmAcceleratorOperationScope,
        stream: &AcceleratorResourceHandle,
        device_bytes: u64,
    ) {
        release_usage(self.streams.get_mut(&stream.id), device_bytes);
        if let Some(device) = stream.address_space.device() {
            release_usage(self.devices.get_mut(device), device_bytes);
        }
        release_usage(self.actors.get_mut(&scope.actor), device_bytes);
        release_usage(self.supervisors.get_mut(&scope.supervisor), device_bytes);
        release_usage(self.applications.get_mut(&scope.application), device_bytes);
        release_usage(Some(&mut self.runtime), device_bytes);
        self.streams
            .retain(|_, usage| *usage != VmAcceleratorUsage::default());
        self.devices
            .retain(|_, usage| *usage != VmAcceleratorUsage::default());
        self.actors
            .retain(|_, usage| *usage != VmAcceleratorUsage::default());
        self.supervisors
            .retain(|_, usage| *usage != VmAcceleratorUsage::default());
        self.applications
            .retain(|_, usage| *usage != VmAcceleratorUsage::default());
    }
}

fn validate_scope_name(kind: &str, value: &str) -> VmRuntimeResult<()> {
    if value.trim().is_empty() {
        return Err(format!("error[accelerator.scope]: {kind} must not be empty").into());
    }
    Ok(())
}

fn validate_stream(stream: &AcceleratorResourceHandle) -> VmRuntimeResult<()> {
    stream
        .validate()
        .map_err(|error| format!("error[accelerator.stream]: {error}"))?;
    if stream.class != AcceleratorResourceClass::Stream {
        return Err("error[accelerator.stream]: operation requires a stream resource".into());
    }
    Ok(())
}

fn check_usage(
    scope: &str,
    usage: VmAcceleratorUsage,
    limit: VmAcceleratorScopeLimit,
    device_bytes: u64,
) -> VmRuntimeResult<()> {
    let operations = usage.operations.checked_add(1).ok_or_else(|| {
        format!("error[accelerator.budget]: {scope} operation accounting overflow")
    })?;
    let bytes = usage
        .device_bytes
        .checked_add(device_bytes)
        .ok_or_else(|| {
            format!("error[accelerator.budget]: {scope} device-memory accounting overflow")
        })?;
    if operations > limit.operations {
        return Err(format!(
            "error[accelerator.budget]: {scope} outstanding-operation limit exceeded"
        )
        .into());
    }
    if bytes > limit.device_bytes {
        return Err(
            format!("error[accelerator.budget]: {scope} device-memory limit exceeded").into(),
        );
    }
    Ok(())
}

fn charge(usage: &mut VmAcceleratorUsage, device_bytes: u64) {
    usage.operations += 1;
    usage.device_bytes += device_bytes;
}

fn release_usage(usage: Option<&mut VmAcceleratorUsage>, device_bytes: u64) {
    let Some(usage) = usage else {
        return;
    };
    usage.operations = usage.operations.saturating_sub(1);
    usage.device_bytes = usage.device_bytes.saturating_sub(device_bytes);
}

fn error_reply(code: &str, message: &str) -> NativeBoundaryReplyTerm {
    NativeBoundaryReplyTerm::Error {
        code: code.to_string(),
        message: message.to_string(),
        offset: 0,
    }
}

#[cfg(test)]
#[path = "accelerator_operation_test.rs"]
mod accelerator_operation_test;
