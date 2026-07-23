//! Supervisor-owned lifecycle for one native execution shard.

use super::native_image_diagnostics::{
    VmNativeGenerationReferenceSnapshot, VmNativeImageDiagnosticMetadata,
};
use super::{
    execution_shard_epoch::{
        VmShardEpochError, VmShardEpochFence, VmShardEpochOperation, VmShardOperationAdmission,
        VmShardOperationCommit,
    },
    execution_shard_protocol::{VmExecutionShardId, VmSealedShardImage, VmShardEpoch},
    supervision::VmRestartBackoffSchedule,
};

/// Protocol version understood by the supervisor and execution shard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmShardProtocolVersion(u16);

impl VmShardProtocolVersion {
    /// Creates a nonzero protocol version.
    pub(crate) const fn new(value: u16) -> Result<Self, VmShardSupervisorError> {
        if value == 0 {
            return Err(VmShardSupervisorError::ZeroProtocolVersion);
        }
        Ok(Self(value))
    }

    /// Returns the protocol version number.
    pub(crate) const fn as_u16(self) -> u16 {
        self.0
    }
}

/// Observable phase of one supervised execution shard.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmShardPhase {
    /// No protocol exchange has started.
    Created,
    /// The shard and supervisor are negotiating a protocol.
    Negotiating,
    /// Protocol negotiation succeeded and an image may be admitted.
    Admitting,
    /// An image and epoch exist, but readiness is not acknowledged.
    AwaitingReady,
    /// Admission and readiness completed and the shard accepts routes.
    Ready,
    /// New routes are closed while accepted work completes.
    Draining,
    /// Graceful termination has been requested.
    Stopping,
    /// The shard terminated without a pending restart.
    Stopped,
    /// A crashed shard is waiting for its restart deadline.
    RestartBackoff,
    /// The restart budget is exhausted permanently.
    Quarantined,
}

/// How a terminal shard process was stopped.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmShardTermination {
    /// The shard acknowledged an orderly stop.
    Graceful,
    /// The supervisor forcibly terminated the shard.
    #[cfg(test)]
    Forced,
}

/// Last accepted health or progress sequence numbers.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmShardSignalProgress {
    /// Last accepted health sequence, or zero before the first signal.
    pub(crate) health: u64,
    /// Last accepted work-progress sequence, or zero before the first signal.
    pub(crate) work: u64,
}

/// Immutable report for one shard crash.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmShardCrashReport {
    /// Stable shard process slot that owned the failed generation.
    pub(crate) shard_id: VmExecutionShardId,
    /// Epoch active when the shard crashed, when admission had completed.
    pub(crate) epoch: Option<VmShardEpoch>,
    /// Non-empty crash reason supplied by the supervisor boundary.
    pub(crate) reason: String,
    /// Monotonic tick at which the crash was observed.
    pub(crate) observed_tick: u64,
    /// Admitted image identity and lifetime captured at the crash boundary.
    pub(crate) native_image: Option<VmNativeImageDiagnosticMetadata>,
}

/// Restart and protocol policy for one execution shard.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmShardSupervisorPolicy {
    /// Exact protocol version accepted by this supervisor.
    pub(crate) protocol: VmShardProtocolVersion,
    /// Number of restarts permitted before terminal quarantine.
    pub(crate) restart_budget: u32,
    /// Shared VM exponential restart schedule.
    pub(crate) restart_backoff: VmRestartBackoffSchedule,
}

impl VmShardSupervisorPolicy {
    /// Creates an explicit shard lifecycle policy.
    pub(crate) const fn new(
        protocol: VmShardProtocolVersion,
        restart_budget: u32,
        restart_backoff: VmRestartBackoffSchedule,
    ) -> Self {
        Self {
            protocol,
            restart_budget,
            restart_backoff,
        }
    }
}

