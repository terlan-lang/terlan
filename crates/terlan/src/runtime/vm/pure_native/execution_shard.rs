//! Same-shard ownership for ordinary native actor execution.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::runtime::vm::actor::VmActorRuntime;
use crate::runtime::vm::execution_shard_epoch::{
    VmShardEpochOperation, VmShardOperationAdmission, VmShardOperationCommit, VmShardOperationId,
    VmShardOperationKind, VmShardReplayPolicy,
};
use crate::runtime::vm::execution_shard_protocol::{
    VmExecutionShardId, VmSealedShardImage, VmShardEpoch,
};
use crate::runtime::vm::execution_shard_supervisor::{
    VmExecutionShardSupervisor, VmShardProtocolVersion, VmShardSupervisorPolicy,
};
use crate::runtime::vm::http_session::VmHttpSessionService;
use crate::runtime::vm::native_image_diagnostics::VmNativeImageDiagnosticMetadata;
use crate::runtime::vm::process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState};
use crate::runtime::vm::supervision::VmRestartBackoffSchedule;
use crate::runtime::vm::support_bundle::VmNativeSupportBundle;
use crate::runtime::vm::ReplValue;

use super::{
    PureNativeBoundary, PureNativeExecution, PureNativeExecutionContext,
    PureNativeExecutionRuntime, PureNativeIoWait, PureNativeIoWake, PureNativeSuspension,
    VmNativeGenerationReferenceClass, VmNativeGenerationReferenceSnapshot,
};

/// Version of the in-process supervisor-to-shard lifecycle contract.
const LOCAL_SHARD_PROTOCOL_VERSION: u16 = 1;
/// Number of abnormal shard restarts admitted before quarantine.
const LOCAL_SHARD_RESTART_BUDGET: u32 = 3;
/// Initial caller-owned monotonic-tick delay before crash recovery.
const LOCAL_SHARD_RESTART_INITIAL_TICKS: u64 = 10;
/// Maximum caller-owned monotonic-tick delay before crash recovery.
const LOCAL_SHARD_RESTART_MAX_TICKS: u64 = 1_000;

/// One observable native dispatch step executed inside the owning shard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NativeShardDispatchEvent {
    /// A generated export entered directly from its actor.
    Entry {
        /// Actor that owns the generated entry call.
        owner: VmProcessId,
    },
    /// A generated continuation resumed directly for its exact owner.
    Resume {
        /// Actor that owns the parked continuation.
        owner: VmProcessId,
        /// Stable generated continuation identity.
        continuation_id: u64,
    },
    /// The generated call returned a public runtime value.
    Complete {
        /// Actor whose generated call completed.
        owner: VmProcessId,
    },
}

/// Immutable admitted image factory shared by independently mutable shards.
#[derive(Debug)]
pub(crate) struct PureNativeExecutionImage {
    /// Admitted image descriptor and immutable direct backend.
    boundary: PureNativeBoundary,
    /// Empty managed metadata template sharing immutable layouts.
    execution: PureNativeExecutionRuntime,
    /// Monotonic identity source for independently supervised shard forks.
    next_shard_sequence: AtomicU64,
}

impl PureNativeExecutionImage {
    /// Loads and admits one image without allocating actor runtime state.
    pub(crate) fn load(path: &Path) -> Result<Self, String> {
        let (boundary, managed) = PureNativeBoundary::load_image(path)?;
        Ok(Self {
            boundary,
            execution: PureNativeExecutionRuntime::from_managed(managed),
            next_shard_sequence: AtomicU64::new(1),
        })
    }

    /// Loads an HTTP image with one shared VM-owned session actor runtime.
    pub(crate) fn load_with_http_sessions(
        path: &Path,
        sessions: VmHttpSessionService,
    ) -> Result<Self, String> {
        let (boundary, mut managed) = PureNativeBoundary::load_image(path)?;
        managed.attach_http_sessions(sessions);
        Ok(Self {
            boundary,
            execution: PureNativeExecutionRuntime::from_managed(managed),
            next_shard_sequence: AtomicU64::new(1),
        })
    }

    /// Returns whether the admitted image owns one exact export.
    pub(crate) fn has_export(&self, function: &str, arity: usize) -> bool {
        self.boundary.has_export(function, arity)
    }

