//! Product VM façade for deterministic actor supervision.

use std::error::Error;
use std::fmt;

use super::backoff::{
    VmSupervisionBackoffCompletion, VmSupervisionBackoffQueue, VmSupervisionBackoffStart,
    VmSupervisionRestartRequest,
};
use super::shutdown::{
    VmInternalSupervisionShutdownStart, VmSupervisionShutdownCompletion,
    VmSupervisionShutdownQueue, VmSupervisionShutdownRequest,
};
use super::{
    VmChildRestartClass, VmChildSpec, VmRestartPolicy, VmShutdownTimeout,
    VmSupervisionMemoryPressure, VmSupervisionRestart, VmSupervisionSystem, VmSupervisorId,
    VmSupervisorRestartHistoryOutcome, VmSupervisorState,
};
use crate::runtime::vm::memory::{VmMemoryAccountant, VmMemoryLimits};
use crate::runtime::vm::process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessTable};
use crate::runtime::vm::resource::VmResourceTable;
use crate::runtime::vm::restart_backoff::VmRestartBackoffSchedule;
use crate::runtime::vm::scheduler::VmScheduler;
use crate::runtime::vm::timer::{VmTimerId, VmTimerTable};

/// Stable external identity of one VM-owned supervisor node.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct VmSupervisorHandle(u64);

impl VmSupervisorHandle {
    /// Returns the numeric identity retained in inspection evidence.
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Stable external identity of one supervised VM actor.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct VmSupervisedChild(u64);

impl VmSupervisedChild {
    /// Returns the VM process identity.
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

/// Stable public category for one rejected supervision operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VmSupervisionErrorKind {
    Configuration,
    Hierarchy,
    ChildLifecycle,
    Restart,
    Shutdown,
    Memory,
    Inspection,
}

/// Typed failure returned by the product VM supervision façade.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmSupervisionError {
    kind: VmSupervisionErrorKind,
    message: String,
}

impl VmSupervisionError {
    fn new(kind: VmSupervisionErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// Returns the stable operation category.
    pub const fn kind(&self) -> VmSupervisionErrorKind {
        self.kind
    }

    /// Returns the underlying VM diagnostic.
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for VmSupervisionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for VmSupervisionError {}

/// Restart selection applied to siblings after one child exits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VmSupervisionStrategy {
    OneForOne,
    OneForAll,
    RestForOne,
}

/// Restart eligibility of one child.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum VmSupervisionRestartClass {
    Permanent,
    Transient,
    Temporary,
}

/// Public child declaration converted into a VM-owned process specification.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmSupervisionChildSpec {
    pub id: String,
    pub module: String,
    pub function: String,
    pub arity: usize,
    pub restart_limit: u32,
    pub restart_class: VmSupervisionRestartClass,
    pub initial_backoff_ms: u64,
    pub maximum_backoff_ms: u64,
    pub shutdown_timeout_ms: Option<u64>,
}

impl VmSupervisionChildSpec {
    /// Creates a permanent child with explicit restart intensity.
    pub fn permanent(
        id: impl Into<String>,
        module: impl Into<String>,
        function: impl Into<String>,
        arity: usize,
        restart_limit: u32,
    ) -> Self {
        Self {
            id: id.into(),
            module: module.into(),
            function: function.into(),
            arity,
            restart_limit,
            restart_class: VmSupervisionRestartClass::Permanent,
            initial_backoff_ms: 0,
            maximum_backoff_ms: 0,
            shutdown_timeout_ms: None,
        }
    }

    /// Selects permanent, transient, or temporary restart behavior.
    pub const fn with_restart_class(mut self, restart_class: VmSupervisionRestartClass) -> Self {
        self.restart_class = restart_class;
        self
    }

    /// Selects deterministic exponential restart delay bounds.
    pub const fn with_backoff(mut self, initial_ms: u64, maximum_ms: u64) -> Self {
        self.initial_backoff_ms = initial_ms;
        self.maximum_backoff_ms = maximum_ms;
        self
    }

