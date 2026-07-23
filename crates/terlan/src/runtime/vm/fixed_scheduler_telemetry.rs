//! Per-scheduler metrics and bounded trace storage for fixed placement.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use super::actor_directory::VmActorPublication;
use super::execution_shard_protocol::VmShardEpoch;
use super::fixed_scheduler_control::{VmFixedActorLease, VmFixedActorMigrationTicket};
use super::multicore_replay::VmMulticoreReplayCapture;
use super::multicore_replay::{VmMulticoreEventContext, VmMulticoreReplayRecorder};
use super::scheduler_topology::{VmFixedActorRoute, VmSchedulerId};

pub(crate) use super::multicore_replay::VmMulticoreEventKind as VmFixedSchedulerEventKind;

/// Default number of recent scheduler events retained per owner thread.
pub(crate) const VM_FIXED_SCHEDULER_TRACE_CAPACITY: usize = 1_024;

/// One scheduler-local event retained for diagnostics and replay input.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmFixedSchedulerTraceEvent {
    /// Monotonic sequence within one scheduler.
    pub(crate) sequence: u64,
    /// Scheduler that observed this event.
    pub(crate) scheduler: VmSchedulerId,
    /// Actor route involved when the event is actor-specific.
    pub(crate) actor_id: Option<u64>,
    /// Stable scheduler event classification.
    pub(crate) kind: VmFixedSchedulerEventKind,
}

/// Immutable scheduler-local accounting snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmFixedSchedulerMetricsSnapshot {
    /// Number of recorded scheduling boundary events.
    pub(crate) events: u64,
    /// Number of generation-checked external I/O completion publications.
    pub(crate) io_completions: u64,
    /// Number of generation-checked capability completion publications.
    pub(crate) capability_completions: u64,
    /// Number of remote system-signal publications.
    pub(crate) signals: u64,
    /// Number of scheduler-owned timer publications.
    pub(crate) timers: u64,
    /// Number of generated AOT entries.
    pub(crate) entries: u64,
    /// Number of terminal generated completions.
    pub(crate) completions: u64,
    /// Number of fail-stop scheduler failures.
    pub(crate) failures: u64,
    /// Number of old events evicted from the bounded trace.
    pub(crate) trace_evictions: u64,
}

/// Counters and trace buffer owned by one fixed scheduler.
#[derive(Debug)]
pub(crate) struct VmFixedSchedulerTelemetry {
    shard_epoch: Option<u64>,
    next_execution_interval: AtomicU64,
    events: AtomicU64,
    io_completions: AtomicU64,
    capability_completions: AtomicU64,
    signals: AtomicU64,
    timers: AtomicU64,
    entries: AtomicU64,
    completions: AtomicU64,
    failures: AtomicU64,
    trace_evictions: AtomicU64,
    /// Actor execution interval active on this scheduler owner, when any.
    active_execution: Mutex<Option<VmMulticoreEventContext>>,
    replay: Mutex<VmMulticoreReplayRecorder>,
}

impl VmFixedSchedulerTelemetry {
    /// Creates one scheduler-local telemetry owner with bounded storage.
    #[cfg(test)]
    pub(crate) fn new(scheduler: VmSchedulerId, trace_capacity: usize) -> Result<Self, String> {
        Self::create(scheduler, None, trace_capacity)
    }

    /// Creates scheduler telemetry qualified by one native-image shard epoch.
    pub(crate) fn for_shard(
        scheduler: VmSchedulerId,
        shard_epoch: VmShardEpoch,
        trace_capacity: usize,
    ) -> Result<Self, String> {
        Self::create(scheduler, Some(shard_epoch.as_u64()), trace_capacity)
    }