/// Typed rejection from the supervisor/shard lifecycle state machine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmShardSupervisorError {
    /// A protocol used the reserved zero version.
    ZeroProtocolVersion,
    /// The proposed protocol does not match supervisor policy.
    ProtocolMismatch {
        /// Version required by the supervisor.
        expected: u16,
        /// Version proposed by the shard.
        actual: u16,
    },
    /// An operation is not legal in the current phase.
    InvalidTransition {
        /// Current shard phase.
        phase: VmShardPhase,
        /// Stable operation name.
        operation: &'static str,
    },
    /// A signal or acknowledgement named another shard generation.
    EpochMismatch {
        /// Current admitted generation.
        expected: VmShardEpoch,
        /// Generation supplied by the shard.
        actual: VmShardEpoch,
    },
    /// A health or progress signal did not advance its sequence.
    NonMonotonicSignal {
        /// Stable signal kind.
        signal: &'static str,
        /// Last accepted sequence.
        previous: u64,
        /// Rejected sequence.
        actual: u64,
    },
    /// A crash report omitted its reason.
    EmptyCrashReason,
    /// Restart deadline addition exceeded the monotonic tick range.
    RestartDeadlineOverflow,
    /// A restart was requested before the backoff deadline.
    RestartBackoffActive {
        /// Earliest allowed restart tick.
        deadline_tick: u64,
        /// Tick supplied by the caller.
        now_tick: u64,
    },
    /// The shard epoch space was exhausted.
    EpochExhausted,
    /// An epoch-bound runtime operation failed admission or completion.
    EpochOperation(
        /// Exact epoch or replay-ledger rejection.
        VmShardEpochError,
    ),
}

/// Supervisor-owned state machine for one execution shard process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmExecutionShardSupervisor {
    /// Stable identity retained across process generations.
    shard_id: VmExecutionShardId,
    /// Protocol, restart-budget, and backoff policy.
    policy: VmShardSupervisorPolicy,
    /// Current externally observable lifecycle phase.
    phase: VmShardPhase,
    /// Image admitted for the current generation.
    image: Option<VmSealedShardImage>,
    /// Current admitted generation identity.
    epoch: Option<VmShardEpoch>,
    /// Cross-restart ingress and effect deduplication ledger.
    epoch_fence: Option<VmShardEpochFence>,
    /// Allocation-free slot for one owner-local operation whose identity can
    /// never be resubmitted by an external caller.
    internal_operation: Option<VmShardEpochOperation>,
    /// Numeric identity reserved for the next admission.
    next_epoch: u64,
    /// Monotonic health and work sequence state.
    signals: VmShardSignalProgress,
    /// Restart budget units consumed across generations.
    restart_count: u32,
    /// Earliest restart tick while in backoff.
    restart_deadline_tick: Option<u64>,
    /// Most recent crash observed by this supervisor.
    last_crash: Option<VmShardCrashReport>,
    /// Terminal process-stop outcome when no restart is pending.
    termination: Option<VmShardTermination>,
}

impl VmExecutionShardSupervisor {
    /// Creates a dormant shard lifecycle with no admitted image or epoch.
    pub(crate) fn new(shard_id: VmExecutionShardId, policy: VmShardSupervisorPolicy) -> Self {
        Self {
            shard_id,
            policy,
            phase: VmShardPhase::Created,
            image: None,
            epoch: None,
            epoch_fence: None,
            internal_operation: None,
            next_epoch: 1,
            signals: VmShardSignalProgress::default(),
            restart_count: 0,
            restart_deadline_tick: None,
            last_crash: None,
            termination: None,
        }
    }

    /// Returns this shard's stable supervisor identity.
    pub(crate) fn shard_id(&self) -> &VmExecutionShardId {
        &self.shard_id
    }

    /// Returns the current lifecycle phase.
    pub(crate) const fn phase(&self) -> VmShardPhase {
        self.phase
    }

    /// Returns whether routes may atomically enter this shard generation.
    pub(crate) const fn is_routable(&self) -> bool {
        matches!(self.phase, VmShardPhase::Ready) && self.image.is_some() && self.epoch.is_some()
    }

    /// Returns the current admitted generation, when one exists.
    pub(crate) const fn epoch(&self) -> Option<VmShardEpoch> {
        self.epoch
    }

    /// Returns the admitted sealed image, when one exists.
    pub(crate) const fn image(&self) -> Option<&VmSealedShardImage> {
        self.image.as_ref()
    }

    /// Returns the last accepted health and work signal sequences.
    #[cfg(test)]
    pub(crate) const fn signals(&self) -> VmShardSignalProgress {
        self.signals
    }

    /// Returns the number of restart budget units consumed.
    pub(crate) const fn restart_count(&self) -> u32 {
        self.restart_count
    }

