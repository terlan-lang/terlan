use std::collections::BTreeMap;

use super::{
    memory::{VmMemoryAccountant, VmMemoryPressureDecision, VmMemoryPressureOutcome},
    process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState, VmProcessTable},
    resource::VmResourceTable,
    restart_backoff::VmRestartBackoffSchedule,
};

pub(crate) mod backoff;
mod runtime;
pub(crate) mod shutdown;

pub use runtime::{
    VmSupervisedChild, VmSupervisionAdvance, VmSupervisionChildSpec, VmSupervisionDeadline,
    VmSupervisionError, VmSupervisionErrorKind, VmSupervisionMemoryDecision, VmSupervisionOutcome,
    VmSupervisionRestartClass, VmSupervisionRestartStart, VmSupervisionRuntime,
    VmSupervisionShutdownStart, VmSupervisionSnapshot, VmSupervisionState, VmSupervisionStrategy,
    VmSupervisorHandle,
};

fn live_timer_owner(
    slot: &mut Option<VmProcessId>,
    module: &'static str,
    processes: &mut VmProcessTable,
) -> VmProcessId {
    if let Some(owner) = *slot {
        if matches!(
            processes.get(owner).map(|process| &process.state),
            Some(
                VmProcessState::Runnable
                    | VmProcessState::Blocked
                    | VmProcessState::Hibernated
                    | VmProcessState::Suspended(_),
            )
        ) {
            return owner;
        }
    }
    let owner = processes.spawn_root(VmProcessSource::new(module, "wait", 0));
    *slot = Some(owner);
    owner
}

/// VM-owned supervisor identifier.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmSupervisorId(u64);

impl VmSupervisorId {
    /// Returns the numeric supervisor id.
    pub(crate) fn as_u64(self) -> u64 {
        self.0
    }
}

/// Supported supervisor restart policy.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmRestartPolicy {
    OneForOne,
    OneForAll,
    RestForOne,
}

/// Inspection-visible supervisor state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmSupervisorState {
    Running,
    Failed {
        child_id: String,
        pid: VmProcessId,
        reason: VmExitReason,
    },
    ChildSupervisorFailed {
        supervisor_id: VmSupervisorId,
        reason: VmExitReason,
    },
}

/// Inspection-visible restart history outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmSupervisorRestartHistoryOutcome {
    Restarted,
    NotRestarted,
    LimitReached,
}

/// Inspection-visible restart history entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmSupervisorRestartHistoryEntry {
    pub(crate) child_id: String,
    pub(crate) old_pid: VmProcessId,
    pub(crate) new_pid: Option<VmProcessId>,
    pub(crate) restart_count: u32,
    pub(crate) reason: VmExitReason,
    pub(crate) outcome: VmSupervisorRestartHistoryOutcome,
    pub(crate) restart_delay_ms: u64,
    pub(crate) shutdown_timeout_ms: Option<u64>,
}

/// Restart class for a child process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmChildRestartClass {
    Permanent,
    Transient,
    Temporary,
}

impl VmChildRestartClass {
    fn should_restart(&self, reason: &VmExitReason) -> bool {
        match self {
            VmChildRestartClass::Permanent => true,
            VmChildRestartClass::Transient => !matches!(reason, VmExitReason::Normal),
            VmChildRestartClass::Temporary => false,
        }
    }
}

/// Deterministic shutdown timeout policy for a supervised child.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmShutdownTimeout {
    pub(crate) timeout_ms: u64,
}

impl VmShutdownTimeout {
    /// Creates a child shutdown timeout in milliseconds.
    pub(crate) fn milliseconds(timeout_ms: u64) -> Self {
        Self { timeout_ms }
    }
}

/// Child process specification owned by a supervisor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmChildSpec {
    pub(crate) id: String,
    pub(crate) source: VmProcessSource,
    pub(crate) restart_limit: u32,
    pub(crate) restart_class: VmChildRestartClass,
    pub(crate) restart_backoff: Option<VmRestartBackoffSchedule>,
    pub(crate) shutdown_timeout: Option<VmShutdownTimeout>,
}

impl VmChildSpec {
    /// Creates a restartable child specification.
    pub(crate) fn new(id: impl Into<String>, source: VmProcessSource, restart_limit: u32) -> Self {
        Self {
            id: id.into(),
            source,
            restart_limit,
            restart_class: VmChildRestartClass::Permanent,
            restart_backoff: None,
            shutdown_timeout: None,
        }
    }