    /// Creates one telemetry owner after validating its bounded capacity.
    fn create(
        scheduler: VmSchedulerId,
        shard_epoch: Option<u64>,
        trace_capacity: usize,
    ) -> Result<Self, String> {
        if trace_capacity == 0 {
            return Err(
                "error[vm.scheduler_telemetry]: trace capacity must be positive".to_string(),
            );
        }
        Ok(Self {
            shard_epoch,
            next_execution_interval: AtomicU64::new(1),
            events: AtomicU64::new(0),
            io_completions: AtomicU64::new(0),
            capability_completions: AtomicU64::new(0),
            signals: AtomicU64::new(0),
            timers: AtomicU64::new(0),
            entries: AtomicU64::new(0),
            completions: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            trace_evictions: AtomicU64::new(0),
            active_execution: Mutex::new(None),
            replay: Mutex::new(
                VmMulticoreReplayRecorder::recording(scheduler, trace_capacity)
                    .map_err(|error| format!("error[vm.scheduler_telemetry]: {error}"))?,
            ),
        })
    }

    /// Records one event without consulting another scheduler or shard lock.
    pub(crate) fn record(
        &self,
        kind: VmFixedSchedulerEventKind,
        route: Option<VmFixedActorRoute>,
    ) -> Result<(), String> {
        let context = match route {
            Some(route) => self
                .base_context()?
                .with_actor(route.actor_id().get())
                .map_err(|error| format!("error[vm.scheduler_telemetry]: {error}"))?,
            None => self.base_context()?,
        };
        self.record_with_context(kind, context)
    }

    /// Records publication using the actor generation and mailbox sequence.
    pub(crate) fn record_publication(
        &self,
        kind: VmFixedSchedulerEventKind,
        route: VmFixedActorRoute,
        publication: VmActorPublication,
    ) -> Result<(), String> {
        self.validate_publication(route, publication)?;
        let context = self
            .base_context()?
            .with_actor(route.actor_id().get())
            .and_then(|context| {
                context.with_actor_generation(publication.handle.actor_generation())
            })
            .and_then(|context| context.with_operation_sequence(publication.sequence))
            .map_err(|error| format!("error[vm.scheduler_telemetry]: {error}"))?;
        self.record_with_context(kind, context)
    }

    /// Records owner dispatch with publication and mutator generations joined.
    pub(crate) fn record_dispatch(
        &self,
        kind: VmFixedSchedulerEventKind,
        lease: &VmFixedActorLease,
        publication: VmActorPublication,
    ) -> Result<(), String> {
        self.validate_publication(lease.route(), publication)?;
        let context = self
            .base_context()?
            .with_actor(lease.route().actor_id().get())
            .and_then(|context| {
                context.with_generations(lease.actor_generation(), lease.owner_generation())
            })
            .and_then(|context| context.with_operation_sequence(publication.sequence))
            .map_err(|error| format!("error[vm.scheduler_telemetry]: {error}"))?;
        self.record_with_context(kind, context)
    }

    /// Builds canonical actor and owner identity from an acquired mutator lease.
    pub(crate) fn context_for_lease(
        &self,
        lease: &VmFixedActorLease,
    ) -> Result<VmMulticoreEventContext, String> {
        self.base_context()?
            .with_actor(lease.route().actor_id().get())
            .and_then(|context| {
                context.with_generations(lease.actor_generation(), lease.owner_generation())
            })
            .map_err(|error| format!("error[vm.scheduler_telemetry]: {error}"))
    }

    /// Records an actor boundary using the generation held by its current lease.
    pub(crate) fn record_owned(
        &self,
        kind: VmFixedSchedulerEventKind,
        lease: &VmFixedActorLease,
    ) -> Result<(), String> {
        self.record_with_context(kind, self.context_for_lease(lease)?)
    }