    /// Returns the active restart deadline, when waiting in backoff.
    #[cfg(test)]
    pub(crate) const fn restart_deadline_tick(&self) -> Option<u64> {
        self.restart_deadline_tick
    }

    /// Returns the most recent crash report.
    #[cfg(test)]
    pub(crate) const fn last_crash(&self) -> Option<&VmShardCrashReport> {
        self.last_crash.as_ref()
    }

    /// Returns terminal stop mode, when the shard stopped without restart.
    #[cfg(test)]
    pub(crate) const fn termination(&self) -> Option<VmShardTermination> {
        self.termination
    }

    /// Starts protocol negotiation for a new or restarted shard process.
    pub(crate) fn begin_negotiation(&mut self) -> Result<(), VmShardSupervisorError> {
        self.require_phase(VmShardPhase::Created, "begin_negotiation")?;
        self.phase = VmShardPhase::Negotiating;
        Ok(())
    }

    /// Accepts the exact supervisor protocol and opens image admission.
    pub(crate) fn negotiate(
        &mut self,
        proposed: VmShardProtocolVersion,
    ) -> Result<(), VmShardSupervisorError> {
        self.require_phase(VmShardPhase::Negotiating, "negotiate")?;
        if proposed != self.policy.protocol {
            return Err(VmShardSupervisorError::ProtocolMismatch {
                expected: self.policy.protocol.as_u16(),
                actual: proposed.as_u16(),
            });
        }
        self.phase = VmShardPhase::Admitting;
        Ok(())
    }

    /// Admits one sealed image and assigns its next monotonic epoch.
    pub(crate) fn admit_image(
        &mut self,
        image: VmSealedShardImage,
    ) -> Result<VmShardEpoch, VmShardSupervisorError> {
        self.require_phase(VmShardPhase::Admitting, "admit_image")?;
        self.install_next_image(image)
    }

    /// Replaces a fully drained image while preserving the shard identity and epoch fence.
    pub(crate) fn replace_drained_image(
        &mut self,
        drained_epoch: VmShardEpoch,
        image: VmSealedShardImage,
    ) -> Result<VmShardEpoch, VmShardSupervisorError> {
        self.require_phase(VmShardPhase::Draining, "replace_drained_image")?;
        self.require_epoch(drained_epoch)?;
        self.install_next_image(image)
    }

    /// Atomically installs the next sealed image and advances its operation fence.
    fn install_next_image(
        &mut self,
        image: VmSealedShardImage,
    ) -> Result<VmShardEpoch, VmShardSupervisorError> {
        let epoch = VmShardEpoch::new(self.next_epoch)
            .map_err(|_| VmShardSupervisorError::EpochExhausted)?;
        let next_epoch = self
            .next_epoch
            .checked_add(1)
            .ok_or(VmShardSupervisorError::EpochExhausted)?;
        let epoch_fence = match &self.epoch_fence {
            Some(current) => {
                let mut advanced = current.clone();
                advanced
                    .advance(epoch)
                    .map_err(VmShardSupervisorError::EpochOperation)?;
                advanced
            }
            None => VmShardEpochFence::new(epoch),
        };
        self.image = Some(image);
        self.epoch = Some(epoch);
        self.epoch_fence = Some(epoch_fence);
        self.internal_operation = None;
        self.next_epoch = next_epoch;
        self.signals = VmShardSignalProgress::default();
        self.phase = VmShardPhase::AwaitingReady;
        Ok(epoch)
    }

    /// Atomically publishes the admitted generation as routable.
    pub(crate) fn acknowledge_ready(
        &mut self,
        epoch: VmShardEpoch,
    ) -> Result<(), VmShardSupervisorError> {
        self.require_phase(VmShardPhase::AwaitingReady, "acknowledge_ready")?;
        self.require_epoch(epoch)?;
        self.phase = VmShardPhase::Ready;
        Ok(())
    }

    /// Accepts a strictly advancing health signal for the active epoch.
    pub(crate) fn signal_health(
        &mut self,
        epoch: VmShardEpoch,
        sequence: u64,
    ) -> Result<(), VmShardSupervisorError> {
        self.require_active_signal_phase("signal_health")?;
        self.require_epoch(epoch)?;
        require_advancing("health", self.signals.health, sequence)?;
        self.signals.health = sequence;
        Ok(())
    }

