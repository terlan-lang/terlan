//! Source admission shares the real actor process table and VM restart queues.

use std::collections::{BTreeMap, VecDeque};

use super::backoff::{VmSupervisionBackoffQueue, VmSupervisionRestartRequest};
use super::VmSupervisionErrorKind::{ChildLifecycle, Configuration, Inspection, Restart, Shutdown};
use super::*;
use crate::runtime::vm::timer::{VmTimerEvent, VmTimerTable};
use crate::runtime::vm::ReplValue;

mod persistence;

#[derive(Debug)]
struct SourceRoot {
    owner: VmProcessId,
    policy: ReplValue,
    limit: usize,
    window: u64,
    restarts: VecDeque<u64>,
}

#[derive(Debug)]
struct SourceChild {
    entry: i64,
    restart_entry: i64,
    order: i64,
    stopped: bool,
    handled: Option<VmProcessId>,
    pending: bool,
    recovery: Option<ReplValue>,
}

/// One native child entry ready for execution on the owning shard.
#[derive(Debug)]
pub(crate) struct SourceChildStart {
    pub(crate) pid: VmProcessId,
    pub(crate) entry: i64,
}

/// Owner-local graph, policy authority, and timer intents for source children.
#[derive(Debug, Default)]
pub(crate) struct VmSourceSupervision {
    system: VmSupervisionSystem,
    roots: BTreeMap<VmSupervisorId, SourceRoot>,
    names: BTreeMap<(VmProcessId, i64), VmSupervisorId>,
    children: BTreeMap<(VmSupervisorId, String), SourceChild>,
    backoff: VmSupervisionBackoffQueue,
    ready: VecDeque<SourceChildStart>,
    exits: VecDeque<(VmProcessId, VmExitReason)>,
    retired: VecDeque<VmProcessId>,
    failure: Option<VmSupervisionError>,
    shutdowns: BTreeMap<crate::runtime::vm::timer::VmTimerId, (VmProcessId, u64)>,
    terminations: VecDeque<(VmProcessId, VmExitReason)>,
}

impl VmSourceSupervision {
    /// Revokes every child and pending restart when its supervising owner exits.
    pub(crate) fn owner_exited(
        &mut self,
        owner: VmProcessId,
        processes: &mut VmProcessTable,
        timers: &mut VmTimerTable,
    ) -> Result<Vec<VmProcessId>, VmSupervisionError> {
        let roots = self
            .roots
            .iter()
            .filter_map(|(id, root)| (root.owner == owner).then_some(*id))
            .collect::<Vec<_>>();
        let mut cleanup = Vec::new();
        for id in roots {
            let node = self.system.supervisors.get_mut(&id).expect("known root");
            node.state = VmSupervisorState::Failed {
                child_id: "owner".into(),
                pid: owner,
                reason: VmExitReason::Killed,
            };
            let children = node.children.keys().cloned().collect::<Vec<_>>();
            for child_id in children {
                let key = (id, child_id.clone());
                self.children.get_mut(&key).expect("known child").stopped = true;
                self.backoff
                    .cancel_child(&mut self.system, timers, processes, id, &child_id)
                    .map_err(|message| VmSupervisionError::new(Shutdown, message))?;
                let child = self
                    .system
                    .supervisors
                    .get_mut(&id)
                    .expect("known root")
                    .children
                    .get_mut(&child_id)
                    .expect("known child");
                child.spec.restart_class = VmChildRestartClass::Temporary;
                if processes.is_alive(child.pid) {
                    cleanup.push(child.pid);
                }
            }
        }
        if let Some(timer_owner) = self.backoff.take_idle_timer_owner() {
            cleanup.push(timer_owner);
        }
        Ok(cleanup)
    }

    pub(crate) fn record_exit(&mut self, pid: VmProcessId, reason: VmExitReason) {
        self.exits.push_back((pid, reason));
        self.retired.push_back(pid);
    }

    pub(crate) fn owns_process(&self, pid: VmProcessId) -> bool {
        self.roots.values().any(|root| root.owner == pid)
            || self
                .system
                .supervisors
                .values()
                .any(|root| root.children.values().any(|child| child.pid == pid))
    }