    /// Selects the graceful-shutdown deadline for this child.
    pub const fn with_shutdown_timeout(mut self, timeout_ms: u64) -> Self {
        self.shutdown_timeout_ms = Some(timeout_ms);
        self
    }
}

/// Opaque VM timer authority returned for a pending lifecycle action.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VmSupervisionDeadline {
    timer_id: VmTimerId,
    pub child_id: VmSupervisedChild,
    pub deadline_tick: u64,
}

impl VmSupervisionDeadline {
    /// Returns the inspection-visible VM timer identity.
    pub fn timer_id(&self) -> u64 {
        self.timer_id.as_u64()
    }
}

/// Deterministic result of applying one restart decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VmSupervisionOutcome {
    Restarted {
        old_child: VmSupervisedChild,
        new_child: VmSupervisedChild,
        restart_count: u32,
        delay_ms: u64,
        shutdown_timeout_ms: Option<u64>,
    },
    RestartedGroup(Vec<(String, VmSupervisedChild, VmSupervisedChild)>),
    NotRestarted {
        child: VmSupervisedChild,
        restart_class: VmSupervisionRestartClass,
    },
    LimitReached {
        child: VmSupervisedChild,
        restart_count: u32,
    },
    Cancelled(VmSupervisedChild),
    TimerOwnerExited(VmSupervisedChild),
    Stale {
        expected: VmSupervisedChild,
        current: VmSupervisedChild,
    },
}

/// Immediate or VM-timer-backed restart admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VmSupervisionRestartStart {
    Immediate(VmSupervisionOutcome),
    Deferred {
        immediate: Vec<VmSupervisionOutcome>,
        deadlines: Vec<VmSupervisionDeadline>,
    },
}

/// Immediate or graceful child shutdown admission.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VmSupervisionShutdownStart {
    Immediate(VmSupervisionOutcome),
    Waiting(VmSupervisionDeadline),
}

/// Scheduler-facing results produced by a monotonic clock advance.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct VmSupervisionAdvance {
    pub outcomes: Vec<VmSupervisionOutcome>,
    pub unhandled_timer_count: usize,
}

/// Product-facing result of applying VM memory-pressure supervision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VmSupervisionMemoryDecision {
    Continue(VmSupervisedChild),
    Collect {
        child: VmSupervisedChild,
        projected_bytes: usize,
    },
    Restart(VmSupervisionOutcome),
}

/// Inspection-visible supervisor lifecycle state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum VmSupervisionState {
    Running,
    Failed {
        child_id: String,
        reason: String,
    },
    ChildSupervisorFailed {
        supervisor: VmSupervisorHandle,
        reason: String,
    },
}

/// Immutable VM supervision graph evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VmSupervisionSnapshot {
    pub supervisor: VmSupervisorHandle,
    pub parent: Option<VmSupervisorHandle>,
    pub name: String,
    pub strategy: VmSupervisionStrategy,
    pub state: VmSupervisionState,
    pub children: Vec<(String, VmSupervisedChild, u32, u32)>,
    pub restart_history: Vec<(String, VmSupervisedChild, Option<VmSupervisedChild>, String)>,
}

/// Complete product VM owner for process tables, timers, restart policy, and shutdown.
#[derive(Debug)]
pub struct VmSupervisionRuntime {
    root: VmSupervisorId,
    system: VmSupervisionSystem,
    processes: VmProcessTable,
    memory: VmMemoryAccountant,
    resources: VmResourceTable,
    restart_timers: VmTimerTable,
    shutdown_timers: VmTimerTable,
    scheduler: VmScheduler,
    backoff: VmSupervisionBackoffQueue,
    shutdown: VmSupervisionShutdownQueue,
}

