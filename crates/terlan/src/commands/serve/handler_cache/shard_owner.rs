//! Thread-owned mutable execution shards for native HTTP handlers.

use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

#[cfg(test)]
use std::sync::atomic::AtomicUsize;
#[cfg(test)]
use std::sync::Barrier;

use crate::runtime::vm::actor_directory::VmMailboxWake;
use crate::runtime::vm::debugger_control::{VmDebuggerControlCommand, VmDebuggerControlSnapshot};
use crate::runtime::vm::execution_shard_protocol::VmExecutionShardId;
use crate::runtime::vm::execution_shard_protocol::VmShardEpoch;
use crate::runtime::vm::fixed_scheduler_control::VmFixedSchedulerControl;
use crate::runtime::vm::fixed_scheduler_telemetry::{
    VmFixedSchedulerEventKind, VmFixedSchedulerTelemetry, VM_FIXED_SCHEDULER_TRACE_CAPACITY,
};
use crate::runtime::vm::multicore_replay::VmMulticoreEventContext;
use crate::runtime::vm::process::VmProcessId;
use crate::runtime::vm::pure_native::{
    PureNativeActorImportFailure, PureNativeActorTransfer, PureNativeCapabilityWait,
    PureNativeExecutionShard, PureNativeIoWait, PureNativeIoWake, PureNativeSuspension,
    PureNativeTimerWait,
};
use crate::runtime::vm::scheduler::VmSchedulerClass;
use crate::runtime::vm::scheduler_topology::{VmFixedActorRoute, VmSchedulerId};
use crate::runtime::vm::work_stealing::VmSchedulerWorkSnapshot;
use crate::runtime::vm::ReplValue;
use crate::terlan_native_boundary::term::NativeBoundaryReplyTerm;

pub(super) mod capability_dispatch;
mod migration;
mod owner_loop;
mod panic_evidence;
mod replay_events;
mod runnable_queue;
mod timer_dispatch;
mod timer_queue;

use owner_loop::owner_loop;
pub(super) use panic_evidence::AotSchedulerPanicEvidence;

/// Bounded ingress prevents connection pressure from growing shard memory.
const SHARD_INBOX_CAPACITY: usize = 1_024;
const AOT_SCHEDULER_STACK_BYTES: usize = 1024 * 1024;
/// Maximum UTF-8 bytes retained from one scheduler panic payload.
const MAX_SCHEDULER_PANIC_DETAIL_BYTES: usize = 512;

/// Result of one owner-thread entry or resume command.
pub(super) enum OwnedInvocationStep {
    Complete {
        route: VmFixedActorRoute,
        value: ReplValue,
    },
    Waiting {
        route: VmFixedActorRoute,
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wait: PureNativeIoWait,
    },
    CapabilityWaiting {
        route: VmFixedActorRoute,
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wait: PureNativeCapabilityWait,
    },
    TimerWaiting {
        route: VmFixedActorRoute,
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wait: PureNativeTimerWait,
        due: Instant,
    },
}

/// Complete generated runnable state moving between scheduler owners.
pub(super) struct OwnedRunnableTransfer {
    source: VmFixedActorRoute,
    destination: VmFixedActorRoute,
    owner: VmProcessId,
    class: VmSchedulerClass,
    suspension: PureNativeSuspension,
    enqueued_at: Instant,
    reply: SyncSender<Result<OwnedInvocationStep, String>>,
    actor: PureNativeActorTransfer,
    replay_context: VmMulticoreEventContext,
}

impl OwnedRunnableTransfer {
    /// Returns the route already published for destination execution.
    pub(super) const fn destination(&self) -> VmFixedActorRoute {
        self.destination
    }
}

/// Destination rejection retaining every runnable actor component.
pub(super) struct OwnedRunnableImportFailure {
    reason: String,
    transfer: Option<OwnedRunnableTransfer>,
}

impl OwnedRunnableImportFailure {
    /// Returns the stable destination or owner-channel rejection.
    pub(super) fn reason(&self) -> &str {
        &self.reason
    }

    /// Returns transfer ownership unless a failed owner consumed it.
    pub(super) fn into_transfer(self) -> Option<OwnedRunnableTransfer> {
        self.transfer
    }
}

