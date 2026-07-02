#![allow(dead_code)]

use std::collections::BTreeMap;

use super::process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState, VmProcessTable};

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
}

/// Child process specification owned by a supervisor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmChildSpec {
    pub(crate) id: String,
    pub(crate) source: VmProcessSource,
    pub(crate) restart_limit: u32,
}

impl VmChildSpec {
    /// Creates a restartable child specification.
    pub(crate) fn new(id: impl Into<String>, source: VmProcessSource, restart_limit: u32) -> Self {
        Self {
            id: id.into(),
            source,
            restart_limit,
        }
    }
}

/// Restart result emitted by a supervisor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmSupervisionRestart {
    Restarted {
        old_pid: VmProcessId,
        new_pid: VmProcessId,
        restart_count: u32,
    },
    LimitReached {
        pid: VmProcessId,
        restart_count: u32,
    },
}

/// Read-only child row for runtime inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmSupervisorChildSnapshot {
    pub(crate) child_id: String,
    pub(crate) pid: VmProcessId,
    pub(crate) source: VmProcessSource,
    pub(crate) restart_count: u32,
    pub(crate) restart_limit: u32,
}

/// Read-only supervisor tree for runtime inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmSupervisorSnapshot {
    pub(crate) id: VmSupervisorId,
    pub(crate) name: String,
    pub(crate) policy: VmRestartPolicy,
    pub(crate) children: Vec<VmSupervisorChildSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VmSupervisorChild {
    spec: VmChildSpec,
    pid: VmProcessId,
    restart_count: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VmSupervisor {
    id: VmSupervisorId,
    name: String,
    policy: VmRestartPolicy,
    children: BTreeMap<String, VmSupervisorChild>,
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
        self.next_supervisor_id = self.next_supervisor_id.saturating_add(1);
        let id = VmSupervisorId(self.next_supervisor_id);
        self.supervisors.insert(
            id,
            VmSupervisor {
                id,
                name: name.into(),
                policy: VmRestartPolicy::OneForOne,
                children: BTreeMap::new(),
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
        supervisor.children.insert(
            spec.id.clone(),
            VmSupervisorChild {
                spec,
                pid,
                restart_count: 0,
            },
        );
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
        let supervisor = self
            .supervisors
            .get_mut(&supervisor_id)
            .ok_or_else(|| format!("missing supervisor {}", supervisor_id.as_u64()))?;
        let child = supervisor
            .children
            .get_mut(child_id)
            .ok_or_else(|| format!("missing child `{child_id}`"))?;
        let old_pid = child.pid;
        if !matches!(
            processes.get(old_pid).map(|process| &process.state),
            Some(VmProcessState::Exited(_))
        ) {
            processes
                .exit_process(old_pid, reason)
                .expect("supervised child must remain exitable before restart");
        }
        if child.restart_count >= child.spec.restart_limit {
            return Ok(VmSupervisionRestart::LimitReached {
                pid: old_pid,
                restart_count: child.restart_count,
            });
        }
        child.restart_count += 1;
        let new_pid = processes.spawn_root(child.spec.source.clone());
        child.pid = new_pid;
        Ok(VmSupervisionRestart::Restarted {
            old_pid,
            new_pid,
            restart_count: child.restart_count,
        })
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
            name: supervisor.name.clone(),
            policy: supervisor.policy.clone(),
            children: supervisor
                .children
                .values()
                .map(|child| VmSupervisorChildSnapshot {
                    child_id: child.spec.id.clone(),
                    pid: child.pid,
                    source: child.spec.source.clone(),
                    restart_count: child.restart_count,
                    restart_limit: child.spec.restart_limit,
                })
                .collect(),
        })
    }
}

#[cfg(test)]
#[path = "supervision_test.rs"]
mod supervision_test;