    /// Assigns the child restart class.
    pub(crate) fn with_restart_class(mut self, restart_class: VmChildRestartClass) -> Self {
        self.restart_class = restart_class;
        self
    }

    /// Assigns a deterministic restart backoff schedule.
    pub(crate) fn with_restart_backoff(
        mut self,
        restart_backoff: VmRestartBackoffSchedule,
    ) -> Self {
        self.restart_backoff = Some(restart_backoff);
        self
    }

    /// Assigns deterministic child shutdown timeout metadata.
    pub(crate) fn with_shutdown_timeout(mut self, shutdown_timeout: VmShutdownTimeout) -> Self {
        self.shutdown_timeout = Some(shutdown_timeout);
        self
    }
}

/// Restart result emitted by a supervisor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmSupervisionRestartEvent {
    pub(crate) child_id: String,
    pub(crate) old_pid: VmProcessId,
    pub(crate) new_pid: VmProcessId,
    pub(crate) restart_count: u32,
    pub(crate) restart_delay_ms: u64,
    pub(crate) shutdown_timeout_ms: Option<u64>,
}

/// Restart result emitted by a supervisor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmSupervisionRestart {
    Restarted {
        old_pid: VmProcessId,
        new_pid: VmProcessId,
        restart_count: u32,
        restart_delay_ms: u64,
        shutdown_timeout_ms: Option<u64>,
    },
    RestartedGroup {
        restarted: Vec<VmSupervisionRestartEvent>,
    },
    NotRestarted {
        pid: VmProcessId,
        restart_class: VmChildRestartClass,
        reason: VmExitReason,
    },
    LimitReached {
        pid: VmProcessId,
        restart_count: u32,
    },
}

/// Supervisor response to one VM-owned memory-pressure decision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmSupervisionMemoryPressure {
    Continue {
        pid: VmProcessId,
    },
    Collect {
        pid: VmProcessId,
        projected_bytes: usize,
    },
    Restart(VmSupervisionRestart),
}

/// Read-only child row for runtime inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmSupervisorChildSnapshot {
    pub(crate) child_id: String,
    pub(crate) pid: VmProcessId,
    pub(crate) source: VmProcessSource,
    pub(crate) restart_count: u32,
    pub(crate) restart_limit: u32,
    pub(crate) restart_class: VmChildRestartClass,
    pub(crate) last_restart_delay_ms: u64,
    pub(crate) shutdown_timeout_ms: Option<u64>,
    pub(crate) last_shutdown_timeout_ms: Option<u64>,
}