impl VmSupervisionRuntime {
    /// Creates a VM-owned root supervision tree with bounded actor memory.
    pub fn new(
        name: impl Into<String>,
        strategy: VmSupervisionStrategy,
    ) -> Result<Self, VmSupervisionError> {
        let mut system = VmSupervisionSystem::default();
        let name = name.into();
        let root = match strategy {
            VmSupervisionStrategy::OneForOne => system.create_supervisor(name),
            other => system.create_supervisor_with_policy(name, other.into()),
        };
        Ok(Self {
            root,
            system,
            processes: VmProcessTable::default(),
            memory: VmMemoryAccountant::new(
                VmMemoryLimits::new(64 * 1024 * 1024, 256 * 1024 * 1024).map_err(|message| {
                    VmSupervisionError::new(VmSupervisionErrorKind::Configuration, message)
                })?,
            ),
            resources: VmResourceTable::default(),
            restart_timers: VmTimerTable::default(),
            shutdown_timers: VmTimerTable::default(),
            scheduler: VmScheduler::default(),
            backoff: VmSupervisionBackoffQueue::default(),
            shutdown: VmSupervisionShutdownQueue::default(),
        })
    }

    /// Returns the root supervisor created with this runtime.
    pub fn root(&self) -> VmSupervisorHandle {
        self.root.into()
    }

    /// Creates a nested supervisor whose terminal state escalates to its parent.
    pub fn create_child_supervisor(
        &mut self,
        parent: VmSupervisorHandle,
        name: impl Into<String>,
        strategy: VmSupervisionStrategy,
    ) -> Result<VmSupervisorHandle, VmSupervisionError> {
        self.system
            .create_child_supervisor_with_policy(parent.into(), name, strategy.into())
            .map(Into::into)
            .map_err(|message| VmSupervisionError::new(VmSupervisionErrorKind::Hierarchy, message))
    }

    /// Starts one typed child under an exact supervisor node.
    pub fn start_child(
        &mut self,
        supervisor: VmSupervisorHandle,
        spec: VmSupervisionChildSpec,
    ) -> Result<VmSupervisedChild, VmSupervisionError> {
        validate_spec(&spec).map_err(|message| {
            VmSupervisionError::new(VmSupervisionErrorKind::ChildLifecycle, message)
        })?;
        self.system
            .start_child(&mut self.processes, supervisor.into(), spec.into())
            .map(Into::into)
            .map_err(|message| {
                VmSupervisionError::new(VmSupervisionErrorKind::ChildLifecycle, message)
            })
    }

    /// Applies a restart synchronously without a backoff deadline.
    pub fn restart_now(
        &mut self,
        supervisor: VmSupervisorHandle,
        child_id: &str,
        reason: impl Into<String>,
    ) -> Result<VmSupervisionOutcome, VmSupervisionError> {
        self.system
            .restart_child(
                &mut self.processes,
                supervisor.into(),
                child_id,
                VmExitReason::Error(reason.into()),
            )
            .map(map_restart)
            .map_err(|message| VmSupervisionError::new(VmSupervisionErrorKind::Restart, message))
    }

    /// Executes the parent supervisor's strategy after a child supervisor
    /// reaches a terminal restart outcome.
    pub fn restart_failed_supervisor(
        &mut self,
        failed: VmSupervisorHandle,
        reason: impl Into<String>,
    ) -> Result<Vec<VmSupervisorHandle>, VmSupervisionError> {
        self.system
            .restart_failed_supervisor(
                &mut self.processes,
                failed.into(),
                VmExitReason::Error(reason.into()),
            )
            .map(|restarted| restarted.into_iter().map(Into::into).collect())
            .map_err(|message| VmSupervisionError::new(VmSupervisionErrorKind::Restart, message))
    }

