//! Deterministic per-scheduler event capture and controlled replay.

#![allow(dead_code)] // MC-8 activates replay event consumers in ordered slices.

use std::collections::VecDeque;
use std::fmt;

use serde::Serialize;

use super::scheduler_topology::{VmSchedulerId, VM_MAX_SCHEDULERS};

/// Versioned schema identity for scheduler replay captures.
pub(crate) const VM_MULTICORE_REPLAY_SCHEMA: &str = "terlan.vm.multicore-replay.v1";

/// Stable kinds emitted at multicore scheduling and ownership boundaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum VmMulticoreEventKind {
    /// A command entered the bounded scheduler inbox.
    Command,
    /// Generated AOT code entered for a new actor.
    Entry,
    /// A scheduler was selected for actor work.
    SchedulerSelected,
    /// An actor changed queue or ownership lifecycle.
    QueueTransition,
    /// A complete actor message was published.
    MessagePublished,
    /// A protocol reactor or capability worker published typed completion data.
    IoCompletionPublished,
    /// A typed I/O completion resumed generated code on its actor owner.
    IoCompletionDispatched,
    /// A capability worker published a typed generated completion.
    CapabilityCompletionPublished,
    /// A capability completion resumed generated code on its actor owner.
    CapabilityCompletionDispatched,
    /// A scheduler-owned timer deadline was published for its parked actor.
    TimerPublished,
    /// A published timer resumed its exact generated continuation.
    TimerDispatched,
    /// A system or cancellation signal was published remotely.
    SignalPublished,
    /// A published system signal was applied by its owner.
    SignalDispatched,
    /// Generated code parked at a published safepoint.
    Parked,
    /// Generated code yielded and entered its scheduler runnable queue.
    Yielded,
    /// A queued generated continuation resumed for another local slice.
    Resumed,
    /// A queued generated continuation detached for another scheduler.
    Stolen,
    /// A transferred generated continuation entered this scheduler queue.
    Imported,
    /// An actor migration started under linear transfer authority.
    MigrationStarted,
    /// An actor migration published its destination owner.
    MigrationCompleted,
    /// A rejected migration restored the source owner.
    MigrationAborted,
    /// A work-stealing decision selected or rejected a victim.
    StealOutcome,
    /// A wake source made one parked actor runnable.
    Wake,
    /// A native image generation became visible to this scheduler.
    ImageGeneration,
    /// A debugger paused runnable service on one scheduler owner.
    DebuggerPaused,
    /// A debugger restored normal runnable service on one scheduler owner.
    DebuggerContinued,
    /// A debugger-authorized actor slice executed under its current owner.
    DebuggerStepped,
    /// Actor work entered one measured execution interval.
    ExecutionStarted,
    /// Actor work left one measured execution interval.
    ExecutionFinished,
    /// Pending actor work was cancelled.
    Cancelled,
    /// Generated code completed and released its actor route.
    Completed,
    /// A supervised execution shard failed under its admitted image generation.
    SupervisionFailed,
    /// A supervised execution shard scheduled a bounded restart attempt.
    SupervisionRestartScheduled,
    /// A supervised execution shard published a recovered image generation.
    SupervisionRestarted,
    /// An execution shard started orderly shutdown under its admitted generation.
    ShutdownStarted,
    /// A fixed scheduler panicked and terminated under fail-stop containment.
    SchedulerPanicked,
    /// Scheduler ownership failed closed.
    Failed,
    /// The scheduler completed orderly shutdown.
    Shutdown,
}

/// Generation-qualified context attached to one scheduler event.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmMulticoreEventContext {
    /// Actor identity when the event is actor-specific.
    pub(crate) actor_id: Option<u64>,
    /// Directory-slot generation for the actor identity.
    pub(crate) actor_generation: Option<u64>,
    /// Mutator-owner generation visible at this boundary.
    pub(crate) owner_generation: Option<u64>,
    /// Native execution-shard image epoch.
    pub(crate) shard_epoch: Option<u64>,
    /// Stable message, signal, timer, I/O, or native operation sequence.
    pub(crate) operation_sequence: Option<u64>,
    /// Peer scheduler selected by migration, steal, or wake routing.
    pub(crate) peer_scheduler: Option<VmSchedulerId>,
    /// Stable interval identity paired by execution start and finish events.
    pub(crate) execution_interval: Option<u64>,
}