/// Destination rejection retaining transfer ownership when rollback is possible.
pub(super) struct OwnedMigrationImportFailure {
    reason: String,
    transfer: Option<PureNativeActorTransfer>,
}

impl OwnedMigrationImportFailure {
    /// Returns the stable destination or channel rejection.
    pub(super) fn reason(&self) -> &str {
        &self.reason
    }

    /// Returns actor ownership unless a failed owner thread consumed it.
    pub(super) fn into_transfer(self) -> Option<PureNativeActorTransfer> {
        self.transfer
    }

    /// Records an owner-thread failure after transfer ownership became unknowable.
    fn lost(reason: String) -> Self {
        Self {
            reason,
            transfer: None,
        }
    }
}

/// Commands are the only mutable entry to one execution shard.
enum ShardCommand {
    Begin {
        route: VmFixedActorRoute,
        export: String,
        args: Vec<ReplValue>,
        reply: SyncSender<Result<OwnedInvocationStep, String>>,
    },
    Drain {
        route: VmFixedActorRoute,
    },
    DetachMigration {
        route: VmFixedActorRoute,
        owner: VmProcessId,
        reply: SyncSender<Result<PureNativeActorTransfer, String>>,
    },
    ImportMigration {
        route: VmFixedActorRoute,
        transfer: PureNativeActorTransfer,
        reply: SyncSender<Result<(), PureNativeActorImportFailure>>,
    },
    DetachRunnable {
        destination: VmSchedulerId,
        class: VmSchedulerClass,
        reply: SyncSender<Result<Option<OwnedRunnableTransfer>, String>>,
    },
    ImportRunnable {
        route: VmFixedActorRoute,
        transfer: OwnedRunnableTransfer,
        reply: SyncSender<Result<(), OwnedRunnableImportFailure>>,
    },
    RunnableSnapshot {
        reply: SyncSender<VmSchedulerWorkSnapshot>,
    },
    #[allow(dead_code)] // Entered by the hidden live-debugger owner API.
    DebuggerControl {
        command: VmDebuggerControlCommand,
        reply: SyncSender<Result<VmDebuggerControlSnapshot, String>>,
    },
    #[cfg(test)]
    CompletedCount {
        reply: SyncSender<u64>,
    },
    #[cfg(test)]
    PanicWhileOwning {
        route: VmFixedActorRoute,
    },
    #[cfg(test)]
    RejectRunnableImports {
        reject: bool,
        reply: SyncSender<()>,
    },
    #[cfg(test)]
    ProbeExecution {
        route: VmFixedActorRoute,
        export: String,
        args: Vec<ReplValue>,
        barrier: Arc<Barrier>,
        active: Arc<AtomicUsize>,
        maximum: Arc<AtomicUsize>,
        reply: SyncSender<Result<(ReplValue, String), String>>,
    },
    Shutdown {
        reply: SyncSender<Result<(), String>>,
    },
}

/// Complete remote events published before their scheduler wake command.
pub(super) enum AotSchedulerPublication {
    IoCompletion {
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wake: PureNativeIoWake,
        reply: SyncSender<Result<OwnedInvocationStep, String>>,
    },
    Timer {
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wait: PureNativeTimerWait,
        reply: SyncSender<Result<OwnedInvocationStep, String>>,
    },
    CapabilityCompletion {
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wait: PureNativeCapabilityWait,
        outcome: NativeBoundaryReplyTerm,
        reply: SyncSender<Result<OwnedInvocationStep, String>>,
    },
    CancellationSignal {
        owner: VmProcessId,
        reason: String,
        reply: Option<SyncSender<Result<(), String>>>,
    },
}

impl AotSchedulerPublication {
    /// Classifies one complete publication before its scheduler is woken.
    fn published_kind(&self) -> VmFixedSchedulerEventKind {
        match self {
            Self::IoCompletion { .. } => VmFixedSchedulerEventKind::IoCompletionPublished,
            Self::Timer { .. } => VmFixedSchedulerEventKind::TimerPublished,
            Self::CapabilityCompletion { .. } => {
                VmFixedSchedulerEventKind::CapabilityCompletionPublished
            }
            Self::CancellationSignal { .. } => VmFixedSchedulerEventKind::SignalPublished,
        }
    }