    /// Creates one independently mutable actor shard over shared admitted code.
    pub(crate) fn spawn_shard(&self) -> Result<PureNativeExecutionShard, String> {
        let sequence = allocate_sequence(&self.next_shard_sequence, "image shard")?;
        let shard_id = shard_identity(self.boundary.image_identity()?, sequence)?;
        PureNativeExecutionShard::with_boundary_and_execution(
            self.boundary.fork_empty()?,
            self.execution.fork_empty(),
            shard_id,
        )
    }
}

/// Execution-shard owner for an admitted image and its local actors.
///
/// Ordinary Terlan calls enter and resume generated code through this object.
/// It deliberately has no worker connection or transport callback; isolated
/// capability workers remain a separate runtime service.
#[derive(Debug)]
pub(crate) struct PureNativeExecutionShard {
    /// Admitted in-process image backend.
    boundary: PureNativeBoundary,
    /// Actor scheduler and VM services owned by this shard.
    actors: VmActorRuntime,
    /// Mutable actor heaps and continuations owned by this shard alone.
    execution: PureNativeExecutionRuntime,
    /// Ordered proof that entry and resume stayed on the direct path.
    trace: Vec<NativeShardDispatchEvent>,
    /// Calls that completed through actor-owned execution.
    completed_call_count: u64,
    /// Active admission, readiness, replacement, crash, and stop lifecycle.
    supervisor: VmExecutionShardSupervisor,
    /// Monotonic identity source for independently supervised child forks.
    #[cfg(test)]
    next_fork_sequence: AtomicU64,
    /// Monotonic identity source for exact-epoch native execution steps.
    next_operation_sequence: AtomicU64,
    /// Runtime-owned pins not discoverable from actor tables directly.
    generation_pins: BTreeMap<VmNativeGenerationReferenceClass, usize>,
}

impl PureNativeExecutionShard {
    /// Loads an admitted image into one local execution shard.
    pub(crate) fn load_image(path: &Path) -> Result<Self, String> {
        PureNativeExecutionImage::load(path)?.spawn_shard()
    }

    /// Creates a shard from one boundary and its admitted execution metadata.
    fn with_boundary_and_execution(
        boundary: PureNativeBoundary,
        execution: PureNativeExecutionRuntime,
        shard_id: VmExecutionShardId,
    ) -> Result<Self, String> {
        let sealed_image = boundary.sealed_image()?;
        let supervisor = admit_supervisor(shard_id, sealed_image)?;
        Ok(Self {
            boundary,
            actors: VmActorRuntime::default(),
            execution,
            trace: Vec::new(),
            completed_call_count: 0,
            supervisor,
            #[cfg(test)]
            next_fork_sequence: AtomicU64::new(1),
            next_operation_sequence: AtomicU64::new(1),
            generation_pins: BTreeMap::new(),
        })
    }

    /// Returns whether the admitted image owns one exact export.
    pub(crate) fn has_export(&self, function: &str, arity: usize) -> bool {
        self.boundary.has_export(function, arity)
    }

    /// Returns the whole-image digest bound to the loaded executable mapping.
    pub(crate) fn whole_image_digest(&self) -> Result<[u8; 32], String> {
        self.boundary.whole_image_digest()
    }

    /// Starts one generated entry under a newly allocated local actor.
    pub(crate) fn begin_call(
        &mut self,
        function: &str,
        args: &[ReplValue],
    ) -> Result<(VmProcessId, PureNativeExecution), String> {
        self.require_routable("begin_call")?;
        let operation = self.begin_epoch_operation(
            "begin_call",
            VmShardOperationKind::ActorRoute,
            VmShardReplayPolicy::AtMostOnce,
        )?;
        let owner = self.actors.spawn_root(call_source(function, args.len()));
        self.trace.push(NativeShardDispatchEvent::Entry { owner });
        let execution = {
            let mut context = PureNativeExecutionContext::new(owner, &mut self.execution);
            match self
                .boundary
                .begin_call_for_actor(&mut self.actors, &mut context, function, args)
            {
                Ok(execution) => execution,
                Err(error) => {
                    let cleanup = self.finish_owner(owner, VmExitReason::Error(error.clone()));
                    return match cleanup {
                        Ok(()) => Err(error),
                        Err(cleanup_error) => Err(format!(
                            "{error}; error[execution_shard.cleanup]: {cleanup_error}"
                        )),
                    };
                }
            }
        };
        self.record_completion(owner, &execution);
        self.commit_epoch_operation(operation)?;
        Ok((owner, execution))
    }