    /// Accepts a strictly advancing work-progress signal for the active epoch.
    pub(crate) fn signal_progress(
        &mut self,
        epoch: VmShardEpoch,
        sequence: u64,
    ) -> Result<(), VmShardSupervisorError> {
        self.require_active_signal_phase("signal_progress")?;
        self.require_epoch(epoch)?;
        require_advancing("progress", self.signals.work, sequence)?;
        self.signals.work = sequence;
        Ok(())
    }

    /// Admits one exact-epoch ingress or effect before runtime mutation.
    pub(crate) fn begin_epoch_operation(
        &mut self,
        operation: VmShardEpochOperation,
    ) -> Result<VmShardOperationAdmission, VmShardSupervisorError> {
        self.require_operation_phase("begin_epoch_operation")?;
        self.epoch_fence
            .as_mut()
            .expect("operation phase requires an admitted epoch fence")
            .begin(operation)
            .map_err(VmShardSupervisorError::EpochOperation)
    }

    /// Commits one admitted operation after its mutation or effect succeeds.
    pub(crate) fn commit_epoch_operation(
        &mut self,
        operation: VmShardEpochOperation,
    ) -> Result<VmShardOperationCommit, VmShardSupervisorError> {
        self.require_operation_phase("commit_epoch_operation")?;
        self.epoch_fence
            .as_mut()
            .expect("operation phase requires an admitted epoch fence")
            .commit(operation)
            .map_err(VmShardSupervisorError::EpochOperation)
    }

    /// Admits one freshly generated owner-local operation without allocating a
    /// replay-ledger node. External ingress/effects must use `begin_epoch_operation`.
    pub(crate) fn begin_internal_operation(
        &mut self,
        operation: VmShardEpochOperation,
    ) -> Result<VmShardOperationAdmission, VmShardSupervisorError> {
        self.require_operation_phase("begin_internal_operation")?;
        self.require_epoch(operation.epoch)?;
        match self.internal_operation {
            None => {
                self.internal_operation = Some(operation);
                Ok(VmShardOperationAdmission::ExecuteFirst)
            }
            Some(current) if current == operation => {
                Ok(VmShardOperationAdmission::DuplicateSuppressed)
            }
            Some(_) => Err(VmShardSupervisorError::EpochOperation(
                VmShardEpochError::OperationIdentityConflict {
                    operation_id: operation.id,
                },
            )),
        }
    }

    /// Commits and clears the exact owner-local operation in constant space.
    pub(crate) fn commit_internal_operation(
        &mut self,
        operation: VmShardEpochOperation,
    ) -> Result<VmShardOperationCommit, VmShardSupervisorError> {
        self.require_operation_phase("commit_internal_operation")?;
        self.require_epoch(operation.epoch)?;
        match self.internal_operation {
            Some(current) if current == operation => {
                self.internal_operation = None;
                Ok(VmShardOperationCommit::Committed)
            }
            Some(_) => Err(VmShardSupervisorError::EpochOperation(
                VmShardEpochError::OperationIdentityConflict {
                    operation_id: operation.id,
                },
            )),
            None => Err(VmShardSupervisorError::EpochOperation(
                VmShardEpochError::UnknownOperation {
                    operation_id: operation.id,
                },
            )),
        }
    }

    /// Clears a failed owner-local operation before its actor is retired.
    pub(crate) fn abort_internal_operation(&mut self, operation: VmShardEpochOperation) -> bool {
        if self.internal_operation == Some(operation) {
            self.internal_operation = None;
            true
        } else {
            false
        }
    }

    /// Retires a committed VM-internal operation after progress publication.
    pub(crate) fn retire_internal_operation(&mut self, operation: VmShardEpochOperation) -> bool {
        self.epoch_fence
            .as_mut()
            .is_some_and(|fence| fence.retire_committed(operation))
    }

    /// Returns retained replay identities for bounded-state tests.
    #[cfg(test)]
    pub(crate) fn operation_count(&self) -> usize {
        usize::from(self.internal_operation.is_some())
            + self
                .epoch_fence
                .as_ref()
                .map_or(0, VmShardEpochFence::operation_count)
    }