impl VmMulticoreEventContext {
    /// Creates empty context for a scheduler-wide boundary.
    pub(crate) const fn scheduler() -> Self {
        Self {
            actor_id: None,
            actor_generation: None,
            owner_generation: None,
            shard_epoch: None,
            operation_sequence: None,
            peer_scheduler: None,
            execution_interval: None,
        }
    }

    /// Attaches one nonzero actor identity.
    pub(crate) fn with_actor(mut self, actor_id: u64) -> Result<Self, VmMulticoreReplayError> {
        if actor_id == 0 {
            return Err(VmMulticoreReplayError::ZeroActorIdentity);
        }
        self.actor_id = Some(actor_id);
        Ok(self)
    }

    /// Attaches actor and owner generations to an actor-specific event.
    pub(crate) fn with_generations(
        mut self,
        actor_generation: u64,
        owner_generation: u64,
    ) -> Result<Self, VmMulticoreReplayError> {
        self = self.with_actor_generation(actor_generation)?;
        self.with_owner_generation(owner_generation)
    }

    /// Attaches the actor generation accepting a publication or execution.
    pub(crate) fn with_actor_generation(
        mut self,
        actor_generation: u64,
    ) -> Result<Self, VmMulticoreReplayError> {
        if self.actor_id.is_none() {
            return Err(VmMulticoreReplayError::GenerationWithoutActor);
        }
        if actor_generation == 0 {
            return Err(VmMulticoreReplayError::ZeroActorGeneration);
        }
        self.actor_generation = Some(actor_generation);
        Ok(self)
    }

    /// Attaches the mutator-owner generation visible at an actor boundary.
    pub(crate) fn with_owner_generation(
        mut self,
        owner_generation: u64,
    ) -> Result<Self, VmMulticoreReplayError> {
        if self.actor_id.is_none() {
            return Err(VmMulticoreReplayError::GenerationWithoutActor);
        }
        self.owner_generation = Some(owner_generation);
        Ok(self)
    }

    /// Attaches one nonzero execution-shard epoch.
    pub(crate) fn with_shard_epoch(
        mut self,
        shard_epoch: u64,
    ) -> Result<Self, VmMulticoreReplayError> {
        if shard_epoch == 0 {
            return Err(VmMulticoreReplayError::ZeroShardEpoch);
        }
        self.shard_epoch = Some(shard_epoch);
        Ok(self)
    }

    /// Attaches one nonzero operation publication sequence.
    pub(crate) fn with_operation_sequence(
        mut self,
        sequence: u64,
    ) -> Result<Self, VmMulticoreReplayError> {
        if sequence == 0 {
            return Err(VmMulticoreReplayError::ZeroOperationSequence);
        }
        self.operation_sequence = Some(sequence);
        Ok(self)
    }

    /// Attaches the peer scheduler involved in a cross-owner decision.
    pub(crate) const fn with_peer_scheduler(mut self, scheduler: VmSchedulerId) -> Self {
        self.peer_scheduler = Some(scheduler);
        self
    }

    /// Attaches one nonzero execution interval identity.
    pub(crate) fn with_execution_interval(
        mut self,
        interval: u64,
    ) -> Result<Self, VmMulticoreReplayError> {
        if interval == 0 {
            return Err(VmMulticoreReplayError::ZeroExecutionInterval);
        }
        self.execution_interval = Some(interval);
        Ok(self)
    }
}

/// One replay-stable scheduler event without wall-clock data.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmMulticoreReplayEvent {
    /// Monotonic sequence within one scheduler capture.
    pub(crate) sequence: u64,
    /// Scheduler that owned the observed boundary.
    pub(crate) scheduler: VmSchedulerId,
    /// Stable event classification.
    pub(crate) kind: VmMulticoreEventKind,
    /// Generation-qualified event context.
    pub(crate) context: VmMulticoreEventContext,
}

/// Immutable bounded capture for one fixed scheduler.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmMulticoreReplayCapture {
    /// Versioned capture schema.
    pub(crate) schema: &'static str,
    /// Scheduler whose local decisions are represented.
    pub(crate) scheduler: VmSchedulerId,
    /// First retained sequence, or next sequence for an empty capture.
    pub(crate) first_sequence: u64,
    /// Sequence assigned to the next event after this capture.
    pub(crate) next_sequence: u64,
    /// Number of old events removed under bounded-buffer pressure.
    pub(crate) dropped_events: u64,
    /// Retained events in scheduler-local sequence order.
    pub(crate) events: Vec<VmMulticoreReplayEvent>,
}