    /// Resumes one parked generated continuation for its exact local actor.
    pub(crate) fn resume_call(
        &mut self,
        owner: VmProcessId,
        suspension: PureNativeSuspension,
    ) -> Result<PureNativeExecution, String> {
        self.resume_owned_call(owner, suspension, None)
    }

    /// Resumes one parked generated continuation from an exact typed VM I/O wake.
    pub(crate) fn resume_io_call(
        &mut self,
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wake: PureNativeIoWake,
    ) -> Result<PureNativeExecution, String> {
        self.resume_owned_call(owner, suspension, Some(wake))
    }

    /// Derives one typed I/O wait under this shard's exact identity.
    pub(crate) fn io_wait(
        &self,
        owner: VmProcessId,
        suspension: &PureNativeSuspension,
    ) -> Result<PureNativeIoWait, String> {
        self.require_active_epoch("io_wait")?;
        PureNativeIoWait::from_suspension(self.supervisor.shard_id().clone(), owner, suspension)
    }

    /// Applies one authorized continuation resume under this shard's active epoch.
    fn resume_owned_call(
        &mut self,
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wake: Option<PureNativeIoWake>,
    ) -> Result<PureNativeExecution, String> {
        self.require_active_epoch("resume_call")?;
        if suspension.owner_id() != owner.as_u64() {
            return Err(format!(
                "error[pure_native_owner]: actor {} cannot resume owner {}",
                owner.as_u64(),
                suspension.owner_id()
            ));
        }
        if let Some(wake) = &wake {
            if let Err(error) =
                wake.wait()
                    .validate_suspension(self.supervisor.shard_id(), owner, &suspension)
            {
                let cleanup = self.finish_owner(owner, VmExitReason::Error(error.clone()));
                return match cleanup {
                    Ok(()) => Err(error),
                    Err(cleanup_error) => Err(format!(
                        "{error}; error[execution_shard.cleanup]: {cleanup_error}"
                    )),
                };
            }
        }
        let operation = self.begin_epoch_operation(
            "resume_call",
            VmShardOperationKind::ContinuationResume,
            VmShardReplayPolicy::AtMostOnce,
        )?;
        self.trace.push(NativeShardDispatchEvent::Resume {
            owner,
            continuation_id: suspension.continuation_id(),
        });
        let execution = {
            let mut context = PureNativeExecutionContext::new(owner, &mut self.execution);
            match wake {
                Some(wake) => self.boundary.resume_io_for_actor(
                    &mut self.actors,
                    &mut context,
                    suspension,
                    wake,
                ),
                None => self.boundary.resume_transition_for_actor(
                    &mut self.actors,
                    &mut context,
                    suspension,
                ),
            }
        };
        let execution = match execution {
            Ok(execution) => execution,
            Err(error) => {
                let cleanup = self.finish_owner(owner, VmExitReason::Error(error.clone()));
                return match cleanup {
                    Ok(()) => Err(error),
                    Err(cleanup_error) => Err(format!(
                        "{error}; error[execution_shard.cleanup]: {cleanup_error}"
                    )),
                };
            }
        };
        self.record_completion(owner, &execution);
        self.commit_epoch_operation(operation)?;
        Ok(execution)
    }

    /// Executes immediately serviceable local transitions to a final value.
    pub(crate) fn call(&mut self, function: &str, args: &[ReplValue]) -> Result<ReplValue, String> {
        let (owner, mut execution) = self.begin_call(function, args)?;
        loop {
            execution = match execution {
                PureNativeExecution::Complete(value) => {
                    self.finish_owner(owner, VmExitReason::Normal)?;
                    return Ok(value);
                }
                PureNativeExecution::Suspended(suspension) => {
                    self.resume_call(owner, suspension)?
                }
            };
        }
    }