/// Read-only supervisor tree for runtime inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmSupervisorSnapshot {
    pub(crate) id: VmSupervisorId,
    pub(crate) parent_id: Option<VmSupervisorId>,
    pub(crate) name: String,
    pub(crate) policy: VmRestartPolicy,
    pub(crate) state: VmSupervisorState,
    pub(crate) children: Vec<VmSupervisorChildSnapshot>,
    pub(crate) restart_history: Vec<VmSupervisorRestartHistoryEntry>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VmSupervisorChild {
    spec: VmChildSpec,
    pid: VmProcessId,
    restart_count: u32,
    last_restart_delay_ms: u64,
    last_shutdown_timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VmSupervisor {
    id: VmSupervisorId,
    parent_id: Option<VmSupervisorId>,
    name: String,
    policy: VmRestartPolicy,
    state: VmSupervisorState,
    child_order: Vec<String>,
    children: BTreeMap<String, VmSupervisorChild>,
    restart_history: Vec<VmSupervisorRestartHistoryEntry>,
}

/// VM-owned supervision system.
///
/// Inputs:
/// - Supervisor creation, child specs, observed child exits, and restart
///   requests.
///
/// Output:
/// - Restart decisions and inspection-visible supervisor trees.
///
/// Transformation:
/// - Models OTP-grade supervision in Terlan VM terms without importing OTP
///   supervisor modules or process semantics.
#[derive(Debug, Default)]
pub(crate) struct VmSupervisionSystem {
    next_supervisor_id: u64,
    supervisors: BTreeMap<VmSupervisorId, VmSupervisor>,
}

impl VmSupervisionSystem {
    /// Creates a supervisor with one-for-one restart policy.
    pub(crate) fn create_supervisor(&mut self, name: impl Into<String>) -> VmSupervisorId {
        self.create_supervisor_with_policy(name, VmRestartPolicy::OneForOne)
    }

    /// Creates a supervisor with an explicit restart policy.
    pub(crate) fn create_supervisor_with_policy(
        &mut self,
        name: impl Into<String>,
        policy: VmRestartPolicy,
    ) -> VmSupervisorId {
        self.insert_supervisor(None, name, policy)
    }

    /// Creates a child supervisor with an explicit parent and restart policy.
    pub(crate) fn create_child_supervisor_with_policy(
        &mut self,
        parent_id: VmSupervisorId,
        name: impl Into<String>,
        policy: VmRestartPolicy,
    ) -> Result<VmSupervisorId, String> {
        if !self.supervisors.contains_key(&parent_id) {
            return Err(format!("missing supervisor {}", parent_id.as_u64()));
        }
        Ok(self.insert_supervisor(Some(parent_id), name, policy))
    }

    fn insert_supervisor(
        &mut self,
        parent_id: Option<VmSupervisorId>,
        name: impl Into<String>,
        policy: VmRestartPolicy,
    ) -> VmSupervisorId {
        self.next_supervisor_id = self.next_supervisor_id.saturating_add(1);
        let id = VmSupervisorId(self.next_supervisor_id);
        self.supervisors.insert(
            id,
            VmSupervisor {
                id,
                parent_id,
                name: name.into(),
                policy,
                state: VmSupervisorState::Running,
                child_order: Vec::new(),
                children: BTreeMap::new(),
                restart_history: Vec::new(),
            },
        );
        id
    }

    /// Starts a child process under a supervisor.
    pub(crate) fn start_child(
        &mut self,
        processes: &mut VmProcessTable,
        supervisor_id: VmSupervisorId,
        spec: VmChildSpec,
    ) -> Result<VmProcessId, String> {
        let supervisor = self
            .supervisors
            .get_mut(&supervisor_id)
            .ok_or_else(|| format!("missing supervisor {}", supervisor_id.as_u64()))?;
        if supervisor.children.contains_key(&spec.id) {
            return Err(format!("child `{}` already exists", spec.id));
        }
        let pid = processes.spawn_root(spec.source.clone());
        let child_id = spec.id.clone();
        supervisor.children.insert(
            child_id.clone(),
            VmSupervisorChild {
                spec,
                pid,
                restart_count: 0,
                last_restart_delay_ms: 0,
                last_shutdown_timeout_ms: None,
            },
        );
        supervisor.child_order.push(child_id);
        Ok(pid)
    }

    /// Applies one-for-one restart policy for an exited child.
    pub(crate) fn restart_child(
        &mut self,
        processes: &mut VmProcessTable,
        supervisor_id: VmSupervisorId,
        child_id: &str,
        reason: VmExitReason,
    ) -> Result<VmSupervisionRestart, String> {
        let (restart, parent_failure) = {
            let supervisor = self
                .supervisors
                .get_mut(&supervisor_id)
                .ok_or_else(|| format!("missing supervisor {}", supervisor_id.as_u64()))?;
            let restart = match supervisor.policy {
                VmRestartPolicy::OneForOne => {
                    let child = supervisor
                        .children
                        .get_mut(child_id)
                        .ok_or_else(|| format!("missing child `{child_id}`"))?;
                    restart_one_for_one_child(processes, child, reason.clone())
                }
                VmRestartPolicy::OneForAll => {
                    if !supervisor.children.contains_key(child_id) {
                        return Err(format!("missing child `{child_id}`"));
                    }
                    restart_one_for_all_children(processes, supervisor, reason.clone())
                }
                VmRestartPolicy::RestForOne => {
                    if !supervisor.children.contains_key(child_id) {
                        return Err(format!("missing child `{child_id}`"));
                    }
                    restart_rest_for_one_children(processes, supervisor, child_id, reason.clone())
                }
            }?;
            record_supervision_restart(supervisor, child_id, &reason, &restart);
            let parent_failure = if let VmSupervisionRestart::LimitReached { pid, .. } = &restart {
                supervisor.state = VmSupervisorState::Failed {
                    child_id: child_id.to_string(),
                    pid: *pid,
                    reason: reason.clone(),
                };
                supervisor
                    .parent_id
                    .map(|parent_id| (parent_id, supervisor_id, reason.clone()))
            } else {
                None
            };
            (restart, parent_failure)
        };
        if let Some((parent_id, failed_supervisor_id, reason)) = parent_failure {
            if let Some(parent) = self.supervisors.get_mut(&parent_id) {
                parent.state = VmSupervisorState::ChildSupervisorFailed {
                    supervisor_id: failed_supervisor_id,
                    reason,
                };
            }
        }
        Ok(restart)
    }

    /// Rebuilds failed child-supervisor subtrees according to their parent's
    /// ordered restart strategy.
    pub(crate) fn restart_failed_supervisor(
        &mut self,
        processes: &mut VmProcessTable,
        failed_supervisor_id: VmSupervisorId,
        reason: VmExitReason,
    ) -> Result<Vec<VmSupervisorId>, String> {
        let failed = self
            .supervisors
            .get(&failed_supervisor_id)
            .ok_or_else(|| format!("missing supervisor {}", failed_supervisor_id.as_u64()))?;
        let parent_id = failed.parent_id.ok_or_else(|| {
            format!(
                "root supervisor {} has no parent restart strategy",
                failed_supervisor_id.as_u64()
            )
        })?;
        let parent = self
            .supervisors
            .get(&parent_id)
            .ok_or_else(|| format!("missing supervisor {}", parent_id.as_u64()))?;
        let siblings = self
            .supervisors
            .values()
            .filter_map(|supervisor| {
                (supervisor.parent_id == Some(parent_id)).then_some(supervisor.id)
            })
            .collect::<Vec<_>>();
        let failed_index = siblings
            .iter()
            .position(|candidate| *candidate == failed_supervisor_id)
            .ok_or_else(|| {
                format!(
                    "supervisor {} is not attached to parent {}",
                    failed_supervisor_id.as_u64(),
                    parent_id.as_u64()
                )
            })?;
        let selected = match parent.policy {
            VmRestartPolicy::OneForOne => vec![failed_supervisor_id],
            VmRestartPolicy::OneForAll => siblings,
            VmRestartPolicy::RestForOne => siblings[failed_index..].to_vec(),
        };
        for supervisor_id in &selected {
            self.restart_supervisor_subtree(processes, *supervisor_id, reason.clone())?;
        }
        if let Some(parent) = self.supervisors.get_mut(&parent_id) {
            parent.state = VmSupervisorState::Running;
        }
        Ok(selected)
    }

    fn restart_supervisor_subtree(
        &mut self,
        processes: &mut VmProcessTable,
        supervisor_id: VmSupervisorId,
        reason: VmExitReason,
    ) -> Result<(), String> {
        let descendants = self
            .supervisors
            .values()
            .filter_map(|supervisor| {
                (supervisor.parent_id == Some(supervisor_id)).then_some(supervisor.id)
            })
            .collect::<Vec<_>>();
        let supervisor = self
            .supervisors
            .get_mut(&supervisor_id)
            .ok_or_else(|| format!("missing supervisor {}", supervisor_id.as_u64()))?;
        let child_ids = supervisor.child_order.clone();
        if !child_ids.is_empty() {
            let mut restarted = Vec::with_capacity(child_ids.len());
            for child_id in child_ids {
                let child = supervisor
                    .children
                    .get_mut(&child_id)
                    .expect("supervisor child order references a live child spec");
                let old_pid = child.pid;
                if !matches!(
                    processes.get(old_pid).map(|process| &process.state),
                    Some(VmProcessState::Exited(_))
                ) {
                    processes.exit_process(old_pid, reason.clone())?;
                }
                let new_pid = processes.spawn_root(child.spec.source.clone());
                child.pid = new_pid;
                child.restart_count = 0;
                child.last_restart_delay_ms = 0;
                child.last_shutdown_timeout_ms = None;
                restarted.push(VmSupervisionRestartEvent {
                    child_id,
                    old_pid,
                    new_pid,
                    restart_count: 0,
                    restart_delay_ms: 0,
                    shutdown_timeout_ms: None,
                });
            }
            let restart = VmSupervisionRestart::RestartedGroup { restarted };
            record_supervision_restart(supervisor, "<supervisor>", &reason, &restart);
        }
        supervisor.state = VmSupervisorState::Running;
        for descendant in descendants {
            self.restart_supervisor_subtree(processes, descendant, reason.clone())?;
        }
        Ok(())
    }

    /// Restarts one child whose supervisor policy was already applied before
    /// a VM-owned backoff deadline was installed.
    fn restart_child_after_backoff(
        &mut self,
        processes: &mut VmProcessTable,
        supervisor_id: VmSupervisorId,
        child_id: &str,
        reason: VmExitReason,
    ) -> Result<VmSupervisionRestart, String> {
        let supervisor = self
            .supervisors
            .get_mut(&supervisor_id)
            .ok_or_else(|| format!("missing supervisor {}", supervisor_id.as_u64()))?;
        let restart = {
            let child = supervisor
                .children
                .get_mut(child_id)
                .ok_or_else(|| format!("missing child `{child_id}`"))?;
            restart_one_for_one_child(processes, child, reason.clone())?
        };
        record_supervision_restart(supervisor, child_id, &reason, &restart);
        Ok(restart)
    }

    /// Applies collection, cleanup, restart, and escalation policy for memory pressure.
    pub(crate) fn handle_memory_pressure(
        &mut self,
        memory: &mut VmMemoryAccountant,
        resources: &mut VmResourceTable,
        processes: &mut VmProcessTable,
        supervisor_id: VmSupervisorId,
        child_id: &str,
        pressure: &VmMemoryPressureDecision,
    ) -> Result<VmSupervisionMemoryPressure, String> {
        let supervisor = self
            .supervisors
            .get(&supervisor_id)
            .ok_or_else(|| format!("missing supervisor {}", supervisor_id.as_u64()))?;
        let child = supervisor
            .children
            .get(child_id)
            .ok_or_else(|| format!("missing child `{child_id}`"))?;
        if pressure.pid != child.pid.as_u64() {
            return Err(format!(
                "memory pressure process {} does not match supervised child `{child_id}` process {}",
                pressure.pid,
                child.pid.as_u64()
            ));
        }
        match pressure.outcome {
            VmMemoryPressureOutcome::Accounted => {
                return Ok(VmSupervisionMemoryPressure::Continue { pid: child.pid });
            }
            VmMemoryPressureOutcome::SoftLimitExceeded => {
                return Ok(VmSupervisionMemoryPressure::Collect {
                    pid: child.pid,
                    projected_bytes: pressure.projected_bytes,
                });
            }
            VmMemoryPressureOutcome::HardLimitRejected => {}
        }

        let reason = VmExitReason::MemoryLimitExceeded {
            requested_bytes: pressure.requested_bytes,
            previous_bytes: pressure.previous_bytes,
            projected_bytes: pressure.projected_bytes,
        };
        let cleanup_pids = memory_pressure_cleanup_pids(supervisor, child_id, &reason);
        for pid in cleanup_pids {
            memory.exit_process_with_memory_cleanup(processes, resources, pid, reason.clone())?;
        }
        let restart = self.restart_child(processes, supervisor_id, child_id, reason)?;
        Ok(VmSupervisionMemoryPressure::Restart(restart))
    }

    /// Returns an inspection-visible supervisor tree.
    pub(crate) fn snapshot(
        &self,
        supervisor_id: VmSupervisorId,
    ) -> Result<VmSupervisorSnapshot, String> {
        let supervisor = self
            .supervisors
            .get(&supervisor_id)
            .ok_or_else(|| format!("missing supervisor {}", supervisor_id.as_u64()))?;
        Ok(VmSupervisorSnapshot {
            id: supervisor.id,
            parent_id: supervisor.parent_id,
            name: supervisor.name.clone(),
            policy: supervisor.policy.clone(),
            state: supervisor.state.clone(),
            children: supervisor
                .children
                .values()
                .map(|child| VmSupervisorChildSnapshot {
                    child_id: child.spec.id.clone(),
                    pid: child.pid,
                    source: child.spec.source.clone(),
                    restart_count: child.restart_count,
                    restart_limit: child.spec.restart_limit,
                    restart_class: child.spec.restart_class.clone(),
                    last_restart_delay_ms: child.last_restart_delay_ms,
                    shutdown_timeout_ms: shutdown_timeout_ms(&child.spec),
                    last_shutdown_timeout_ms: child.last_shutdown_timeout_ms,
                })
                .collect(),
            restart_history: supervisor.restart_history.clone(),
        })
    }
}

fn memory_pressure_cleanup_pids(
    supervisor: &VmSupervisor,
    child_id: &str,
    reason: &VmExitReason,
) -> Vec<VmProcessId> {
    let selected_ids = match supervisor.policy {
        VmRestartPolicy::OneForOne => vec![child_id.to_string()],
        VmRestartPolicy::OneForAll => supervisor.child_order.clone(),
        VmRestartPolicy::RestForOne => {
            let start = supervisor
                .child_order
                .iter()
                .position(|known| known == child_id)
                .expect("memory pressure child was validated before cleanup selection");
            supervisor.child_order[start..].to_vec()
        }
    };
    let restart_limited = selected_ids.iter().any(|selected_id| {
        let child = supervisor
            .children
            .get(selected_id)
            .expect("selected memory pressure child belongs to supervisor");
        child.spec.restart_class.should_restart(reason)
            && child.restart_count >= child.spec.restart_limit
    });
    if restart_limited {
        return vec![
            supervisor
                .children
                .get(child_id)
                .expect("memory pressure child was validated")
                .pid,
        ];
    }
    selected_ids
        .iter()
        .map(|selected_id| {
            supervisor
                .children
                .get(selected_id)
                .expect("selected memory pressure child belongs to supervisor")
                .pid
        })
        .collect()
}

fn record_supervision_restart(
    supervisor: &mut VmSupervisor,
    child_id: &str,
    reason: &VmExitReason,
    restart: &VmSupervisionRestart,
) {
    match restart {
        VmSupervisionRestart::Restarted {
            old_pid,
            new_pid,
            restart_count,
            restart_delay_ms,
            shutdown_timeout_ms,
        } => supervisor
            .restart_history
            .push(VmSupervisorRestartHistoryEntry {
                child_id: child_id.to_string(),
                old_pid: *old_pid,
                new_pid: Some(*new_pid),
                restart_count: *restart_count,
                reason: reason.clone(),
                outcome: VmSupervisorRestartHistoryOutcome::Restarted,
                restart_delay_ms: *restart_delay_ms,
                shutdown_timeout_ms: *shutdown_timeout_ms,
            }),
        VmSupervisionRestart::RestartedGroup { restarted } => {
            for event in restarted {
                supervisor
                    .restart_history
                    .push(VmSupervisorRestartHistoryEntry {
                        child_id: event.child_id.clone(),
                        old_pid: event.old_pid,
                        new_pid: Some(event.new_pid),
                        restart_count: event.restart_count,
                        reason: reason.clone(),
                        outcome: VmSupervisorRestartHistoryOutcome::Restarted,
                        restart_delay_ms: event.restart_delay_ms,
                        shutdown_timeout_ms: event.shutdown_timeout_ms,
                    });
            }
        }
        VmSupervisionRestart::NotRestarted { pid, .. } => {
            let restart_count = supervisor
                .children
                .get(child_id)
                .map(|child| child.restart_count)
                .unwrap_or(0);
            supervisor
                .restart_history
                .push(VmSupervisorRestartHistoryEntry {
                    child_id: child_id.to_string(),
                    old_pid: *pid,
                    new_pid: None,
                    restart_count,
                    reason: reason.clone(),
                    outcome: VmSupervisorRestartHistoryOutcome::NotRestarted,
                    restart_delay_ms: 0,
                    shutdown_timeout_ms: supervisor
                        .children
                        .get(child_id)
                        .and_then(|child| child.last_shutdown_timeout_ms),
                });
        }
        VmSupervisionRestart::LimitReached { pid, restart_count } => supervisor
            .restart_history
            .push(VmSupervisorRestartHistoryEntry {
                child_id: child_id.to_string(),
                old_pid: *pid,
                new_pid: None,
                restart_count: *restart_count,
                reason: reason.clone(),
                outcome: VmSupervisorRestartHistoryOutcome::LimitReached,
                restart_delay_ms: 0,
                shutdown_timeout_ms: supervisor
                    .children
                    .get(child_id)
                    .and_then(|child| child.last_shutdown_timeout_ms),
            }),
    }
}

fn restart_one_for_one_child(
    processes: &mut VmProcessTable,
    child: &mut VmSupervisorChild,
    reason: VmExitReason,
) -> Result<VmSupervisionRestart, String> {
    let old_pid = child.pid;
    let shutdown_timeout_ms = shutdown_timeout_ms(&child.spec);
    if !matches!(
        processes.get(old_pid).map(|process| &process.state),
        Some(VmProcessState::Exited(_))
    ) {
        processes.exit_process(old_pid, reason.clone())?;
    }
    child.last_shutdown_timeout_ms = shutdown_timeout_ms;
    if !child.spec.restart_class.should_restart(&reason) {
        return Ok(VmSupervisionRestart::NotRestarted {
            pid: old_pid,
            restart_class: child.spec.restart_class.clone(),
            reason,
        });
    }
    if child.restart_count >= child.spec.restart_limit {
        return Ok(VmSupervisionRestart::LimitReached {
            pid: old_pid,
            restart_count: child.restart_count,
        });
    }
    child.restart_count += 1;
    child.last_restart_delay_ms = restart_delay_ms(&child.spec, child.restart_count);
    let new_pid = processes.spawn_root(child.spec.source.clone());
    child.pid = new_pid;
    Ok(VmSupervisionRestart::Restarted {
        old_pid,
        new_pid,
        restart_count: child.restart_count,
        restart_delay_ms: child.last_restart_delay_ms,
        shutdown_timeout_ms: child.last_shutdown_timeout_ms,
    })
}

fn restart_one_for_all_children(
    processes: &mut VmProcessTable,
    supervisor: &mut VmSupervisor,
    reason: VmExitReason,
) -> Result<VmSupervisionRestart, String> {
    let child_ids = supervisor.child_order.clone();
    restart_child_group(processes, supervisor, child_ids, reason)
}

fn restart_rest_for_one_children(
    processes: &mut VmProcessTable,
    supervisor: &mut VmSupervisor,
    child_id: &str,
    reason: VmExitReason,
) -> Result<VmSupervisionRestart, String> {
    let start_index = supervisor
        .child_order
        .iter()
        .position(|known_child_id| known_child_id == child_id)
        .expect("child was checked before rest-for-one restart");
    let child_ids = supervisor.child_order[start_index..].to_vec();
    restart_child_group(processes, supervisor, child_ids, reason)
}

fn restart_child_group(
    processes: &mut VmProcessTable,
    supervisor: &mut VmSupervisor,
    child_ids: Vec<String>,
    reason: VmExitReason,
) -> Result<VmSupervisionRestart, String> {
    for child_id in &child_ids {
        let child = supervisor
            .children
            .get(child_id)
            .expect("group restart child id came from supervisor child order");
        if child.spec.restart_class.should_restart(&reason)
            && child.restart_count >= child.spec.restart_limit
        {
            return Ok(VmSupervisionRestart::LimitReached {
                pid: child.pid,
                restart_count: child.restart_count,
            });
        }
    }
    let mut restarted = Vec::with_capacity(child_ids.len());
    for child_id in child_ids {
        let child = supervisor
            .children
            .get_mut(&child_id)
            .expect("child id came from supervisor children");
        let old_pid = child.pid;
        let shutdown_timeout_ms = shutdown_timeout_ms(&child.spec);
        if !matches!(
            processes.get(old_pid).map(|process| &process.state),
            Some(VmProcessState::Exited(_))
        ) {
            processes.exit_process(old_pid, reason.clone())?;
        }
        child.last_shutdown_timeout_ms = shutdown_timeout_ms;
        if !child.spec.restart_class.should_restart(&reason) {
            continue;
        }
        child.restart_count += 1;
        child.last_restart_delay_ms = restart_delay_ms(&child.spec, child.restart_count);
        let new_pid = processes.spawn_root(child.spec.source.clone());
        child.pid = new_pid;
        restarted.push(VmSupervisionRestartEvent {
            child_id,
            old_pid,
            new_pid,
            restart_count: child.restart_count,
            restart_delay_ms: child.last_restart_delay_ms,
            shutdown_timeout_ms: child.last_shutdown_timeout_ms,
        });
    }
    Ok(VmSupervisionRestart::RestartedGroup { restarted })
}

fn restart_delay_ms(spec: &VmChildSpec, restart_count: u32) -> u64 {
    spec.restart_backoff
        .as_ref()
        .map(|schedule| schedule.delay_for_restart_count(restart_count))
        .unwrap_or(0)
}

fn shutdown_timeout_ms(spec: &VmChildSpec) -> Option<u64> {
    spec.shutdown_timeout
        .as_ref()
        .map(|timeout| timeout.timeout_ms)
}

#[cfg(test)]
#[path = "supervision_test.rs"]
#[cfg(test)]
mod supervision_test;

#[cfg(test)]
#[path = "supervision/memory_pressure_test.rs"]
#[cfg(test)]
mod memory_pressure_test;