    /// Admits a source declaration only after both executable entries resolve.
    pub(crate) fn start(
        &mut self,
        processes: &mut VmProcessTable,
        owner: VmProcessId,
        policy: &ReplValue,
        spec: &ReplValue,
        recovery: Option<&ReplValue>,
        source: VmProcessSource,
    ) -> Result<VmProcessId, VmSupervisionError> {
        let root = self.root(owner, policy)?;
        if !matches!(
            self.system.supervisors[&root].state,
            VmSupervisorState::Running
        ) {
            return Err(VmSupervisionError::new(
                ChildLifecycle,
                "supervisor has exhausted its restart budget",
            ));
        }
        let id = int(spec, "id", 1)?.to_string();
        let entry = int(spec, "value", 2)?;
        let order = int(spec, "order", 4)?;
        let initial = nonnegative(spec, "initial_backoff", 5)?;
        let maximum = nonnegative(spec, "maximum_backoff", 6)?;
        let timeout = nonnegative(spec, "shutdown_timeout", 7)?;
        if initial > maximum {
            return Err(VmSupervisionError::new(
                Configuration,
                "initial backoff exceeds maximum backoff",
            ));
        }
        let class = match atom(field(spec, "restart_class", 3)?)? {
            "permanent" => VmChildRestartClass::Permanent,
            "transient" => VmChildRestartClass::Transient,
            "temporary" => VmChildRestartClass::Temporary,
            _ => {
                return Err(VmSupervisionError::new(
                    Configuration,
                    "unknown restart class",
                ))
            }
        };
        let restart_entry = match recovery {
            Some(recovery) => {
                persistence::validate(recovery)?;
                int(recovery, "restore_entry", 4)?
            }
            None => entry,
        };
        let mut child = VmChildSpec::new(&id, source, u32::MAX)
            .with_restart_class(class)
            .with_shutdown_timeout(VmShutdownTimeout::milliseconds(timeout));
        if initial > 0 {
            child =
                child.with_restart_backoff(VmRestartBackoffSchedule::exponential(initial, maximum));
        }
        let pid = self
            .system
            .start_child(processes, root, child)
            .map_err(|message| VmSupervisionError::new(ChildLifecycle, message))?;
        self.children.insert(
            (root, id.clone()),
            SourceChild {
                entry,
                restart_entry,
                order,
                stopped: false,
                handled: None,
                pending: false,
                recovery: recovery.cloned(),
            },
        );
        let node = self
            .system
            .supervisors
            .get_mut(&root)
            .expect("admitted root");
        node.child_order
            .sort_by_key(|id| (self.children[&(root, id.clone())].order, id.clone()));
        self.ready.push_back(SourceChildStart { pid, entry });
        Ok(pid)
    }

    fn root(
        &mut self,
        owner: VmProcessId,
        policy: &ReplValue,
    ) -> Result<VmSupervisorId, VmSupervisionError> {
        let name = int(policy, "name", 1)?;
        if let Some(id) = self.names.get(&(owner, name)) {
            return if self.roots[id].policy == *policy {
                Ok(*id)
            } else {
                Err(VmSupervisionError::new(
                    Configuration,
                    "supervisor name already has a different policy",
                ))
            };
        }
        let limit = nonnegative(policy, "restart_limit", 3)?;
        let window = nonnegative(policy, "restart_window", 4)?;
        if limit > 100_000 || window == 0 {
            return Err(VmSupervisionError::new(
                Configuration,
                "restart limit must be at most 100000 and window must be positive",
            ));
        }
        let strategy = match atom(field(policy, "strategy", 2)?)? {
            "one_for_one" => VmRestartPolicy::OneForOne,
            "one_for_all" => VmRestartPolicy::OneForAll,
            "rest_for_one" => VmRestartPolicy::RestForOne,
            _ => {
                return Err(VmSupervisionError::new(
                    Configuration,
                    "unknown restart strategy",
                ))
            }
        };
        let id = self
            .system
            .create_supervisor_with_policy(name.to_string(), strategy);
        self.roots.insert(
            id,
            SourceRoot {
                owner,
                policy: policy.clone(),
                limit: limit as usize,
                window,
                restarts: VecDeque::new(),
            },
        );
        self.names.insert((owner, name), id);
        Ok(id)
    }