impl VmMulticoreReplayCapture {
    /// Returns whether this capture begins at sequence one without loss.
    pub(crate) fn is_complete(&self) -> bool {
        self.dropped_events == 0 && self.first_sequence == 1
    }
}

/// Bounded scheduler-local captures for one live runtime generation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VmMulticoreReplayEvidence {
    /// Versioned scheduler replay schema shared by every child capture.
    pub(crate) schema: &'static str,
    /// Nonzero runtime generation that owned all captured schedulers.
    pub(crate) runtime_generation: u64,
    /// Maximum retained events admitted by this aggregate.
    pub(crate) maximum_events: usize,
    /// Number of retained scheduler-local events.
    pub(crate) retained_events: usize,
    /// Number of scheduler-local prefix events dropped under pressure.
    pub(crate) dropped_events: u64,
    /// Whether every scheduler capture is complete enough for replay.
    pub(crate) replayable: bool,
    /// Captures in deterministic scheduler-index order.
    pub(crate) schedulers: Vec<VmMulticoreReplayCapture>,
}

impl VmMulticoreReplayEvidence {
    /// Validates and aggregates one bounded capture from every scheduler.
    pub(crate) fn new(
        runtime_generation: u64,
        scheduler_count: usize,
        maximum_events: usize,
        mut captures: Vec<VmMulticoreReplayCapture>,
    ) -> Result<Self, VmMulticoreReplayError> {
        if runtime_generation == 0 {
            return Err(VmMulticoreReplayError::ZeroRuntimeGeneration);
        }
        if !(1..=VM_MAX_SCHEDULERS).contains(&scheduler_count) {
            return Err(VmMulticoreReplayError::InvalidSchedulerCount);
        }
        if maximum_events == 0 {
            return Err(VmMulticoreReplayError::ZeroAggregateCapacity);
        }
        if captures.len() != scheduler_count {
            return Err(VmMulticoreReplayError::CaptureCountMismatch {
                expected: scheduler_count,
                actual: captures.len(),
            });
        }
        captures.sort_by_key(|capture| capture.scheduler);
        let mut retained_events = 0_usize;
        let mut dropped_events = 0_u64;
        for (expected, capture) in captures.iter().enumerate() {
            if capture.scheduler.index() != expected {
                return Err(VmMulticoreReplayError::UnexpectedScheduler {
                    expected,
                    actual: capture.scheduler.index(),
                });
            }
            validate_capture_structure(capture)?;
            retained_events = retained_events
                .checked_add(capture.events.len())
                .ok_or(VmMulticoreReplayError::AggregateEventCountExhausted)?;
            dropped_events = dropped_events
                .checked_add(capture.dropped_events)
                .ok_or(VmMulticoreReplayError::DroppedEventCountExhausted)?;
        }
        if retained_events > maximum_events {
            return Err(VmMulticoreReplayError::AggregateCapacityExceeded {
                maximum: maximum_events,
                actual: retained_events,
            });
        }
        let replayable = captures.iter().all(VmMulticoreReplayCapture::is_complete);
        Ok(Self {
            schema: VM_MULTICORE_REPLAY_SCHEMA,
            runtime_generation,
            maximum_events,
            retained_events,
            dropped_events,
            replayable,
            schedulers: captures,
        })
    }
}

/// Recorder operating mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VmMulticoreReplayMode {
    Record,
    Replay,
}

/// Result of recording or validating one scheduler event.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmMulticoreRecordOutcome {
    /// Stable event retained or consumed by replay.
    pub(crate) event: VmMulticoreReplayEvent,
    /// Whether this operation evicted the oldest recorded event.
    pub(crate) evicted: bool,
}

/// Per-scheduler bounded recorder or deterministic replay cursor.
#[derive(Debug)]
pub(crate) struct VmMulticoreReplayRecorder {
    scheduler: VmSchedulerId,
    mode: VmMulticoreReplayMode,
    capacity: usize,
    next_sequence: u64,
    dropped_events: u64,
    recorded: VecDeque<VmMulticoreReplayEvent>,
    expected: VecDeque<VmMulticoreReplayEvent>,
}

impl VmMulticoreReplayRecorder {
    /// Creates one bounded record-mode scheduler stream.
    pub(crate) fn recording(
        scheduler: VmSchedulerId,
        capacity: usize,
    ) -> Result<Self, VmMulticoreReplayError> {
        if capacity == 0 {
            return Err(VmMulticoreReplayError::ZeroCapacity);
        }
        Ok(Self {
            scheduler,
            mode: VmMulticoreReplayMode::Record,
            capacity,
            next_sequence: 1,
            dropped_events: 0,
            recorded: VecDeque::with_capacity(capacity),
            expected: VecDeque::new(),
        })
    }