    /// Classifies one publication as it enters generated actor execution.
    fn dispatched_kind(&self) -> VmFixedSchedulerEventKind {
        match self {
            Self::IoCompletion { .. } => VmFixedSchedulerEventKind::IoCompletionDispatched,
            Self::Timer { .. } => VmFixedSchedulerEventKind::TimerDispatched,
            Self::CapabilityCompletion { .. } => {
                VmFixedSchedulerEventKind::CapabilityCompletionDispatched
            }
            Self::CancellationSignal { .. } => VmFixedSchedulerEventKind::SignalDispatched,
        }
    }
}

/// One immutable address for a mutable shard owned by exactly one thread.
pub(super) struct AotHandlerShardOwner {
    scheduler: VmSchedulerId,
    shard_identity: VmExecutionShardId,
    shard_epoch: VmShardEpoch,
    control: Arc<VmFixedSchedulerControl<AotSchedulerPublication>>,
    telemetry: Arc<VmFixedSchedulerTelemetry>,
    failure: Arc<Mutex<Option<String>>>,
    panic_evidence: Arc<Mutex<Option<AotSchedulerPanicEvidence>>>,
    inbox: SyncSender<ShardCommand>,
    join: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for AotHandlerShardOwner {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let metrics = self.telemetry.snapshot();
        let retained_trace_events = self
            .telemetry
            .trace()
            .map(|events| events.len())
            .unwrap_or_default();
        formatter
            .debug_struct("AotHandlerShardOwner")
            .field("scheduler", &self.scheduler)
            .field("metrics", &metrics)
            .field("retained_trace_events", &retained_trace_events)
            .finish_non_exhaustive()
    }
}

impl AotHandlerShardOwner {
    /// Moves all mutable shard state to a dedicated owner thread.
    pub(super) fn spawn(
        scheduler: VmSchedulerId,
        shard: PureNativeExecutionShard,
        control: Arc<VmFixedSchedulerControl<AotSchedulerPublication>>,
        failure: Arc<Mutex<Option<String>>>,
    ) -> Result<Self, String> {
        let shard_identity = shard.shard_id().clone();
        let shard_epoch = shard.generation()?;
        let (inbox, commands) = mpsc::sync_channel(SHARD_INBOX_CAPACITY);
        let telemetry = Arc::new(VmFixedSchedulerTelemetry::for_shard(
            scheduler,
            shard_epoch,
            VM_FIXED_SCHEDULER_TRACE_CAPACITY,
        )?);
        let panic_evidence = Arc::new(Mutex::new(None));
        let join = thread::Builder::new()
            .name(format!("terlan-aot-scheduler-{}", scheduler.index()))
            .stack_size(AOT_SCHEDULER_STACK_BYTES)
            .spawn({
                let control = Arc::clone(&control);
                let failure = Arc::clone(&failure);
                let telemetry = Arc::clone(&telemetry);
                let panic_evidence = Arc::clone(&panic_evidence);
                move || {
                    scheduler_thread(
                        shard,
                        commands,
                        control,
                        telemetry,
                        failure,
                        panic_evidence,
                        scheduler,
                    )
                }
            })
            .map_err(|error| format!("error[serve.aot.shard_owner]: {error}"))?;
        Ok(Self {
            scheduler,
            shard_identity,
            shard_epoch,
            control,
            telemetry,
            failure,
            panic_evidence,
            inbox,
            join: Mutex::new(Some(join)),
        })
    }