    /// Applies restart policy and installs VM timer deadlines for delayed children.
    pub fn schedule_restart(
        &mut self,
        supervisor: VmSupervisorHandle,
        child_id: &str,
        reason: impl Into<String>,
        now_tick: u64,
    ) -> Result<VmSupervisionRestartStart, VmSupervisionError> {
        let start = self
            .backoff
            .schedule_restart(
                &mut self.system,
                &mut self.restart_timers,
                &mut self.processes,
                VmSupervisionRestartRequest::new(
                    supervisor.into(),
                    child_id,
                    VmExitReason::Error(reason.into()),
                    now_tick,
                ),
            )
            .map_err(|message| VmSupervisionError::new(VmSupervisionErrorKind::Restart, message))?;
        Ok(match start {
            VmSupervisionBackoffStart::Immediate(outcome) => {
                VmSupervisionRestartStart::Immediate(map_restart(outcome))
            }
            VmSupervisionBackoffStart::Deferred {
                restarted_immediately,
                scheduled,
            } => VmSupervisionRestartStart::Deferred {
                immediate: restarted_immediately.into_iter().map(map_restart).collect(),
                deadlines: scheduled
                    .into_iter()
                    .map(|scheduled| VmSupervisionDeadline {
                        timer_id: scheduled.timer_id,
                        child_id: scheduled.failed_pid.into(),
                        deadline_tick: scheduled.deadline_tick,
                    })
                    .collect(),
            },
        })
    }

    /// Cancels one exact pending restart without manufacturing timer identity.
    pub fn cancel_restart(
        &mut self,
        deadline: VmSupervisionDeadline,
    ) -> Result<VmSupervisionOutcome, VmSupervisionError> {
        self.backoff
            .cancel_restart(
                &mut self.system,
                &mut self.restart_timers,
                &mut self.processes,
                deadline.timer_id,
            )
            .map(map_backoff_completion)
            .map_err(|message| VmSupervisionError::new(VmSupervisionErrorKind::Restart, message))
    }

    /// Advances only restart deadlines and applies due restart intents.
    pub fn advance_restart_clock(
        &mut self,
        now_tick: u64,
    ) -> Result<VmSupervisionAdvance, VmSupervisionError> {
        let events =
            self.restart_timers
                .advance_clock(&mut self.processes, &mut self.scheduler, now_tick);
        let mut advance = VmSupervisionAdvance::default();
        for event in &events {
            match self
                .backoff
                .handle_timer_event(&mut self.system, &mut self.processes, event)
                .map_err(|message| {
                    VmSupervisionError::new(VmSupervisionErrorKind::Restart, message)
                })? {
                Some(outcome) => advance.outcomes.push(map_backoff_completion(outcome)),
                None => advance.unhandled_timer_count += 1,
            }
        }
        Ok(advance)
    }

    /// Starts graceful shutdown or applies immediate termination policy.
    pub fn begin_shutdown(
        &mut self,
        supervisor: VmSupervisorHandle,
        child_id: &str,
        now_tick: u64,
    ) -> Result<VmSupervisionShutdownStart, VmSupervisionError> {
        let start = self
            .shutdown
            .begin_shutdown(
                &mut self.system,
                &mut self.shutdown_timers,
                &mut self.processes,
                VmSupervisionShutdownRequest::new(
                    supervisor.into(),
                    child_id,
                    VmExitReason::Killed,
                    now_tick,
                ),
            )
            .map_err(|message| {
                VmSupervisionError::new(VmSupervisionErrorKind::Shutdown, message)
            })?;
        Ok(match start {
            VmInternalSupervisionShutdownStart::Immediate(outcome) => {
                VmSupervisionShutdownStart::Immediate(map_restart(outcome))
            }
            VmInternalSupervisionShutdownStart::Waiting(scheduled) => {
                VmSupervisionShutdownStart::Waiting(VmSupervisionDeadline {
                    timer_id: scheduled.timer_id,
                    child_id: scheduled.pid.into(),
                    deadline_tick: scheduled.deadline_tick,
                })
            }
        })
    }