    /// Revokes routing before accepted work begins draining.
    pub(crate) fn begin_drain(
        &mut self,
        epoch: VmShardEpoch,
    ) -> Result<(), VmShardSupervisorError> {
        self.require_phase(VmShardPhase::Ready, "begin_drain")?;
        self.require_epoch(epoch)?;
        self.phase = VmShardPhase::Draining;
        Ok(())
    }

    /// Quarantines a draining generation with its exact retained lifetime proof.
    pub(crate) fn quarantine_drain_timeout_with_lifetime(
        &mut self,
        epoch: VmShardEpoch,
        reason: impl Into<String>,
        observed_tick: u64,
        references: &VmNativeGenerationReferenceSnapshot,
    ) -> Result<(), VmShardSupervisorError> {
        self.require_phase(VmShardPhase::Draining, "quarantine_drain_timeout")?;
        self.require_epoch(epoch)?;
        let reason = reason.into();
        if reason.trim().is_empty() {
            return Err(VmShardSupervisorError::EmptyCrashReason);
        }
        self.last_crash = Some(VmShardCrashReport {
            shard_id: self.shard_id.clone(),
            epoch: Some(epoch),
            reason,
            observed_tick,
            native_image: self.crash_image_metadata(references),
        });
        self.restart_deadline_tick = None;
        self.phase = VmShardPhase::Quarantined;
        Ok(())
    }

    /// Requests graceful process termination after draining.
    pub(crate) fn request_graceful_stop(
        &mut self,
        epoch: VmShardEpoch,
    ) -> Result<(), VmShardSupervisorError> {
        self.require_phase(VmShardPhase::Draining, "request_graceful_stop")?;
        self.require_epoch(epoch)?;
        self.phase = VmShardPhase::Stopping;
        Ok(())
    }

    /// Records the shard's graceful stop acknowledgement.
    pub(crate) fn acknowledge_stopped(
        &mut self,
        epoch: VmShardEpoch,
    ) -> Result<(), VmShardSupervisorError> {
        self.require_phase(VmShardPhase::Stopping, "acknowledge_stopped")?;
        self.require_epoch(epoch)?;
        self.phase = VmShardPhase::Stopped;
        self.termination = Some(VmShardTermination::Graceful);
        Ok(())
    }

    /// Forcibly terminates any nonterminal shard generation.
    #[cfg(test)]
    pub(crate) fn force_terminate(&mut self) -> Result<(), VmShardSupervisorError> {
        if matches!(
            self.phase,
            VmShardPhase::Stopped | VmShardPhase::Quarantined
        ) {
            return Err(VmShardSupervisorError::InvalidTransition {
                phase: self.phase,
                operation: "force_terminate",
            });
        }
        self.phase = VmShardPhase::Stopped;
        self.termination = Some(VmShardTermination::Forced);
        self.restart_deadline_tick = None;
        Ok(())
    }

    /// Records a crash and enters backoff or terminal quarantine.
    #[cfg(test)]
    pub(crate) fn report_crash(
        &mut self,
        reason: impl Into<String>,
        observed_tick: u64,
    ) -> Result<(), VmShardSupervisorError> {
        self.report_crash_with_lifetime(
            reason,
            observed_tick,
            &VmNativeGenerationReferenceSnapshot::new(),
        )
    }

    /// Records a crash with exact admitted image and generation lifetime metadata.
    pub(crate) fn report_crash_with_lifetime(
        &mut self,
        reason: impl Into<String>,
        observed_tick: u64,
        references: &VmNativeGenerationReferenceSnapshot,
    ) -> Result<(), VmShardSupervisorError> {
        if matches!(
            self.phase,
            VmShardPhase::Stopped | VmShardPhase::RestartBackoff | VmShardPhase::Quarantined
        ) {
            return Err(VmShardSupervisorError::InvalidTransition {
                phase: self.phase,
                operation: "report_crash",
            });
        }
        let reason = reason.into();
        if reason.trim().is_empty() {
            return Err(VmShardSupervisorError::EmptyCrashReason);
        }
        let restart_count = self.restart_count.saturating_add(1);
        let report = VmShardCrashReport {
            shard_id: self.shard_id.clone(),
            epoch: self.epoch,
            reason,
            observed_tick,
            native_image: self.crash_image_metadata(references),
        };
        if self.restart_count >= self.policy.restart_budget {
            self.restart_count = restart_count;
            self.last_crash = Some(report);
            self.restart_deadline_tick = None;
            self.phase = VmShardPhase::Quarantined;
            return Ok(());
        }
        let delay = self
            .policy
            .restart_backoff
            .delay_for_restart_count(restart_count);
        let deadline = observed_tick
            .checked_add(delay)
            .ok_or(VmShardSupervisorError::RestartDeadlineOverflow)?;
        self.restart_count = restart_count;
        self.last_crash = Some(report);
        self.restart_deadline_tick = Some(deadline);
        self.phase = VmShardPhase::RestartBackoff;
        Ok(())
    }