    pub(super) fn begin<F>(
        &self,
        route: VmFixedActorRoute,
        export: String,
        args: Vec<ReplValue>,
        mut coordinate: F,
    ) -> Result<OwnedInvocationStep, String>
    where
        F: FnMut(),
    {
        self.control.register(route)?;
        let (reply, response) = mpsc::sync_channel(1);
        if let Err(error) = self.send(ShardCommand::Begin {
            route,
            export,
            args,
            reply,
        }) {
            let cleanup = self.control.discard(route);
            return match cleanup {
                Ok(()) => Err(error),
                Err(cleanup) => Err(format!("{error}; {cleanup}")),
            };
        }
        loop {
            match response.recv_timeout(Duration::from_millis(1)) {
                Ok(result) => return result,
                Err(RecvTimeoutError::Timeout) => coordinate(),
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(
                        "error[serve.aot.shard_owner]: owner stopped before begin reply"
                            .to_string(),
                    )
                }
            }
        }
    }

    pub(super) fn resume(
        &self,
        route: VmFixedActorRoute,
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wake: PureNativeIoWake,
    ) -> Result<OwnedInvocationStep, String> {
        let (reply, response) = mpsc::sync_channel(1);
        self.publish_wake(
            route,
            AotSchedulerPublication::IoCompletion {
                owner,
                suspension,
                wake,
                reply,
            },
        )?;
        receive(response, "resume")?
    }

    /// Publishes one external capability result before its actor owner resumes code.
    #[allow(dead_code)] // Retained as a deterministic manual completion test seam.
    pub(super) fn resume_capability(
        &self,
        route: VmFixedActorRoute,
        owner: VmProcessId,
        suspension: PureNativeSuspension,
        wait: PureNativeCapabilityWait,
        outcome: NativeBoundaryReplyTerm,
    ) -> Result<OwnedInvocationStep, String> {
        let (reply, response) = mpsc::sync_channel(1);
        self.publish_wake(
            route,
            AotSchedulerPublication::CapabilityCompletion {
                owner,
                suspension,
                wait,
                outcome,
                reply,
            },
        )?;
        receive(response, "capability resume")?
    }

    pub(super) fn cancel(
        &self,
        route: VmFixedActorRoute,
        owner: VmProcessId,
        reason: String,
    ) -> Result<(), String> {
        let (reply, response) = mpsc::sync_channel(1);
        self.publish_wake(
            route,
            AotSchedulerPublication::CancellationSignal {
                owner,
                reason,
                reply: Some(reply),
            },
        )?;
        receive(response, "cancel")?
    }

    /// Drop-time cancellation is asynchronous and cannot block the shard owner.
    pub(super) fn cancel_detached(
        &self,
        route: VmFixedActorRoute,
        owner: VmProcessId,
        reason: String,
    ) {
        let publication = AotSchedulerPublication::CancellationSignal {
            owner,
            reason,
            reply: None,
        };
        let kind = publication.published_kind();
        let Ok((identity, _wake)) = self.control.publish_identified(route, publication) else {
            return;
        };
        if self
            .telemetry
            .record_publication(kind, route, identity)
            .is_err()
        {
            return;
        }
        if self.inbox.try_send(ShardCommand::Drain { route }).is_err() {
            // A full bounded inbox is already applying backpressure. A short
            // helper preserves actor cleanup without blocking the dropping task.
            let inbox = self.inbox.clone();
            let _ = thread::Builder::new()
                .name("terlan-aot-cancel".to_string())
                .spawn(move || {
                    let _ = inbox.send(ShardCommand::Drain { route });
                });
        }
    }

    /// Detaches one parked generated actor on its current scheduler owner.
    pub(super) fn detach_migration(
        &self,
        route: VmFixedActorRoute,
        owner: VmProcessId,
    ) -> Result<PureNativeActorTransfer, String> {
        let (reply, response) = mpsc::sync_channel(1);
        self.send(ShardCommand::DetachMigration {
            route,
            owner,
            reply,
        })?;
        receive(response, "detach migration")?
    }

    /// Imports one actor on this owner or returns its complete transfer.
    pub(super) fn import_migration(
        &self,
        route: VmFixedActorRoute,
        transfer: PureNativeActorTransfer,
    ) -> Result<(), OwnedMigrationImportFailure> {
        let (reply, response) = mpsc::sync_channel(1);
        let command = ShardCommand::ImportMigration {
            route,
            transfer,
            reply,
        };
        if let Err(error) = self.inbox.send(command) {
            let ShardCommand::ImportMigration { transfer, .. } = error.0 else {
                unreachable!("failed migration send returns its migration command")
            };
            return Err(OwnedMigrationImportFailure {
                reason: self.fail_shard(format!(
                    "scheduler {} owner thread stopped before migration import",
                    self.scheduler.index()
                )),
                transfer: Some(transfer),
            });
        }
        match receive(response, "import migration") {
            Ok(Ok(())) => Ok(()),
            Ok(Err(failure)) => Err(OwnedMigrationImportFailure {
                reason: failure.reason().to_string(),
                transfer: Some(failure.into_transfer()),
            }),
            Err(reason) => Err(OwnedMigrationImportFailure::lost(reason)),
        }
    }

    /// Detaches at most one queued generated continuation for one destination.
    pub(super) fn detach_runnable_to(
        &self,
        destination: VmSchedulerId,
        class: VmSchedulerClass,
    ) -> Result<Option<OwnedRunnableTransfer>, String> {
        let (reply, response) = mpsc::sync_channel(1);
        self.send(ShardCommand::DetachRunnable {
            destination,
            class,
            reply,
        })?;
        receive(response, "detach runnable")?
    }

    /// Imports one complete runnable continuation on its published route.
    pub(super) fn import_runnable(
        &self,
        route: VmFixedActorRoute,
        transfer: OwnedRunnableTransfer,
    ) -> Result<(), OwnedRunnableImportFailure> {
        let (reply, response) = mpsc::sync_channel(1);
        let command = ShardCommand::ImportRunnable {
            route,
            transfer,
            reply,
        };
        if let Err(error) = self.inbox.send(command) {
            let ShardCommand::ImportRunnable { transfer, .. } = error.0 else {
                unreachable!("failed runnable send returns its import command")
            };
            return Err(OwnedRunnableImportFailure {
                reason: self.fail_shard(format!(
                    "scheduler {} stopped before runnable import",
                    self.scheduler.index()
                )),
                transfer: Some(transfer),
            });
        }
        match receive(response, "import runnable") {
            Ok(result) => result,
            Err(reason) => Err(OwnedRunnableImportFailure {
                reason,
                transfer: None,
            }),
        }
    }

    /// Returns one immutable snapshot of the live generated runnable queue.
    pub(super) fn runnable_snapshot(&self) -> Result<VmSchedulerWorkSnapshot, String> {
        let (reply, response) = mpsc::sync_channel(1);
        self.send(ShardCommand::RunnableSnapshot { reply })?;
        receive(response, "runnable snapshot")
    }

    /// Applies one live debugger command on the thread owning runnable mutation.
    #[allow(dead_code)] // Activated by the hidden live-debugger command surface.
    pub(super) fn debugger_control(
        &self,
        command: VmDebuggerControlCommand,
    ) -> Result<VmDebuggerControlSnapshot, String> {
        let (reply, response) = mpsc::sync_channel(1);
        self.send(ShardCommand::DebuggerControl { command, reply })?;
        receive(response, "debugger control")?
    }

    /// Returns the exact destination identity used to rebind typed I/O waits.
    pub(super) fn shard_identity(&self) -> &VmExecutionShardId {
        &self.shard_identity
    }

    /// Returns the exact destination generation used to rebind typed I/O waits.
    pub(super) const fn shard_epoch(&self) -> VmShardEpoch {
        self.shard_epoch
    }

    /// Returns the fixed scheduler exclusively owning this command channel.
    pub(super) const fn scheduler(&self) -> VmSchedulerId {
        self.scheduler
    }

    #[cfg(test)]
    pub(super) fn completed_call_count(&self) -> Result<u64, String> {
        let (reply, response) = mpsc::sync_channel(1);
        self.send(ShardCommand::CompletedCount { reply })?;
        receive(response, "completed count")
    }

    /// Injects one panic after acquiring a real actor mutator lease.
    #[cfg(test)]
    pub(super) fn panic_scheduler_while_owning(
        &self,
        route: VmFixedActorRoute,
    ) -> Result<(), String> {
        self.control.register(route)?;
        if let Err(error) = self.send(ShardCommand::PanicWhileOwning { route }) {
            let _ = self.control.discard(route);
            return Err(error);
        }
        Ok(())
    }

    /// Fills the bounded replay buffer before deterministic panic injection.
    #[cfg(test)]
    pub(super) fn fill_panic_replay_pressure(&self, events: usize) -> Result<(), String> {
        for _ in 0..events {
            self.telemetry
                .record(VmFixedSchedulerEventKind::Command, None)?;
        }
        Ok(())
    }

    /// Returns retained scheduler and supervisor evidence after a panic.
    pub(super) fn panic_evidence(&self) -> Result<Option<AotSchedulerPanicEvidence>, String> {
        self.panic_evidence
            .lock()
            .map(|evidence| evidence.clone())
            .map_err(|_| "error[vm.scheduler_panic]: evidence lock poisoned".to_string())
    }

    /// Pauses local runnable service without blocking owner commands.
    #[cfg(test)]
    pub(super) fn pause_runnable(&self, paused: bool) -> Result<(), String> {
        let command = if paused {
            VmDebuggerControlCommand::Pause
        } else {
            VmDebuggerControlCommand::Continue
        };
        self.debugger_control(command).map(|_| ())
    }

    /// Injects deterministic destination rejection for rollback verification.
    #[cfg(test)]
    pub(super) fn reject_runnable_imports(&self, reject: bool) -> Result<(), String> {
        let (reply, response) = mpsc::sync_channel(1);
        self.send(ShardCommand::RejectRunnableImports { reject, reply })?;
        receive(response, "reject runnable imports")
    }

    /// Returns immutable scheduler counters for integration verification.
    #[cfg(test)]
    pub(super) fn telemetry_snapshot(
        &self,
    ) -> crate::runtime::vm::fixed_scheduler_telemetry::VmFixedSchedulerMetricsSnapshot {
        self.telemetry.snapshot()
    }

    /// Returns the bounded scheduler-local event trace.
    #[cfg(test)]
    pub(super) fn telemetry_trace(
        &self,
    ) -> Result<
        Vec<crate::runtime::vm::fixed_scheduler_telemetry::VmFixedSchedulerTraceEvent>,
        String,
    > {
        self.telemetry.trace()
    }

    /// Returns canonical generation-qualified scheduler replay evidence.
    pub(super) fn multicore_replay_capture(
        &self,
    ) -> Result<crate::runtime::vm::multicore_replay::VmMulticoreReplayCapture, String> {
        self.telemetry.replay_capture()
    }

    /// Returns canonical replay evidence under its historical test name.
    #[cfg(test)]
    pub(super) fn telemetry_replay_capture(
        &self,
    ) -> Result<crate::runtime::vm::multicore_replay::VmMulticoreReplayCapture, String> {
        self.multicore_replay_capture()
    }

    /// Executes one generated export inside a synchronized scheduler envelope.
    #[cfg(test)]
    pub(super) fn probe_execution(
        &self,
        route: VmFixedActorRoute,
        export: String,
        barrier: Arc<Barrier>,
        active: Arc<AtomicUsize>,
        maximum: Arc<AtomicUsize>,
    ) -> Result<(ReplValue, String), String> {
        self.probe_execution_with_args(route, export, Vec::new(), barrier, active, maximum)
    }

    /// Executes one generated export with typed arguments inside a synchronized
    /// scheduler envelope.
    #[cfg(test)]
    pub(super) fn probe_execution_with_args(
        &self,
        route: VmFixedActorRoute,
        export: String,
        args: Vec<ReplValue>,
        barrier: Arc<Barrier>,
        active: Arc<AtomicUsize>,
        maximum: Arc<AtomicUsize>,
    ) -> Result<(ReplValue, String), String> {
        self.control.register(route)?;
        let (reply, response) = mpsc::sync_channel(1);
        self.send(ShardCommand::ProbeExecution {
            route,
            export,
            args,
            barrier,
            active,
            maximum,
            reply,
        })?;
        receive(response, "probe execution")?
    }

    pub(super) fn shutdown(&self) -> Result<(), String> {
        let (reply, response) = mpsc::sync_channel(1);
        let sent = self.inbox.send(ShardCommand::Shutdown { reply });
        let result = if sent.is_ok() {
            match receive(response, "shutdown") {
                Ok(result) => result,
                Err(error) => Err(error),
            }
        } else {
            Err("error[serve.aot.shard_owner]: owner thread stopped".to_string())
        };
        let join = self
            .join
            .lock()
            .map_err(|_| "error[serve.aot.shard_owner]: join lock poisoned".to_string())?
            .take();
        if let Some(join) = join {
            join.join()
                .map_err(|_| "error[serve.aot.shard_owner]: owner thread panicked".to_string())?;
        }
        if let Some(failure) = self
            .failure
            .lock()
            .map_err(|_| "error[vm.scheduler_shard_failed]: failure lock poisoned".to_string())?
            .as_ref()
        {
            return Err(format!("error[vm.scheduler_shard_failed]: {failure}"));
        }
        result
    }

    fn send(&self, command: ShardCommand) -> Result<(), String> {
        if let Some(failure) = self
            .failure
            .lock()
            .map_err(|_| "error[vm.scheduler_shard_failed]: failure lock poisoned".to_string())?
            .as_ref()
        {
            return Err(format!("error[vm.scheduler_shard_failed]: {failure}"));
        }
        if self.inbox.send(command).is_err() {
            return Err(self.fail_shard(format!(
                "scheduler {} owner thread stopped",
                self.scheduler.index()
            )));
        }
        Ok(())
    }

    /// Publishes a complete event before waking its fixed home scheduler.
    fn publish_wake(
        &self,
        route: VmFixedActorRoute,
        publication: AotSchedulerPublication,
    ) -> Result<(), String> {
        let kind = publication.published_kind();
        let (identity, wake) = self.control.publish_identified(route, publication)?;
        self.telemetry
            .record_publication(kind, route, identity)
            .map_err(|error| self.fail_shard(error))?;
        if wake != VmMailboxWake::Enqueue {
            return Err(self.fail_shard(format!(
                "actor {} wake observed impossible non-parked lifecycle",
                route.actor_id()
            )));
        }
        self.send(ShardCommand::Drain { route })
    }

    /// Latches one impossible owner condition across the whole shard.
    fn fail_shard(&self, reason: String) -> String {
        if let Ok(mut failure) = self.failure.lock() {
            if failure.is_none() {
                *failure = Some(reason.clone());
            }
        }
        format!("error[vm.scheduler_shard_failed]: {reason}")
    }
}