    /// Creates controlled replay from one complete, contiguous capture.
    pub(crate) fn replaying(
        capture: VmMulticoreReplayCapture,
    ) -> Result<Self, VmMulticoreReplayError> {
        validate_capture(&capture)?;
        let capacity = capture.events.len().max(1);
        Ok(Self {
            scheduler: capture.scheduler,
            mode: VmMulticoreReplayMode::Replay,
            capacity,
            next_sequence: 1,
            dropped_events: 0,
            recorded: VecDeque::new(),
            expected: capture.events.into(),
        })
    }

    /// Records or validates the next exact scheduler-local event.
    pub(crate) fn observe(
        &mut self,
        kind: VmMulticoreEventKind,
        context: VmMulticoreEventContext,
    ) -> Result<VmMulticoreRecordOutcome, VmMulticoreReplayError> {
        let event = VmMulticoreReplayEvent {
            sequence: self.next_sequence,
            scheduler: self.scheduler,
            kind,
            context,
        };
        match self.mode {
            VmMulticoreReplayMode::Record => {
                self.next_sequence = next_sequence(self.next_sequence)?;
                let evicted = if self.recorded.len() == self.capacity {
                    self.recorded.pop_front();
                    self.dropped_events = self
                        .dropped_events
                        .checked_add(1)
                        .ok_or(VmMulticoreReplayError::DroppedEventCountExhausted)?;
                    true
                } else {
                    false
                };
                self.recorded.push_back(event);
                Ok(VmMulticoreRecordOutcome { event, evicted })
            }
            VmMulticoreReplayMode::Replay => {
                let expected = self
                    .expected
                    .front()
                    .copied()
                    .ok_or(VmMulticoreReplayError::ReplayExhausted { actual: event.kind })?;
                if expected != event {
                    return Err(VmMulticoreReplayError::ReplayMismatch {
                        expected,
                        actual: event,
                    });
                }
                self.expected.pop_front();
                self.next_sequence = next_sequence(self.next_sequence)?;
                Ok(VmMulticoreRecordOutcome {
                    event,
                    evicted: false,
                })
            }
        }
    }

    /// Captures the current bounded record stream without closing it.
    pub(crate) fn capture(&self) -> Result<VmMulticoreReplayCapture, VmMulticoreReplayError> {
        if self.mode != VmMulticoreReplayMode::Record {
            return Err(VmMulticoreReplayError::CaptureDuringReplay);
        }
        let first_sequence = self
            .recorded
            .front()
            .map(|event| event.sequence)
            .unwrap_or(self.next_sequence);
        Ok(VmMulticoreReplayCapture {
            schema: VM_MULTICORE_REPLAY_SCHEMA,
            scheduler: self.scheduler,
            first_sequence,
            next_sequence: self.next_sequence,
            dropped_events: self.dropped_events,
            events: self.recorded.iter().copied().collect(),
        })
    }

    /// Requires controlled replay to have consumed every captured event.
    pub(crate) fn finish_replay(&self) -> Result<(), VmMulticoreReplayError> {
        if self.mode != VmMulticoreReplayMode::Replay {
            return Err(VmMulticoreReplayError::FinishOutsideReplay);
        }
        if self.expected.is_empty() {
            Ok(())
        } else {
            Err(VmMulticoreReplayError::ReplayIncomplete {
                remaining: self.expected.len(),
            })
        }
    }
}