    /// Completes one externally driven call and releases its actor-owned state.
    pub(crate) fn finish_completed_call(&mut self, owner: VmProcessId) -> Result<(), String> {
        self.finish_owner(owner, VmExitReason::Normal)
    }

    /// Cancels one externally driven call and releases its actor-owned state.
    pub(crate) fn cancel_call(
        &mut self,
        owner: VmProcessId,
        reason: impl Into<String>,
    ) -> Result<(), String> {
        self.finish_owner(owner, VmExitReason::Error(reason.into()))
    }

    /// Returns the number of local actor calls that reached completion.
    pub(crate) fn completed_call_count(&self) -> u64 {
        self.completed_call_count
    }

    /// Returns the currently admitted native image generation.
    pub(crate) fn generation(&self) -> Result<VmShardEpoch, String> {
        self.supervisor.epoch().ok_or_else(|| {
            "error[execution_shard.lifecycle]: shard has no admitted generation".to_string()
        })
    }

    /// Pins one runtime reference that cannot be inferred from actor tables.
    pub(crate) fn pin_generation_reference(
        &mut self,
        class: VmNativeGenerationReferenceClass,
    ) -> Result<(), String> {
        let count = self.generation_pins.entry(class).or_default();
        *count = count.checked_add(1).ok_or_else(|| {
            format!(
                "error[execution_shard.generation_pin]: {} reference count overflowed",
                class.name()
            )
        })?;
        Ok(())
    }

    /// Releases one externally tracked generation reference exactly once.
    pub(crate) fn release_generation_reference(
        &mut self,
        class: VmNativeGenerationReferenceClass,
    ) -> Result<(), String> {
        let Some(count) = self.generation_pins.get_mut(&class) else {
            return Err(format!(
                "error[execution_shard.generation_pin]: {} has no reference to release",
                class.name()
            ));
        };
        *count -= 1;
        if *count == 0 {
            self.generation_pins.remove(&class);
        }
        Ok(())
    }

    /// Captures every runtime reference that prevents generation unload.
    pub(crate) fn generation_references(&self) -> VmNativeGenerationReferenceSnapshot {
        let processes = self.actors.processes().snapshots();
        let live = processes
            .iter()
            .filter(|process| !matches!(process.state, VmProcessState::Exited(_)))
            .collect::<Vec<_>>();
        let parked_continuations = self
            .execution
            .pending_continuation_count()
            .max(self.actors.pending_native_continuation_count());
        let mut snapshot = VmNativeGenerationReferenceSnapshot::new();
        snapshot.record(VmNativeGenerationReferenceClass::NativeFrame, live.len());
        snapshot.record(
            VmNativeGenerationReferenceClass::ParkedContinuation,
            parked_continuations,
        );
        snapshot.record(
            VmNativeGenerationReferenceClass::ActorHeap,
            self.execution.managed_ref().actor_count(),
        );
        snapshot.record(
            VmNativeGenerationReferenceClass::MailboxFragment,
            live.iter().map(|process| process.mailbox_messages).sum(),
        );
        snapshot.record(
            VmNativeGenerationReferenceClass::Timer,
            self.actors.timer_snapshots().len(),
        );
        snapshot.record(
            VmNativeGenerationReferenceClass::Resource,
            self.actors.resource_snapshots().len(),
        );
        snapshot.record(VmNativeGenerationReferenceClass::AsyncCapabilityCallback, 0);
        snapshot.record(VmNativeGenerationReferenceClass::Debugger, 0);
        snapshot.record(VmNativeGenerationReferenceClass::CrashMetadata, 0);
        for (class, count) in &self.generation_pins {
            snapshot.add(*class, *count);
        }
        snapshot
    }

    /// Captures immutable identity and current lifetime state for diagnostics.
    pub(crate) fn native_image_diagnostics(
        &self,
    ) -> Result<VmNativeImageDiagnosticMetadata, String> {
        self.boundary
            .diagnostic_metadata(self.generation()?.as_u64(), &self.generation_references())
    }

    /// Captures one deterministic support bundle for the admitted generation.
    #[allow(dead_code)] // Used by the standalone terlan-vm binary, not terlc.
    pub(crate) fn native_support_bundle(&self) -> Result<VmNativeSupportBundle, String> {
        Ok(VmNativeSupportBundle::new(self.native_image_diagnostics()?))
    }