/// Converts any scheduler panic into a supervised fail-stop shard failure.
fn scheduler_thread(
    mut shard: PureNativeExecutionShard,
    commands: Receiver<ShardCommand>,
    control: Arc<VmFixedSchedulerControl<AotSchedulerPublication>>,
    telemetry: Arc<VmFixedSchedulerTelemetry>,
    failure: Arc<Mutex<Option<String>>>,
    panic_evidence: Arc<Mutex<Option<AotSchedulerPanicEvidence>>>,
    scheduler: VmSchedulerId,
) {
    let outcome = panic::catch_unwind(AssertUnwindSafe(|| {
        owner_loop(&mut shard, commands, control, telemetry.as_ref(), scheduler)
    }));
    if let Err(payload) = outcome {
        let detail = panic_detail(payload);
        let reason = format!("scheduler {} panicked: {detail}", scheduler.index());
        let _ = telemetry.record_scheduler_panic();
        let _ = shard.report_crash(reason.clone(), 0);
        if let (Ok(scheduler_replay), Ok(shard_lifecycle)) =
            (telemetry.replay_capture(), shard.lifecycle_replay_capture())
        {
            if let Ok(mut evidence) = panic_evidence.lock() {
                *evidence = Some(AotSchedulerPanicEvidence::new(
                    scheduler,
                    reason.clone(),
                    scheduler_replay,
                    shard_lifecycle,
                ));
            }
        }
        if let Ok(mut current) = failure.lock() {
            if current.is_none() {
                *current = Some(reason);
            }
        }
    }
}

/// Renders a stable panic reason without host-specific backtrace text.
fn panic_detail(payload: Box<dyn std::any::Any + Send>) -> String {
    let detail = if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_string()
    };
    truncate_utf8(&detail, MAX_SCHEDULER_PANIC_DETAIL_BYTES)
}

/// Truncates diagnostic text at a valid UTF-8 boundary.
fn truncate_utf8(value: &str, maximum_bytes: usize) -> String {
    if value.len() <= maximum_bytes {
        return value.to_string();
    }
    let mut end = maximum_bytes;
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_string()
}

fn receive<T>(response: Receiver<T>, operation: &str) -> Result<T, String> {
    response.recv().map_err(|_| {
        format!("error[serve.aot.shard_owner]: owner stopped before {operation} reply")
    })
}

#[cfg(test)]
#[path = "shard_owner_test.rs"]
mod shard_owner_test;