/// Typed capture and replay validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmMulticoreReplayError {
    /// A bounded recorder cannot retain zero events.
    ZeroCapacity,
    /// Runtime generation identities are one-based.
    ZeroRuntimeGeneration,
    /// Aggregation requires one to the supported maximum scheduler count.
    InvalidSchedulerCount,
    /// An aggregate cannot admit zero retained events.
    ZeroAggregateCapacity,
    /// Capture count does not match the runtime scheduler topology.
    CaptureCountMismatch {
        /// Number of schedulers owned by the runtime generation.
        expected: usize,
        /// Number of captures supplied by the caller.
        actual: usize,
    },
    /// Sorted captures did not contain exactly one expected scheduler.
    UnexpectedScheduler {
        /// Scheduler index required at this aggregate position.
        expected: usize,
        /// Scheduler index present in the capture.
        actual: usize,
    },
    /// Retained aggregate events exceeded the caller-owned hard bound.
    AggregateCapacityExceeded {
        /// Maximum retained event count admitted by the aggregate.
        maximum: usize,
        /// Retained event count found across all captures.
        actual: usize,
    },
    /// Checked aggregate retained-event accounting overflowed.
    AggregateEventCountExhausted,
    /// Actor identities are one-based.
    ZeroActorIdentity,
    /// Actor generation was supplied without actor identity.
    GenerationWithoutActor,
    /// Actor generations are one-based.
    ZeroActorGeneration,
    /// Shard epochs are one-based.
    ZeroShardEpoch,
    /// Operation publication sequences are one-based.
    ZeroOperationSequence,
    /// Execution interval identities are one-based.
    ZeroExecutionInterval,
    /// Scheduler-local event sequence reached its maximum.
    EventSequenceExhausted,
    /// Dropped-event accounting reached its maximum.
    DroppedEventCountExhausted,
    /// Capture schema is not supported.
    UnsupportedSchema,
    /// A capture with dropped prefix events cannot drive controlled replay.
    IncompleteCapture,
    /// A capture contains a scheduler identity from another stream.
    ForeignSchedulerEvent,
    /// Captured event sequences are not one-based and contiguous.
    CorruptSequence,
    /// Capture first-sequence metadata does not follow its dropped prefix.
    CorruptFirstSequence,
    /// Capture next-sequence metadata does not follow the retained events.
    CorruptNextSequence,
    /// The next observation did not match the captured event.
    ReplayMismatch {
        /// Complete captured event identity.
        expected: VmMulticoreReplayEvent,
        /// Complete observed event identity.
        actual: VmMulticoreReplayEvent,
    },
    /// Replay observed an event after the capture ended.
    ReplayExhausted {
        /// Unexpected observed kind.
        actual: VmMulticoreEventKind,
    },
    /// Replay ended with unconsumed expected events.
    ReplayIncomplete {
        /// Number of events still expected.
        remaining: usize,
    },
    /// Record capture was requested from a replay cursor.
    CaptureDuringReplay,
    /// Replay completion was requested from a recording stream.
    FinishOutsideReplay,
}

impl fmt::Display for VmMulticoreReplayError {
    /// Renders one stable replay diagnostic.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{self:?}")
    }
}

impl std::error::Error for VmMulticoreReplayError {}

/// Advances a one-based sequence without saturation or wraparound.
fn next_sequence(sequence: u64) -> Result<u64, VmMulticoreReplayError> {
    sequence
        .checked_add(1)
        .ok_or(VmMulticoreReplayError::EventSequenceExhausted)
}

/// Validates a capture before any controlled replay state is created.
fn validate_capture(capture: &VmMulticoreReplayCapture) -> Result<(), VmMulticoreReplayError> {
    validate_capture_structure(capture)?;
    if !capture.is_complete() {
        return Err(VmMulticoreReplayError::IncompleteCapture);
    }
    Ok(())
}

/// Validates one diagnostic capture while permitting a declared dropped prefix.
fn validate_capture_structure(
    capture: &VmMulticoreReplayCapture,
) -> Result<(), VmMulticoreReplayError> {
    if capture.schema != VM_MULTICORE_REPLAY_SCHEMA {
        return Err(VmMulticoreReplayError::UnsupportedSchema);
    }
    let expected_first = capture
        .dropped_events
        .checked_add(1)
        .ok_or(VmMulticoreReplayError::CorruptFirstSequence)?;
    if capture.first_sequence != expected_first {
        return Err(VmMulticoreReplayError::CorruptFirstSequence);
    }
    for (index, event) in capture.events.iter().enumerate() {
        if event.scheduler != capture.scheduler {
            return Err(VmMulticoreReplayError::ForeignSchedulerEvent);
        }
        let expected = u64::try_from(index)
            .ok()
            .and_then(|index| capture.first_sequence.checked_add(index))
            .ok_or(VmMulticoreReplayError::CorruptSequence)?;
        if event.sequence != expected {
            return Err(VmMulticoreReplayError::CorruptSequence);
        }
    }
    let expected_next = u64::try_from(capture.events.len())
        .ok()
        .and_then(|length| capture.first_sequence.checked_add(length))
        .ok_or(VmMulticoreReplayError::CorruptNextSequence)?;
    if capture.next_sequence != expected_next {
        return Err(VmMulticoreReplayError::CorruptNextSequence);
    }
    Ok(())
}

#[cfg(test)]
#[path = "multicore_replay_test.rs"]
mod multicore_replay_test;