    /// Records cooperative child exit and cancels its shutdown deadline.
    pub fn complete_shutdown(
        &mut self,
        supervisor: VmSupervisorHandle,
        child_id: &str,
    ) -> Result<VmSupervisionOutcome, VmSupervisionError> {
        let pid = child_pid(&self.system, supervisor.into(), child_id).map_err(|message| {
            VmSupervisionError::new(VmSupervisionErrorKind::Shutdown, message)
        })?;
        self.processes
            .exit_process(pid, VmExitReason::Normal)
            .map_err(|message| {
                VmSupervisionError::new(VmSupervisionErrorKind::Shutdown, message)
            })?;
        self.shutdown
            .complete_shutdown(
                &mut self.system,
                &mut self.shutdown_timers,
                &mut self.processes,
                supervisor.into(),
                child_id,
            )
            .map(map_shutdown_completion)
            .map_err(|message| VmSupervisionError::new(VmSupervisionErrorKind::Shutdown, message))
    }

    /// Advances graceful-shutdown deadlines on the VM scheduler clock.
    pub fn advance_shutdown_clock(
        &mut self,
        now_tick: u64,
    ) -> Result<VmSupervisionAdvance, VmSupervisionError> {
        let advance = self
            .shutdown
            .advance_clock(
                &mut self.system,
                &mut self.shutdown_timers,
                &mut self.processes,
                &mut self.scheduler,
                now_tick,
            )
            .map_err(|message| {
                VmSupervisionError::new(VmSupervisionErrorKind::Shutdown, message)
            })?;
        Ok(VmSupervisionAdvance {
            outcomes: advance
                .completions
                .into_iter()
                .map(map_shutdown_completion)
                .collect(),
            unhandled_timer_count: advance.unhandled_timer_events.len(),
        })
    }

    /// Charges child memory and routes hard-limit rejection through supervision.
    pub fn charge_child_memory(
        &mut self,
        supervisor: VmSupervisorHandle,
        child_id: &str,
        requested_bytes: usize,
    ) -> Result<VmSupervisionMemoryDecision, VmSupervisionError> {
        let pid = child_pid(&self.system, supervisor.into(), child_id)
            .map_err(|message| VmSupervisionError::new(VmSupervisionErrorKind::Memory, message))?;
        let pressure = self
            .memory
            .account_heap(&mut self.processes, pid, requested_bytes)
            .map_err(|message| VmSupervisionError::new(VmSupervisionErrorKind::Memory, message))?;
        match self
            .system
            .handle_memory_pressure(
                &mut self.memory,
                &mut self.resources,
                &mut self.processes,
                supervisor.into(),
                child_id,
                &pressure,
            )
            .map_err(|message| VmSupervisionError::new(VmSupervisionErrorKind::Memory, message))?
        {
            VmSupervisionMemoryPressure::Continue { pid } => {
                Ok(VmSupervisionMemoryDecision::Continue(pid.into()))
            }
            VmSupervisionMemoryPressure::Collect {
                pid,
                projected_bytes,
            } => Ok(VmSupervisionMemoryDecision::Collect {
                child: pid.into(),
                projected_bytes,
            }),
            VmSupervisionMemoryPressure::Restart(outcome) => {
                Ok(VmSupervisionMemoryDecision::Restart(map_restart(outcome)))
            }
        }
    }

    /// Returns the number of pending restart and shutdown intents.
    pub fn pending_lifecycle_count(&self) -> usize {
        self.backoff.pending_len() + self.shutdown.pending_len()
    }