    /// Creates an empty actor shard sharing only immutable admitted image code.
    #[cfg(test)]
    pub(crate) fn fork_empty(&self) -> Result<Self, String> {
        self.require_routable("fork_empty")?;
        let sequence = allocate_sequence(&self.next_fork_sequence, "child shard")?;
        let shard_id = VmExecutionShardId::new(format!(
            "{}.fork-{sequence}",
            self.supervisor.shard_id().as_str()
        ))
        .map_err(|error| lifecycle_error("allocate child shard identity", error))?;
        Self::with_boundary_and_execution(
            self.boundary.fork_empty()?,
            self.execution.fork_empty(),
            shard_id,
        )
    }

    /// Gracefully releases the admitted image backend.
    pub(crate) fn shutdown(&mut self) -> Result<(), String> {
        let references = self.generation_references();
        if !references.is_quiescent() {
            return Err(pending_generation_error(self.generation()?, &references));
        }
        let epoch = self.require_routable("shutdown")?;
        self.supervisor
            .begin_drain(epoch)
            .map_err(|error| lifecycle_error("begin shutdown drain", error))?;
        if let Err(error) = self.boundary.shutdown() {
            let _ = self.supervisor.report_crash_with_lifetime(
                format!("native backend shutdown failed: {error}"),
                0,
                &references,
            );
            return Err(error);
        }
        self.supervisor
            .request_graceful_stop(epoch)
            .map_err(|error| lifecycle_error("request graceful stop", error))?;
        self.supervisor
            .acknowledge_stopped(epoch)
            .map_err(|error| lifecycle_error("acknowledge graceful stop", error))?;
        self.execution = self.execution.fork_empty();
        Ok(())
    }

    /// Replaces an idle image through drain, sealed admission, and readiness.
    pub(crate) fn replace_image(&mut self, path: &Path) -> Result<VmShardEpoch, String> {
        let (candidate_boundary, candidate_execution) = load_image_components(path)?;
        self.reject_duplicate_generation(&candidate_boundary)?;
        let references = self.generation_references();
        if !references.is_quiescent() {
            return Err(pending_generation_error(self.generation()?, &references));
        }
        self.replace_components(candidate_boundary, candidate_execution)
    }

    /// Replaces an image after quiescence or quarantines it at the deadline.
    ///
    /// Repeated calls while draining are permitted. A pending result keeps the
    /// current image loaded so accepted continuations can complete. A timeout
    /// quarantines that same loaded generation instead of unloading reachable
    /// native code.
    pub(crate) fn replace_image_before_deadline(
        &mut self,
        path: &Path,
        observed_tick: u64,
        deadline_tick: u64,
    ) -> Result<VmShardEpoch, String> {
        let (candidate_boundary, candidate_execution) = load_image_components(path)?;
        self.reject_duplicate_generation(&candidate_boundary)?;
        let current_epoch = self.generation()?;
        match self.supervisor.phase() {
            crate::runtime::vm::execution_shard_supervisor::VmShardPhase::Ready => self
                .supervisor
                .begin_drain(current_epoch)
                .map_err(|error| lifecycle_error("begin image replacement drain", error))?,
            crate::runtime::vm::execution_shard_supervisor::VmShardPhase::Draining => {}
            phase => {
                return Err(format!(
                    "error[execution_shard.lifecycle]: replace_image_before_deadline requires Ready or Draining, found {phase:?}"
                ));
            }
        }
        let references = self.generation_references();
        if !references.is_quiescent() {
            let pending = pending_generation_error(current_epoch, &references);
            if observed_tick >= deadline_tick {
                self.supervisor
                    .quarantine_drain_timeout_with_lifetime(
                        current_epoch,
                        pending.clone(),
                        observed_tick,
                        &references,
                    )
                    .map_err(|error| lifecycle_error("quarantine timed-out generation", error))?;
                return Err(format!(
                    "error[execution_shard.generation_quarantined]: deadline_tick={deadline_tick}; {pending}"
                ));
            }
            return Err(format!(
                "error[execution_shard.generation_draining]: deadline_tick={deadline_tick}; {pending}"
            ));
        }
        self.install_drained_components(current_epoch, candidate_boundary, candidate_execution)
    }