    /// Builds diagnostic metadata from the currently admitted sealed image.
    fn crash_image_metadata(
        &self,
        references: &VmNativeGenerationReferenceSnapshot,
    ) -> Option<VmNativeImageDiagnosticMetadata> {
        let image = self.image.as_ref()?;
        let epoch = self.epoch?;
        Some(
            VmNativeImageDiagnosticMetadata::new(
                image.identity(),
                *image.descriptor_digest(),
                image.continuation_ids().to_vec(),
                epoch.as_u64(),
                references,
            )
            .expect("admitted sealed image always has valid diagnostic identity"),
        )
    }

    /// Starts a fresh negotiation after the restart deadline has elapsed.
    pub(crate) fn restart_when_due(&mut self, now_tick: u64) -> Result<(), VmShardSupervisorError> {
        self.require_phase(VmShardPhase::RestartBackoff, "restart_when_due")?;
        let deadline_tick = self
            .restart_deadline_tick
            .expect("restart backoff phase owns a deadline");
        if now_tick < deadline_tick {
            return Err(VmShardSupervisorError::RestartBackoffActive {
                deadline_tick,
                now_tick,
            });
        }
        self.image = None;
        self.epoch = None;
        self.internal_operation = None;
        self.signals = VmShardSignalProgress::default();
        self.restart_deadline_tick = None;
        self.termination = None;
        self.phase = VmShardPhase::Negotiating;
        Ok(())
    }

    /// Requires one exact lifecycle phase without mutating state on failure.
    fn require_phase(
        &self,
        expected: VmShardPhase,
        operation: &'static str,
    ) -> Result<(), VmShardSupervisorError> {
        if self.phase != expected {
            return Err(VmShardSupervisorError::InvalidTransition {
                phase: self.phase,
                operation,
            });
        }
        Ok(())
    }

    /// Requires a phase in which operational signals remain meaningful.
    fn require_active_signal_phase(
        &self,
        operation: &'static str,
    ) -> Result<(), VmShardSupervisorError> {
        if !matches!(self.phase, VmShardPhase::Ready | VmShardPhase::Draining) {
            return Err(VmShardSupervisorError::InvalidTransition {
                phase: self.phase,
                operation,
            });
        }
        Ok(())
    }

    /// Requires readiness or draining for an epoch-bound runtime operation.
    fn require_operation_phase(
        &self,
        operation: &'static str,
    ) -> Result<(), VmShardSupervisorError> {
        if !matches!(self.phase, VmShardPhase::Ready | VmShardPhase::Draining) {
            return Err(VmShardSupervisorError::InvalidTransition {
                phase: self.phase,
                operation,
            });
        }
        Ok(())
    }

    /// Requires the exact currently admitted epoch.
    fn require_epoch(&self, actual: VmShardEpoch) -> Result<(), VmShardSupervisorError> {
        let expected = self.epoch.expect("epoch-bearing phase requires admission");
        if actual != expected {
            return Err(VmShardSupervisorError::EpochMismatch { expected, actual });
        }
        Ok(())
    }
}

/// Requires a signal sequence to move strictly forward.
fn require_advancing(
    signal: &'static str,
    previous: u64,
    actual: u64,
) -> Result<(), VmShardSupervisorError> {
    if actual <= previous {
        return Err(VmShardSupervisorError::NonMonotonicSignal {
            signal,
            previous,
            actual,
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "execution_shard_fault_injection_test.rs"]
mod execution_shard_fault_injection_test;
#[cfg(test)]
#[path = "execution_shard_supervisor_test.rs"]
mod execution_shard_supervisor_test;
#[cfg(test)]
#[path = "execution_shard_test_support.rs"]
mod execution_shard_test_support;