    fn lookup(
        &self,
        owner: VmProcessId,
        policy: &ReplValue,
    ) -> Result<VmSupervisorId, VmSupervisionError> {
        let name = int(policy, "name", 1)?;
        let id = *self.names.get(&(owner, name)).ok_or_else(|| {
            VmSupervisionError::new(Configuration, "supervisor has no admitted children")
        })?;
        if self.roots[&id].policy != *policy {
            return Err(VmSupervisionError::new(
                Configuration,
                "supervisor policy does not match admission",
            ));
        }
        Ok(id)
    }

    fn child_key(
        &self,
        owner: VmProcessId,
        policy: &ReplValue,
        spec: &ReplValue,
    ) -> Result<(VmSupervisorId, String), VmSupervisionError> {
        let key = (self.lookup(owner, policy)?, int(spec, "id", 1)?.to_string());
        let child = self.children.get(&key).ok_or_else(|| {
            VmSupervisionError::new(ChildLifecycle, "child declaration is not registered")
        })?;
        if child.entry != int(spec, "value", 2)? {
            return Err(VmSupervisionError::new(
                ChildLifecycle,
                "child entry does not match admission",
            ));
        }
        Ok(key)
    }

    pub(crate) fn current(
        &self,
        owner: VmProcessId,
        policy: &ReplValue,
        spec: &ReplValue,
        processes: &VmProcessTable,
    ) -> Result<VmProcessId, VmSupervisionError> {
        let key = self.child_key(owner, policy, spec)?;
        let child = &self.children[&key];
        let pid = self.system.supervisors[&key.0].children[&key.1].pid;
        if child.stopped
            || child.pending
            || matches!(
                processes.get(pid).map(|process| &process.state),
                Some(VmProcessState::Exited(_)) | None
            )
        {
            return Err(VmSupervisionError::new(
                ChildLifecycle,
                "child has no running process",
            ));
        }
        Ok(pid)
    }

    /// Suppresses restart before requesting cancellation or its shutdown deadline.
    pub(crate) fn stop(
        &mut self,
        owner: VmProcessId,
        policy: &ReplValue,
        spec: &ReplValue,
        processes: &mut VmProcessTable,
        timers: &mut VmTimerTable,
        scheduler: &mut crate::runtime::vm::scheduler::VmScheduler,
    ) -> Result<Option<VmProcessId>, VmSupervisionError> {
        let key = self.child_key(owner, policy, spec)?;
        self.children
            .get_mut(&key)
            .expect("validated child")
            .stopped = true;
        self.backoff
            .cancel_child(&mut self.system, timers, processes, key.0, &key.1)
            .map_err(|message| VmSupervisionError::new(Shutdown, message))?;
        self.system
            .supervisors
            .get_mut(&key.0)
            .expect("known root")
            .children
            .get_mut(&key.1)
            .expect("known child")
            .spec
            .restart_class = VmChildRestartClass::Temporary;
        self.system
            .supervisors
            .get_mut(&key.0)
            .expect("known root")
            .child_order
            .retain(|id| id != &key.1);
        let child = &self.system.supervisors[&key.0].children[&key.1];
        let pid = child.pid;
        let timeout = child
            .spec
            .shutdown_timeout
            .as_ref()
            .map_or(0, |timeout| timeout.timeout_ms);
        if processes.get(pid).is_none() {
            return Ok(None);
        }
        if timeout == 0
            || matches!(
                processes.get(pid).map(|process| &process.state),
                Some(VmProcessState::Exited(_))
            )
        {
            return Ok(Some(pid));
        }
        let deadline = timers
            .current_tick()
            .checked_add(timeout)
            .ok_or_else(|| VmSupervisionError::new(Configuration, "shutdown deadline overflow"))?;
        let timer = timers
            .start_one_shot(processes, owner, deadline)
            .map_err(|message| VmSupervisionError::new(Shutdown, message))?;
        self.shutdowns.insert(timer, (pid, timeout));
        // Cancellation is acknowledged at the next native safepoint. A child
        // which cannot reach one is terminated at the recorded deadline.
        scheduler
            .request_cancellation(processes, pid)
            .map_err(|message| VmSupervisionError::new(Shutdown, message))?;
        Ok(None)
    }