    /// Replaces this shard with already validated image components.
    fn replace_components(
        &mut self,
        candidate_boundary: PureNativeBoundary,
        candidate_execution: PureNativeExecutionRuntime,
    ) -> Result<VmShardEpoch, String> {
        self.reject_duplicate_generation(&candidate_boundary)?;
        let candidate_image = candidate_boundary.sealed_image()?;
        let current_epoch = self.require_routable("replace_image")?;
        self.supervisor
            .begin_drain(current_epoch)
            .map_err(|error| lifecycle_error("begin image replacement drain", error))?;
        self.install_drained_components_with_image(
            current_epoch,
            candidate_image,
            candidate_boundary,
            candidate_execution,
        )
    }

    /// Prevents one immutable image generation from being republished under a new epoch.
    fn reject_duplicate_generation(&self, candidate: &PureNativeBoundary) -> Result<(), String> {
        if self.boundary.is_same_generation(candidate)? {
            return Err(
                "error[execution_shard.duplicate_generation]: image identity and descriptor digest are already admitted"
                    .to_string(),
            );
        }
        Ok(())
    }

    /// Installs validated components after their predecessor has drained.
    fn install_drained_components(
        &mut self,
        current_epoch: VmShardEpoch,
        candidate_boundary: PureNativeBoundary,
        candidate_execution: PureNativeExecutionRuntime,
    ) -> Result<VmShardEpoch, String> {
        let candidate_image = candidate_boundary.sealed_image()?;
        self.install_drained_components_with_image(
            current_epoch,
            candidate_image,
            candidate_boundary,
            candidate_execution,
        )
    }

    /// Swaps one quiescent generation while preserving lifecycle ordering.
    fn install_drained_components_with_image(
        &mut self,
        current_epoch: VmShardEpoch,
        candidate_image: VmSealedShardImage,
        candidate_boundary: PureNativeBoundary,
        candidate_execution: PureNativeExecutionRuntime,
    ) -> Result<VmShardEpoch, String> {
        if let Err(error) = self.boundary.shutdown() {
            let references = self.generation_references();
            let _ = self.supervisor.report_crash_with_lifetime(
                format!("native backend replacement shutdown failed: {error}"),
                0,
                &references,
            );
            return Err(error);
        }
        let replacement_epoch = self
            .supervisor
            .replace_drained_image(current_epoch, candidate_image)
            .map_err(|error| lifecycle_error("admit replacement image", error))?;
        self.boundary = candidate_boundary;
        self.execution = candidate_execution;
        self.actors = VmActorRuntime::default();
        self.generation_pins.clear();
        self.supervisor
            .acknowledge_ready(replacement_epoch)
            .map_err(|error| lifecycle_error("publish replacement readiness", error))?;
        self.supervisor
            .signal_health(replacement_epoch, 1)
            .map_err(|error| lifecycle_error("publish replacement health", error))?;
        Ok(replacement_epoch)
    }

    /// Records an abnormal shard failure under the active supervised epoch.
    #[cfg(test)]
    pub(crate) fn report_crash(
        &mut self,
        reason: impl Into<String>,
        observed_tick: u64,
    ) -> Result<(), String> {
        let references = self.generation_references();
        self.supervisor
            .report_crash_with_lifetime(reason, observed_tick, &references)
            .map_err(|error| lifecycle_error("report native shard crash", error))
    }

    /// Recovers a crashed shard with a newly validated image after backoff.
    #[allow(dead_code)] // Recovery hook for long-lived supervisor embeddings; short-lived runners exit.
    pub(crate) fn recover_image(
        &mut self,
        path: &Path,
        now_tick: u64,
    ) -> Result<VmShardEpoch, String> {
        let (candidate_boundary, candidate_execution) = load_image_components(path)?;
        self.recover_components(candidate_boundary, candidate_execution, now_tick)
    }