    /// Captures deterministic graph and restart-history evidence.
    pub fn snapshot(
        &self,
        supervisor: VmSupervisorHandle,
    ) -> Result<VmSupervisionSnapshot, VmSupervisionError> {
        let snapshot = self.system.snapshot(supervisor.into()).map_err(|message| {
            VmSupervisionError::new(VmSupervisionErrorKind::Inspection, message)
        })?;
        Ok(VmSupervisionSnapshot {
            supervisor: snapshot.id.into(),
            parent: snapshot.parent_id.map(Into::into),
            name: snapshot.name,
            strategy: snapshot.policy.into(),
            state: map_state(snapshot.state),
            children: snapshot
                .children
                .into_iter()
                .map(|child| {
                    (
                        child.child_id,
                        child.pid.into(),
                        child.restart_count,
                        child.restart_limit,
                    )
                })
                .collect(),
            restart_history: snapshot
                .restart_history
                .into_iter()
                .map(|entry| {
                    (
                        entry.child_id,
                        entry.old_pid.into(),
                        entry.new_pid.map(Into::into),
                        match entry.outcome {
                            VmSupervisorRestartHistoryOutcome::Restarted => "restarted",
                            VmSupervisorRestartHistoryOutcome::NotRestarted => "not_restarted",
                            VmSupervisorRestartHistoryOutcome::LimitReached => "limit_reached",
                        }
                        .to_string(),
                    )
                })
                .collect(),
        })
    }
}

fn validate_spec(spec: &VmSupervisionChildSpec) -> Result<(), String> {
    if spec.id.trim().is_empty() || spec.module.trim().is_empty() || spec.function.trim().is_empty()
    {
        return Err("supervised child id, module, and function must be nonempty".to_string());
    }
    if spec.initial_backoff_ms > spec.maximum_backoff_ms && spec.maximum_backoff_ms != 0 {
        return Err("supervision initial backoff exceeds maximum backoff".to_string());
    }
    Ok(())
}

fn child_pid(
    system: &VmSupervisionSystem,
    supervisor: VmSupervisorId,
    child_id: &str,
) -> Result<VmProcessId, String> {
    system
        .snapshot(supervisor)?
        .children
        .into_iter()
        .find_map(|child| (child.child_id == child_id).then_some(child.pid))
        .ok_or_else(|| format!("missing child `{child_id}`"))
}

fn map_restart(outcome: VmSupervisionRestart) -> VmSupervisionOutcome {
    match outcome {
        VmSupervisionRestart::Restarted {
            old_pid,
            new_pid,
            restart_count,
            restart_delay_ms,
            shutdown_timeout_ms,
        } => VmSupervisionOutcome::Restarted {
            old_child: old_pid.into(),
            new_child: new_pid.into(),
            restart_count,
            delay_ms: restart_delay_ms,
            shutdown_timeout_ms,
        },
        VmSupervisionRestart::RestartedGroup { restarted } => VmSupervisionOutcome::RestartedGroup(
            restarted
                .into_iter()
                .map(|event| (event.child_id, event.old_pid.into(), event.new_pid.into()))
                .collect(),
        ),
        VmSupervisionRestart::NotRestarted {
            pid, restart_class, ..
        } => VmSupervisionOutcome::NotRestarted {
            child: pid.into(),
            restart_class: restart_class.into(),
        },
        VmSupervisionRestart::LimitReached { pid, restart_count } => {
            VmSupervisionOutcome::LimitReached {
                child: pid.into(),
                restart_count,
            }
        }
    }
}

fn map_backoff_completion(outcome: VmSupervisionBackoffCompletion) -> VmSupervisionOutcome {
    match outcome {
        VmSupervisionBackoffCompletion::Restarted(outcome) => map_restart(outcome),
        VmSupervisionBackoffCompletion::Cancelled { failed_pid, .. } => {
            VmSupervisionOutcome::Cancelled(failed_pid.into())
        }
        VmSupervisionBackoffCompletion::TimerOwnerExited { failed_pid, .. } => {
            VmSupervisionOutcome::TimerOwnerExited(failed_pid.into())
        }
        VmSupervisionBackoffCompletion::Stale {
            failed_pid,
            current_pid,
            ..
        } => VmSupervisionOutcome::Stale {
            expected: failed_pid.into(),
            current: current_pid.into(),
        },
    }
}