    /// Starts one scheduler-local actor execution interval.
    pub(crate) fn begin_execution(
        &self,
        lease: &VmFixedActorLease,
    ) -> Result<VmMulticoreEventContext, String> {
        if self
            .active_execution
            .lock()
            .map_err(|_| {
                "error[vm.scheduler_telemetry]: active execution lock poisoned".to_string()
            })?
            .is_some()
        {
            return Err(
                "error[vm.scheduler_telemetry]: scheduler already owns an execution interval"
                    .to_string(),
            );
        }
        let interval = self
            .next_execution_interval
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .map_err(|_| {
                "error[vm.scheduler_telemetry]: execution interval identity exhausted".to_string()
            })?;
        let context = self
            .context_for_lease(lease)?
            .with_execution_interval(interval)
            .map_err(|error| format!("error[vm.scheduler_telemetry]: {error}"))?;
        self.record_with_context(VmFixedSchedulerEventKind::SchedulerSelected, context)?;
        self.record_with_context(VmFixedSchedulerEventKind::ExecutionStarted, context)?;
        *self.active_execution.lock().map_err(|_| {
            "error[vm.scheduler_telemetry]: active execution lock poisoned".to_string()
        })? = Some(context);
        Ok(context)
    }

    /// Finishes the exact scheduler-local execution interval previously started.
    pub(crate) fn finish_execution(&self, context: VmMulticoreEventContext) -> Result<(), String> {
        if context.execution_interval.is_none() {
            return Err(
                "error[vm.scheduler_telemetry]: finished execution has no interval identity"
                    .to_string(),
            );
        }
        let active = *self.active_execution.lock().map_err(|_| {
            "error[vm.scheduler_telemetry]: active execution lock poisoned".to_string()
        })?;
        if active != Some(context) {
            return Err(
                "error[vm.scheduler_telemetry]: finished execution does not match the active interval"
                    .to_string(),
            );
        }
        self.record_with_context(VmFixedSchedulerEventKind::ExecutionFinished, context)?;
        self.active_execution
            .lock()
            .map_err(|_| {
                "error[vm.scheduler_telemetry]: active execution lock poisoned".to_string()
            })?
            .take();
        Ok(())
    }

    /// Records fail-stop panic evidence under the active actor lease when present.
    pub(crate) fn record_scheduler_panic(&self) -> Result<VmMulticoreEventContext, String> {
        let context = self
            .active_execution
            .lock()
            .map_err(|_| {
                "error[vm.scheduler_telemetry]: active execution lock poisoned".to_string()
            })?
            .take()
            .map(Ok)
            .unwrap_or_else(|| self.base_context())?;
        self.record_with_context(VmFixedSchedulerEventKind::SchedulerPanicked, context)?;
        Ok(context)
    }

    /// Builds generation-qualified source context for one migration ticket.
    pub(crate) fn context_for_migration(
        &self,
        ticket: &VmFixedActorMigrationTicket,
    ) -> Result<VmMulticoreEventContext, String> {
        self.base_context()?
            .with_actor(ticket.source().actor_id().get())
            .and_then(|context| {
                context.with_generations(ticket.actor_generation(), ticket.owner_generation())
            })
            .map(|context| context.with_peer_scheduler(ticket.destination().scheduler()))
            .map_err(|error| format!("error[vm.scheduler_telemetry]: {error}"))
    }