    /// Selects and cleans sibling actors before VM restart allocation.
    pub(crate) fn next_exit(
        &mut self,
        now: u64,
    ) -> Option<(VmSupervisorId, String, VmExitReason, Vec<VmProcessId>)> {
        while let Some((pid, reason)) = self.exits.pop_front() {
            if let Some(root) = self.roots.iter().find_map(|(id, root)| {
                (root.owner == pid
                    && matches!(
                        self.system.supervisors[id].state,
                        VmSupervisorState::Running
                    ))
                .then_some(*id)
            }) {
                let node = self.system.supervisors.get_mut(&root).expect("known root");
                node.state = VmSupervisorState::Failed {
                    child_id: "owner".into(),
                    pid,
                    reason: reason.clone(),
                };
                for ((id, _), child) in &mut self.children {
                    if *id == root {
                        child.stopped = true;
                    }
                }
                let cleanup = node.children.values().map(|child| child.pid).collect();
                self.exits.push_front((pid, reason.clone()));
                return Some((root, "owner".into(), reason, cleanup));
            }
            let key = self.children.iter().find_map(|(key, child)| {
                let actual = self.system.supervisors[&key.0].children[&key.1].pid;
                (actual == pid && child.handled != Some(pid) && !child.stopped && !child.pending)
                    .then(|| key.clone())
            });
            let Some(key) = key else { continue };
            let node = &self.system.supervisors[&key.0];
            if !matches!(node.state, VmSupervisorState::Running) {
                continue;
            }
            let class = node.children[&key.1].spec.restart_class.clone();
            if !class.should_restart(&reason) {
                self.children.get_mut(&key).expect("known child").handled = Some(pid);
                let node = self.system.supervisors.get_mut(&key.0).expect("known root");
                let event = VmSupervisionRestart::NotRestarted {
                    pid,
                    restart_class: class.clone(),
                    reason: reason.clone(),
                };
                record_supervision_restart(node, &key.1, &reason, &event);
                continue;
            }
            let selected = match node.policy {
                VmRestartPolicy::OneForOne => vec![key.1.clone()],
                VmRestartPolicy::OneForAll => node.child_order.clone(),
                VmRestartPolicy::RestForOne => node
                    .child_order
                    .iter()
                    .skip_while(|id| *id != &key.1)
                    .cloned()
                    .collect(),
            };
            let selected = selected
                .into_iter()
                .filter(|id| !self.children[&(key.0, id.clone())].stopped)
                .collect::<Vec<_>>();
            let root = self.roots.get_mut(&key.0).expect("known root");
            while root
                .restarts
                .front()
                .is_some_and(|tick| now.saturating_sub(*tick) >= root.window)
            {
                root.restarts.pop_front();
            }
            if root.restarts.len() >= root.limit {
                let node = self.system.supervisors.get_mut(&key.0).expect("known root");
                node.state = VmSupervisorState::Failed {
                    child_id: key.1.clone(),
                    pid,
                    reason: reason.clone(),
                };
                let event = VmSupervisionRestart::LimitReached {
                    pid,
                    restart_count: root.restarts.len() as u32,
                };
                record_supervision_restart(node, &key.1, &reason, &event);
                let cleanup = node.children.values().map(|child| child.pid).collect();
                return Some((key.0, key.1, reason, cleanup));
            }
            root.restarts.push_back(now);
            let cleanup = selected.iter().map(|id| node.children[id].pid).collect();
            for id in selected {
                let child = self
                    .children
                    .get_mut(&(key.0, id.clone()))
                    .expect("selected child");
                child.handled = Some(node.children[&id].pid);
                child.pending = node.children[&id]
                    .spec
                    .restart_class
                    .should_restart(&reason);
            }
            return Some((key.0, key.1, reason, cleanup));
        }
        None
    }