    /// Recovers this shard with already validated image components.
    fn recover_components(
        &mut self,
        candidate_boundary: PureNativeBoundary,
        candidate_execution: PureNativeExecutionRuntime,
        now_tick: u64,
    ) -> Result<VmShardEpoch, String> {
        let candidate_image = candidate_boundary.sealed_image()?;
        self.supervisor
            .restart_when_due(now_tick)
            .map_err(|error| lifecycle_error("restart native shard", error))?;
        self.supervisor
            .negotiate(local_protocol_version())
            .map_err(|error| lifecycle_error("renegotiate native shard", error))?;
        let recovered_epoch = self
            .supervisor
            .admit_image(candidate_image)
            .map_err(|error| lifecycle_error("admit recovered image", error))?;
        self.boundary = candidate_boundary;
        self.execution = candidate_execution;
        self.actors = VmActorRuntime::default();
        self.supervisor
            .acknowledge_ready(recovered_epoch)
            .map_err(|error| lifecycle_error("publish recovered readiness", error))?;
        self.supervisor
            .signal_health(recovered_epoch, 1)
            .map_err(|error| lifecycle_error("publish recovered health", error))?;
        Ok(recovered_epoch)
    }

    /// Records one successful generated return exactly once.
    fn record_completion(&mut self, owner: VmProcessId, execution: &PureNativeExecution) {
        if matches!(execution, PureNativeExecution::Complete(_)) {
            self.completed_call_count = self.completed_call_count.saturating_add(1);
            self.trace
                .push(NativeShardDispatchEvent::Complete { owner });
        }
    }

    /// Exits one completed actor and releases only its backend-owned heap.
    fn finish_owner(&mut self, owner: VmProcessId, reason: VmExitReason) -> Result<(), String> {
        if reason != VmExitReason::Normal {
            let diagnostics = self.native_image_diagnostics()?;
            self.actors.set_native_image_diagnostics(diagnostics);
        }
        if self.actors.is_alive(owner) {
            self.actors.exit_actor(owner, reason)?;
        } else if self.actors.processes().get(owner).is_none() {
            return Err(format!("missing process {}", owner.as_u64()));
        }
        let mut context = PureNativeExecutionContext::new(owner, &mut self.execution);
        self.boundary.release_owner(&mut context)
    }

    /// Requires this shard to own one fully acknowledged routable image.
    fn require_routable(&self, operation: &'static str) -> Result<VmShardEpoch, String> {
        if !self.supervisor.is_routable() {
            return Err(format!(
                "error[execution_shard.lifecycle]: {operation} requires Ready, found {:?}",
                self.supervisor.phase()
            ));
        }
        self.supervisor.epoch().ok_or_else(|| {
            format!("error[execution_shard.lifecycle]: {operation} has no admitted epoch")
        })
    }

    /// Requires an admitted generation that is ready or draining accepted work.
    fn require_active_epoch(&self, operation: &'static str) -> Result<VmShardEpoch, String> {
        if !matches!(
            self.supervisor.phase(),
            crate::runtime::vm::execution_shard_supervisor::VmShardPhase::Ready
                | crate::runtime::vm::execution_shard_supervisor::VmShardPhase::Draining
        ) {
            return Err(format!(
                "error[execution_shard.lifecycle]: {operation} requires Ready or Draining, found {:?}",
                self.supervisor.phase()
            ));
        }
        self.generation()
    }

    /// Admits one direct native execution step under the active shard epoch.
    fn begin_epoch_operation(
        &mut self,
        label: &'static str,
        kind: VmShardOperationKind,
        replay_policy: VmShardReplayPolicy,
    ) -> Result<VmShardEpochOperation, String> {
        let epoch = self.require_active_epoch(label)?;
        let sequence = allocate_sequence(&self.next_operation_sequence, "shard operation")?;
        let operation_id = VmShardOperationId::new(sequence)
            .map_err(|error| lifecycle_error("allocate shard operation identity", error))?;
        let operation = VmShardEpochOperation::new(operation_id, epoch, kind, replay_policy);
        match self
            .supervisor
            .begin_epoch_operation(operation)
            .map_err(|error| lifecycle_error("admit shard operation", error))?
        {
            VmShardOperationAdmission::ExecuteFirst => Ok(operation),
            admission => Err(format!(
                "error[execution_shard.operation_admission]: fresh operation {sequence} received {admission:?}"
            )),
        }
    }

