//! Read-only debugger projections owned by one mutable execution shard.

use crate::runtime::vm::process::{VmProcessId, VmProcessSource};
use crate::runtime::vm::ReplValue;
use terlan_runtime_abi::{BoundaryError, ErrorDomain};

use super::super::{PureNativeExecutionContext, PureNativeSuspension};
use super::PureNativeExecutionShard;

fn debugger_projection_error(rendered: impl Into<String>) -> BoundaryError {
    BoundaryError::message(
        ErrorDomain::VmRuntime,
        "inspect execution shard debugger state",
        rendered,
    )
}

impl PureNativeExecutionShard {
    /// Captures deterministic process rows for debugger inspection.
    pub(crate) fn debugger_process_snapshots(
        &self,
    ) -> Vec<crate::runtime::vm::process::VmProcessSnapshot> {
        self.actors.processes().snapshots()
    }

    /// Captures deterministic VM-owned resource rows for debugger inspection.
    pub(crate) fn debugger_resource_snapshots(
        &self,
    ) -> Vec<crate::runtime::vm::resource::VmResourceSnapshot> {
        self.actors.resource_snapshots()
    }

    /// Captures deterministic VM-owned timer rows for debugger inspection.
    pub(crate) fn debugger_timer_snapshots(
        &self,
    ) -> Vec<crate::runtime::vm::timer::VmTimerSnapshot> {
        self.actors.timer_snapshots()
    }

    /// Captures a bounded mailbox without changing selective-receive state.
    pub(crate) fn debugger_mailbox_snapshot(
        &self,
        owner: VmProcessId,
        limit: usize,
    ) -> Result<crate::runtime::vm::process::VmMailboxSnapshot, BoundaryError> {
        self.actors
            .processes()
            .mailbox_snapshot(owner, limit)
            .map_err(|_| {
                debugger_projection_error(format!(
                    "error[vm.debugger.process_missing]: process {} does not exist",
                    owner.as_u64()
                ))
            })
    }

    /// Captures links, monitors, and trap-exit state for one debug actor.
    pub(crate) fn debugger_failure_snapshot(
        &self,
        owner: VmProcessId,
    ) -> Result<crate::runtime::vm::failure::VmFailureProcessSnapshot, BoundaryError> {
        self.actors
            .failure_snapshot(owner)
            .map_err(debugger_projection_error)
    }

    /// Captures bounded supervisor crash/restart evidence for this shard.
    pub(crate) fn debugger_supervisor_history(&self) -> Vec<String> {
        self.supervisor
            .crash_history()
            .map(|report| {
                format!(
                    "shard={}:epoch={}:tick={}:reason={}",
                    report.shard_id.as_str(),
                    report
                        .epoch
                        .map_or_else(|| "none".to_string(), |epoch| epoch.as_u64().to_string()),
                    report.observed_tick,
                    report.reason
                )
            })
            .collect()
    }

    /// Reconstructs typed capture values without consuming the continuation.
    pub(crate) fn debugger_capture_values(
        &mut self,
        owner: VmProcessId,
        suspension: &PureNativeSuspension,
    ) -> Result<Vec<ReplValue>, BoundaryError> {
        if suspension.owner_id() != owner.as_u64() {
            return Err(debugger_projection_error(format!(
                "error[vm.debugger.capture_owner]: process {} cannot inspect owner {}",
                owner.as_u64(),
                suspension.owner_id()
            )));
        }
        let types = suspension.debugger_capture_types();
        let words = self
            .execution
            .debugger_continuation_capture_words(
                owner.as_u64(),
                suspension.continuation_id(),
                types,
                suspension.debugger_capture_slots(),
            )
            .map_err(debugger_projection_error)?;
        let boundary = &self.boundary;
        let context = PureNativeExecutionContext::new(owner, &mut self.execution);
        types
            .iter()
            .zip(words)
            .map(|(boundary_type, word)| {
                boundary
                    .debugger_decode_capture(&context, boundary_type, word)
                    .map_err(debugger_projection_error)
            })
            .collect()
    }

    /// Publishes a stopped continuation's source location to VM inspection.
    pub(crate) fn debugger_set_location(
        &mut self,
        owner: VmProcessId,
        source: VmProcessSource,
        instruction_offset: usize,
    ) -> Result<(), BoundaryError> {
        self.actors
            .set_debugger_location(owner, source, instruction_offset)
            .map_err(debugger_projection_error)
    }
}