    /// Records one generation-qualified boundary in metrics and canonical replay.
    pub(crate) fn record_with_context(
        &self,
        kind: VmFixedSchedulerEventKind,
        context: VmMulticoreEventContext,
    ) -> Result<(), String> {
        let outcome = self
            .replay
            .lock()
            .map_err(|_| "error[vm.scheduler_telemetry]: replay lock poisoned".to_string())?
            .observe(kind, context)
            .map_err(|error| format!("error[vm.scheduler_telemetry]: {error}"))?;
        self.count(kind);
        if outcome.evicted {
            self.trace_evictions.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }

    /// Updates scheduler-local counters for one accepted event kind.
    fn count(&self, kind: VmFixedSchedulerEventKind) {
        self.events.fetch_add(1, Ordering::Relaxed);
        match kind {
            VmFixedSchedulerEventKind::IoCompletionPublished => {
                self.io_completions.fetch_add(1, Ordering::Relaxed);
            }
            VmFixedSchedulerEventKind::CapabilityCompletionPublished => {
                self.capability_completions.fetch_add(1, Ordering::Relaxed);
            }
            VmFixedSchedulerEventKind::SignalPublished => {
                self.signals.fetch_add(1, Ordering::Relaxed);
            }
            VmFixedSchedulerEventKind::TimerPublished => {
                self.timers.fetch_add(1, Ordering::Relaxed);
            }
            VmFixedSchedulerEventKind::Entry => {
                self.entries.fetch_add(1, Ordering::Relaxed);
            }
            VmFixedSchedulerEventKind::Completed => {
                self.completions.fetch_add(1, Ordering::Relaxed);
            }
            VmFixedSchedulerEventKind::SchedulerPanicked | VmFixedSchedulerEventKind::Failed => {
                self.failures.fetch_add(1, Ordering::Relaxed);
            }
            _ => {}
        }
    }

    /// Creates scheduler-wide context qualified by the optional shard epoch.
    fn base_context(&self) -> Result<VmMulticoreEventContext, String> {
        match self.shard_epoch {
            Some(epoch) => VmMulticoreEventContext::scheduler()
                .with_shard_epoch(epoch)
                .map_err(|error| format!("error[vm.scheduler_telemetry]: {error}")),
            None => Ok(VmMulticoreEventContext::scheduler()),
        }
    }

    /// Rejects publication evidence that names a different actor identity.
    fn validate_publication(
        &self,
        route: VmFixedActorRoute,
        publication: VmActorPublication,
    ) -> Result<(), String> {
        if publication.handle.pid().as_u64() != route.actor_id().get() {
            return Err(format!(
                "error[vm.scheduler_telemetry]: publication actor {} does not match route {}",
                publication.handle.pid().as_u64(),
                route.actor_id()
            ));
        }
        Ok(())
    }

    /// Returns immutable counters for focused tests and future diagnostics.
    pub(crate) fn snapshot(&self) -> VmFixedSchedulerMetricsSnapshot {
        VmFixedSchedulerMetricsSnapshot {
            events: self.events.load(Ordering::Relaxed),
            io_completions: self.io_completions.load(Ordering::Relaxed),
            capability_completions: self.capability_completions.load(Ordering::Relaxed),
            signals: self.signals.load(Ordering::Relaxed),
            timers: self.timers.load(Ordering::Relaxed),
            entries: self.entries.load(Ordering::Relaxed),
            completions: self.completions.load(Ordering::Relaxed),
            failures: self.failures.load(Ordering::Relaxed),
            trace_evictions: self.trace_evictions.load(Ordering::Relaxed),
        }
    }

    /// Returns the retained scheduler-local trace in sequence order.
    pub(crate) fn trace(&self) -> Result<Vec<VmFixedSchedulerTraceEvent>, String> {
        let capture = self
            .replay
            .lock()
            .map_err(|_| "error[vm.scheduler_telemetry]: replay lock poisoned".to_string())?
            .capture()
            .map_err(|error| format!("error[vm.scheduler_telemetry]: {error}"))?;
        Ok(capture
            .events
            .into_iter()
            .map(|event| VmFixedSchedulerTraceEvent {
                sequence: event.sequence,
                scheduler: event.scheduler,
                actor_id: event.context.actor_id,
                kind: event.kind,
            })
            .collect())
    }

    /// Returns the canonical scheduler-local replay capture.
    pub(crate) fn replay_capture(&self) -> Result<VmMulticoreReplayCapture, String> {
        self.replay
            .lock()
            .map_err(|_| "error[vm.scheduler_telemetry]: replay lock poisoned".to_string())?
            .capture()
            .map_err(|error| format!("error[vm.scheduler_telemetry]: {error}"))
    }
}

#[cfg(test)]
#[path = "fixed_scheduler_telemetry_test.rs"]
mod fixed_scheduler_telemetry_test;