    pub(crate) fn restart(
        &mut self,
        processes: &mut VmProcessTable,
        timers: &mut VmTimerTable,
        root: VmSupervisorId,
        id: &str,
        reason: VmExitReason,
    ) -> Result<(), VmSupervisionError> {
        if !matches!(
            self.system.supervisors[&root].state,
            VmSupervisorState::Running
        ) {
            let children = self.system.supervisors[&root].child_order.clone();
            for id in children {
                self.backoff
                    .cancel_child(&mut self.system, timers, processes, root, &id)
                    .map_err(|message| VmSupervisionError::new(Restart, message))?;
                let child = self
                    .children
                    .get_mut(&(root, id.clone()))
                    .expect("known child");
                child.stopped = true;
                child.pending = false;
                self.system
                    .supervisors
                    .get_mut(&root)
                    .expect("known root")
                    .children
                    .get_mut(&id)
                    .expect("known child")
                    .spec
                    .restart_class = VmChildRestartClass::Temporary;
            }
            return Ok(());
        }
        let now = timers.current_tick();
        self.backoff
            .schedule_restart(
                &mut self.system,
                timers,
                processes,
                VmSupervisionRestartRequest::new(root, id, reason, now),
            )
            .map_err(|message| VmSupervisionError::new(Restart, message))?;
        self.collect_replacements();
        Ok(())
    }

    fn collect_replacements(&mut self) {
        for (key, child) in &mut self.children {
            let pid = self.system.supervisors[&key.0].children[&key.1].pid;
            if child.pending && child.handled != Some(pid) {
                child.pending = false;
                self.ready.push_back(SourceChildStart {
                    pid,
                    entry: child.restart_entry,
                });
            }
        }
    }

    pub(crate) fn timer_event(&mut self, processes: &mut VmProcessTable, event: &VmTimerEvent) {
        if let Some((pid, timeout_ms)) = self.shutdowns.remove(&event.timer_id()) {
            if matches!(
                event,
                VmTimerEvent::Fired { .. } | VmTimerEvent::DeadlineMissed { .. }
            ) {
                self.terminations
                    .push_back((pid, VmExitReason::ShutdownTimeout { timeout_ms }));
            }
        }
        if let Err(error) = self
            .backoff
            .handle_timer_event(&mut self.system, processes, event)
        {
            self.failure = Some(VmSupervisionError::new(Restart, error));
        }
        self.collect_replacements();
    }

    pub(crate) fn take_ready(&mut self) -> Result<Option<SourceChildStart>, VmSupervisionError> {
        if let Some(error) = self.failure.take() {
            return Err(error);
        }
        while let Some(start) = self.ready.pop_front() {
            if self.children.iter().any(|(key, child)| {
                !child.stopped
                    && self.system.supervisors[&key.0].children[&key.1].pid == start.pid
                    && matches!(
                        self.system.supervisors[&key.0].state,
                        VmSupervisorState::Running
                    )
            }) {
                return Ok(Some(start));
            }
        }
        Ok(None)
    }

    pub(crate) fn take_termination(&mut self) -> Option<(VmProcessId, VmExitReason)> {
        self.terminations.pop_front().or_else(|| {
            self.backoff
                .take_idle_timer_owner()
                .map(|owner| (owner, VmExitReason::Normal))
        })
    }

    pub(crate) fn take_retired(&mut self) -> Option<VmProcessId> {
        self.retired.pop_front()
    }