fn map_shutdown_completion(outcome: VmSupervisionShutdownCompletion) -> VmSupervisionOutcome {
    match outcome {
        VmSupervisionShutdownCompletion::Exited { restart, .. }
        | VmSupervisionShutdownCompletion::TimedOut { restart, .. } => map_restart(restart),
        VmSupervisionShutdownCompletion::Cancelled { pid, .. } => {
            VmSupervisionOutcome::Cancelled(pid.into())
        }
        VmSupervisionShutdownCompletion::TimerOwnerExited { pid, .. } => {
            VmSupervisionOutcome::TimerOwnerExited(pid.into())
        }
        VmSupervisionShutdownCompletion::Stale {
            expected_pid,
            current_pid,
            ..
        } => VmSupervisionOutcome::Stale {
            expected: expected_pid.into(),
            current: current_pid.into(),
        },
    }
}

fn map_state(state: VmSupervisorState) -> VmSupervisionState {
    match state {
        VmSupervisorState::Running => VmSupervisionState::Running,
        VmSupervisorState::Failed {
            child_id, reason, ..
        } => VmSupervisionState::Failed {
            child_id,
            reason: format!("{reason:?}"),
        },
        VmSupervisorState::ChildSupervisorFailed {
            supervisor_id,
            reason,
        } => VmSupervisionState::ChildSupervisorFailed {
            supervisor: supervisor_id.into(),
            reason: format!("{reason:?}"),
        },
    }
}

impl From<VmSupervisorId> for VmSupervisorHandle {
    fn from(value: VmSupervisorId) -> Self {
        Self(value.as_u64())
    }
}

impl From<VmSupervisorHandle> for VmSupervisorId {
    fn from(value: VmSupervisorHandle) -> Self {
        Self(value.0)
    }
}

impl From<VmProcessId> for VmSupervisedChild {
    fn from(value: VmProcessId) -> Self {
        Self(value.as_u64())
    }
}

impl From<VmSupervisionStrategy> for VmRestartPolicy {
    fn from(value: VmSupervisionStrategy) -> Self {
        match value {
            VmSupervisionStrategy::OneForOne => Self::OneForOne,
            VmSupervisionStrategy::OneForAll => Self::OneForAll,
            VmSupervisionStrategy::RestForOne => Self::RestForOne,
        }
    }
}

impl From<VmRestartPolicy> for VmSupervisionStrategy {
    fn from(value: VmRestartPolicy) -> Self {
        match value {
            VmRestartPolicy::OneForOne => Self::OneForOne,
            VmRestartPolicy::OneForAll => Self::OneForAll,
            VmRestartPolicy::RestForOne => Self::RestForOne,
        }
    }
}

impl From<VmChildRestartClass> for VmSupervisionRestartClass {
    fn from(value: VmChildRestartClass) -> Self {
        match value {
            VmChildRestartClass::Permanent => Self::Permanent,
            VmChildRestartClass::Transient => Self::Transient,
            VmChildRestartClass::Temporary => Self::Temporary,
        }
    }
}

impl From<VmSupervisionRestartClass> for VmChildRestartClass {
    fn from(value: VmSupervisionRestartClass) -> Self {
        match value {
            VmSupervisionRestartClass::Permanent => Self::Permanent,
            VmSupervisionRestartClass::Transient => Self::Transient,
            VmSupervisionRestartClass::Temporary => Self::Temporary,
        }
    }
}

impl From<VmSupervisionChildSpec> for VmChildSpec {
    fn from(value: VmSupervisionChildSpec) -> Self {
        let mut spec = VmChildSpec::new(
            value.id,
            VmProcessSource::new(value.module, value.function, value.arity),
            value.restart_limit,
        )
        .with_restart_class(value.restart_class.into());
        if value.initial_backoff_ms != 0 || value.maximum_backoff_ms != 0 {
            spec = spec.with_restart_backoff(VmRestartBackoffSchedule::exponential(
                value.initial_backoff_ms,
                value.maximum_backoff_ms,
            ));
        }
        if let Some(timeout_ms) = value.shutdown_timeout_ms {
            spec = spec.with_shutdown_timeout(VmShutdownTimeout::milliseconds(timeout_ms));
        }
        spec
    }
}
