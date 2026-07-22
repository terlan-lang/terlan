use std::collections::{BTreeMap, BTreeSet};

use super::{VmHttpTcpServer, VmHttpTcpServerPoll};
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessId, VmProcessTable},
    scheduler::VmScheduler,
    tcp::VmTcpRuntime,
    timer::{VmTimerEvent, VmTimerId, VmTimerTable},
};

/// Active request-deadline timers indexed by their handler process.
#[derive(Debug, Default)]
pub(super) struct VmHttpHandlerDeadlines {
    timers: BTreeMap<VmProcessId, VmTimerId>,
}

/// HTTP poll outcome with typed VM timer evidence.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmHttpDeadlinePoll {
    pub(crate) http: VmHttpTcpServerPoll,
    pub(crate) timer_events: Vec<VmTimerEvent>,
    pub(crate) timed_out_handlers: Vec<VmProcessId>,
}

impl VmHttpTcpServer {
    /// Creates server state with a VM-owned request deadline for every handler.
    pub(crate) fn with_handler_timeout_ticks(
        listener: crate::runtime::vm::tcp::VmTcpListener,
        handler_source: crate::runtime::vm::process::VmProcessSource,
        timeout_ticks: u64,
    ) -> Result<Self, String> {
        if timeout_ticks == 0 {
            return Err("VM HTTP handler timeout must be greater than 0 ticks".to_string());
        }
        let mut server = Self::new(listener, handler_source);
        server.handler_timeout_ticks = Some(timeout_ticks);
        Ok(server)
    }

    /// Polls keep-alive HTTP handlers while enforcing VM-owned request deadlines.
    pub(crate) fn poll_keep_alive_with_deadlines(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        timers: &mut VmTimerTable,
        scheduler: &mut VmScheduler,
        now_tick: u64,
        handler: impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
    ) -> Result<VmHttpDeadlinePoll, String> {
        let timeout_ticks = self
            .handler_timeout_ticks
            .ok_or_else(|| "VM HTTP handler deadlines require a configured timeout".to_string())?;
        let deadline_tick = now_tick
            .checked_add(timeout_ticks)
            .ok_or_else(|| format!("VM HTTP handler deadline overflow at tick {now_tick}"))?;

        let mut timer_events = timers.advance_clock(processes, scheduler, now_tick);
        let mut timed_out_handlers = Vec::new();
        for event in &timer_events {
            let timer_id = event.timer_id();
            let Some(process) = self.handler_deadlines.owner_for_timer(timer_id) else {
                continue;
            };
            self.handler_deadlines.remove(process);
            if self
                .cancel_handler(
                    processes,
                    tcp,
                    process,
                    VmExitReason::Error("http_request_deadline_exceeded".to_string()),
                )?
                .is_some()
            {
                timed_out_handlers.push(process);
            }
        }

        let http = self.poll_keep_alive(processes, tcp, handler)?;
        for process in self.last_completed_handlers.clone() {
            if let Some(timer_id) = self.handler_deadlines.remove(process) {
                timer_events.push(timers.cancel(timer_id)?);
            }
        }

        let completed = self
            .last_completed_handlers
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        let active = self
            .handlers
            .iter()
            .map(|handler| handler.process)
            .collect::<BTreeSet<_>>();
        for process in self.handler_deadlines.owners() {
            if !active.contains(&process) {
                if let Some(timer_id) = self.handler_deadlines.remove(process) {
                    timer_events.push(timers.cancel(timer_id)?);
                }
            }
        }
        for process in active {
            if !completed.contains(&process) && !self.handler_deadlines.contains(process) {
                let timer_id = timers.start_one_shot(processes, process, deadline_tick)?;
                self.handler_deadlines.insert(process, timer_id);
            }
        }

        Ok(VmHttpDeadlinePoll {
            http,
            timer_events,
            timed_out_handlers,
        })
    }
}

impl VmHttpHandlerDeadlines {
    fn contains(&self, process: VmProcessId) -> bool {
        self.timers.contains_key(&process)
    }

    fn insert(&mut self, process: VmProcessId, timer: VmTimerId) {
        self.timers.insert(process, timer);
    }

    fn remove(&mut self, process: VmProcessId) -> Option<VmTimerId> {
        self.timers.remove(&process)
    }

    fn owners(&self) -> Vec<VmProcessId> {
        self.timers.keys().copied().collect()
    }

    fn owner_for_timer(&self, timer: VmTimerId) -> Option<VmProcessId> {
        self.timers
            .iter()
            .find_map(|(process, known)| (*known == timer).then_some(*process))
    }
}