    pub(crate) fn snapshot(
        &self,
        owner: VmProcessId,
        policy: &ReplValue,
        processes: &VmProcessTable,
    ) -> Result<ReplValue, VmSupervisionError> {
        let root = self.lookup(owner, policy)?;
        let snapshot = self
            .system
            .snapshot(root)
            .map_err(|message| VmSupervisionError::new(Inspection, message))?;
        let children =
            snapshot
                .children
                .iter()
                .map(|child| {
                    let meta = &self.children[&(root, child.child_id.clone())];
                    record(
                        "ChildSnapshot",
                        vec![
                            (
                                "id",
                                ReplValue::Int(child.child_id.parse().expect("numeric child id")),
                            ),
                            ("process_id", ReplValue::Int(child.pid.as_u64() as i64)),
                            ("restart_count", ReplValue::Int(child.restart_count.into())),
                            (
                                "running",
                                ReplValue::Bool(processes.get(child.pid).is_some_and(|p| {
                                    !matches!(p.state, VmProcessState::Exited(_))
                                })),
                            ),
                            (
                                "pending_restart",
                                ReplValue::Bool(meta.pending && !meta.stopped),
                            ),
                            (
                                "last_backoff",
                                ReplValue::Int(child.last_restart_delay_ms as i64),
                            ),
                            (
                                "shutdown_timeout",
                                ReplValue::Int(child.shutdown_timeout_ms.unwrap_or(0) as i64),
                            ),
                        ],
                    )
                })
                .collect();
        let history = snapshot
            .restart_history
            .iter()
            .map(|event| {
                record(
                    "RestartEvent",
                    vec![
                        (
                            "child_id",
                            ReplValue::Int(event.child_id.parse().expect("numeric child id")),
                        ),
                        (
                            "old_process_id",
                            ReplValue::Int(event.old_pid.as_u64() as i64),
                        ),
                        (
                            "new_process_id",
                            ReplValue::Int(event.new_pid.map_or(0, |pid| pid.as_u64()) as i64),
                        ),
                        ("restart_count", ReplValue::Int(event.restart_count.into())),
                        ("reason", ReplValue::String(format!("{:?}", event.reason))),
                        ("outcome", ReplValue::String(format!("{:?}", event.outcome))),
                        ("backoff", ReplValue::Int(event.restart_delay_ms as i64)),
                    ],
                )
            })
            .collect();
        Ok(record(
            "SupervisorSnapshot",
            vec![
                (
                    "running",
                    ReplValue::Bool(matches!(snapshot.state, VmSupervisorState::Running)),
                ),
                ("children", ReplValue::List(children)),
                ("restart_history", ReplValue::List(history)),
            ],
        ))
    }
}

pub(crate) fn field<'a>(
    value: &'a ReplValue,
    name: &str,
    index: usize,
) -> Result<&'a ReplValue, VmSupervisionError> {
    match value {
        ReplValue::Tuple(values) => values.get(index),
        ReplValue::Record { fields, .. } => fields
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value),
        _ => None,
    }
    .ok_or_else(|| {
        VmSupervisionError::new(
            Configuration,
            format!("supervision declaration is missing `{name}`"),
        )
    })
}

pub(crate) fn int(value: &ReplValue, name: &str, index: usize) -> Result<i64, VmSupervisionError> {
    match field(value, name, index)? {
        ReplValue::Int(value) => Ok(*value),
        _ => Err(VmSupervisionError::new(
            Configuration,
            format!("`{name}` must be Int"),
        )),
    }
}

fn nonnegative(value: &ReplValue, name: &str, index: usize) -> Result<u64, VmSupervisionError> {
    u64::try_from(int(value, name, index)?).map_err(|_| {
        VmSupervisionError::new(Configuration, format!("`{name}` must be nonnegative"))
    })
}

fn atom(value: &ReplValue) -> Result<&str, VmSupervisionError> {
    match value {
        ReplValue::Atom(value) => Ok(value),
        _ => Err(VmSupervisionError::new(
            Configuration,
            "expected a supervision policy atom",
        )),
    }
}

pub(crate) fn record(name: &str, fields: Vec<(&str, ReplValue)>) -> ReplValue {
    ReplValue::Record {
        name: format!("std.vm.Supervisor.{name}"),
        fields: fields
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect(),
    }
}

#[cfg(test)]
mod source_test;