    /// Commits one direct native execution step and advances observable progress.
    fn commit_epoch_operation(&mut self, operation: VmShardEpochOperation) -> Result<(), String> {
        match self
            .supervisor
            .commit_epoch_operation(operation)
            .map_err(|error| lifecycle_error("commit shard operation", error))?
        {
            VmShardOperationCommit::Committed => self
                .supervisor
                .signal_progress(operation.epoch, operation.id.as_u64())
                .map_err(|error| lifecycle_error("publish shard operation progress", error)),
            VmShardOperationCommit::AlreadyCommitted => Err(format!(
                "error[execution_shard.operation_commit]: fresh operation {} was already committed",
                operation.id.as_u64()
            )),
        }
    }
}

/// Loads one validated boundary and its empty managed execution state.
fn load_image_components(
    path: &Path,
) -> Result<(PureNativeBoundary, PureNativeExecutionRuntime), String> {
    let (boundary, managed) = PureNativeBoundary::load_image(path)?;
    Ok((boundary, PureNativeExecutionRuntime::from_managed(managed)))
}

/// Creates and fully admits one active local shard supervisor.
fn admit_supervisor(
    shard_id: VmExecutionShardId,
    image: VmSealedShardImage,
) -> Result<VmExecutionShardSupervisor, String> {
    let protocol = local_protocol_version();
    let policy = VmShardSupervisorPolicy::new(
        protocol,
        LOCAL_SHARD_RESTART_BUDGET,
        VmRestartBackoffSchedule::exponential(
            LOCAL_SHARD_RESTART_INITIAL_TICKS,
            LOCAL_SHARD_RESTART_MAX_TICKS,
        ),
    );
    let mut supervisor = VmExecutionShardSupervisor::new(shard_id, policy);
    supervisor
        .begin_negotiation()
        .map_err(|error| lifecycle_error("begin native shard negotiation", error))?;
    supervisor
        .negotiate(protocol)
        .map_err(|error| lifecycle_error("negotiate native shard", error))?;
    let epoch = supervisor
        .admit_image(image)
        .map_err(|error| lifecycle_error("admit native shard image", error))?;
    supervisor
        .acknowledge_ready(epoch)
        .map_err(|error| lifecycle_error("publish native shard readiness", error))?;
    supervisor
        .signal_health(epoch, 1)
        .map_err(|error| lifecycle_error("publish native shard health", error))?;
    Ok(supervisor)
}

/// Returns the single protocol version implemented by the local shard path.
fn local_protocol_version() -> VmShardProtocolVersion {
    VmShardProtocolVersion::new(LOCAL_SHARD_PROTOCOL_VERSION)
        .expect("constant local shard protocol version must be nonzero")
}

/// Allocates one nonzero sequence without wrapping or reusing identities.
fn allocate_sequence(sequence: &AtomicU64, kind: &str) -> Result<u64, String> {
    sequence
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .map_err(|_| format!("error[execution_shard.identity]: {kind} identity space exhausted"))
}

/// Builds a stable shard identity from admitted image metadata and sequence.
fn shard_identity(image_identity: &str, sequence: u64) -> Result<VmExecutionShardId, String> {
    VmExecutionShardId::new(format!("{image_identity}.shard-{sequence}"))
        .map_err(|error| lifecycle_error("create native shard identity", error))
}

/// Renders one stable lifecycle diagnostic at the active shard boundary.
fn lifecycle_error(context: &str, error: impl std::fmt::Debug) -> String {
    format!("error[execution_shard.lifecycle]: {context}: {error:?}")
}

/// Renders one stable proof that a native generation remains reachable.
fn pending_generation_error(
    epoch: VmShardEpoch,
    references: &VmNativeGenerationReferenceSnapshot,
) -> String {
    format!(
        "error[execution_shard.generation_references]: epoch={} total={} references={}",
        epoch.as_u64(),
        references.total(),
        references.render_pending()
    )
}

/// Derives an inspection-only process source from a qualified export name.
fn call_source(function: &str, arity: usize) -> VmProcessSource {
    let (module, function) = function
        .rsplit_once('.')
        .unwrap_or(("native.Image", function));
    VmProcessSource::new(module, function, arity)
}

#[cfg(test)]
#[path = "execution_shard_test.rs"]
mod execution_shard_test;
