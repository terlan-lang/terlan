use super::{VmHttpLifecycleEvent, VmHttpShutdownMode, VmHttpTcpServer};
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessId, VmProcessTable},
    tcp::VmTcpRuntime,
    tls::{VmTlsPlan, VmTlsRuntime},
};

/// Final state of a bounded graceful HTTP drain.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum VmHttpDrainOutcome {
    Pending,
    Drained,
    Forced,
}

/// Internal HTTP server lifecycle used to enforce drain transitions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum VmHttpLifecycleState {
    Running,
    Draining {
        initial_completed_total: usize,
        remaining_polls: usize,
        completed_polls: usize,
    },
    Stopped,
}

/// Deterministic accounting for a graceful HTTP drain.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmHttpDrainReport {
    pub(crate) outcome: VmHttpDrainOutcome,
    pub(crate) polls: usize,
    pub(crate) completed_handlers: usize,
    pub(crate) forced_handlers: usize,
    pub(crate) cleanup: Vec<String>,
}

impl VmHttpTcpServer {
    /// Cancels one active handler and releases its VM process and TCP stream.
    #[cfg(test)]
    pub(crate) fn cancel_handler(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        process: VmProcessId,
        reason: VmExitReason,
    ) -> Result<Option<Vec<String>>, String> {
        let Some(index) = self
            .handlers
            .iter()
            .position(|handler| handler.process == process)
        else {
            return Ok(None);
        };
        let handler = self.handlers.remove(index);
        if self.handlers.is_empty() {
            self.next_handler_index = 0;
        } else {
            if index < self.next_handler_index {
                self.next_handler_index -= 1;
            }
            if self.next_handler_index >= self.handlers.len() {
                self.next_handler_index = 0;
            }
        }
        self.finish_handler(processes, tcp, &handler, reason)
            .map(Some)
    }

    /// Stops new accepts and starts a scheduler-driven graceful drain.
    #[cfg(test)]
    pub(crate) fn begin_drain(
        &mut self,
        tcp: &mut VmTcpRuntime,
        max_polls: usize,
    ) -> Result<(), String> {
        if max_polls == 0 {
            return Err("VM HTTP drain poll limit must be greater than 0".to_string());
        }
        match self.lifecycle {
            VmHttpLifecycleState::Running => {}
            VmHttpLifecycleState::Draining { .. } => {
                return Err("VM HTTP server is already draining".to_string());
            }
            VmHttpLifecycleState::Stopped => {
                return Err("VM HTTP server is stopped".to_string());
            }
        }
        let event = VmHttpLifecycleEvent::ShutdownHandoff {
            mode: VmHttpShutdownMode::Drain,
            active_handlers: self.handlers.len(),
        };
        self.authorize_lifecycle(&event)?;
        tcp.close_listener(self.listener)?;
        self.lifecycle = VmHttpLifecycleState::Draining {
            initial_completed_total: self.completed_total,
            remaining_polls: max_polls,
            completed_polls: 0,
        };
        self.observe_lifecycle(&event)?;
        Ok(())
    }

    /// Polls one plaintext graceful-drain scheduler tick.
    #[cfg(test)]
    pub(crate) fn poll_drain(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        forced_reason: VmExitReason,
        handler: impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
    ) -> Result<VmHttpDrainReport, String> {
        self.poll_drain_retained(processes, tcp, None, forced_reason, handler)
    }

    /// Polls one TLS-aware graceful-drain scheduler tick and removes the
    /// listener plan after a terminal outcome.
    #[cfg(test)]
    pub(crate) fn poll_drain_with_tls(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        tls: &mut VmTlsRuntime,
        forced_reason: VmExitReason,
        handler: impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
    ) -> Result<(VmHttpDrainReport, Option<VmTlsPlan>), String> {
        let report =
            self.poll_drain_retained(processes, tcp, Some(&*tls), forced_reason, handler)?;
        let plan = (report.outcome != VmHttpDrainOutcome::Pending)
            .then(|| tls.remove_listener_plan(self.listener))
            .flatten();
        Ok((report, plan))
    }

    /// Polls one graceful-drain tick while retaining an optional TLS plan for
    /// the duration of the scheduler operation.
    #[cfg(test)]
    fn poll_drain_retained(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        tls: Option<&VmTlsRuntime>,
        forced_reason: VmExitReason,
        handler: impl FnMut(::http::Request<String>) -> Result<::http::Response<String>, String>,
    ) -> Result<VmHttpDrainReport, String> {
        let (initial_completed_total, remaining_polls, completed_polls) = match self.lifecycle {
            VmHttpLifecycleState::Running => {
                return Err("VM HTTP server drain has not started".to_string());
            }
            VmHttpLifecycleState::Draining {
                initial_completed_total,
                remaining_polls,
                completed_polls,
            } => (initial_completed_total, remaining_polls, completed_polls),
            VmHttpLifecycleState::Stopped => {
                return Err("VM HTTP server is stopped".to_string());
            }
        };

        if self.handlers.is_empty() {
            self.lifecycle = VmHttpLifecycleState::Stopped;
            return Ok(VmHttpDrainReport {
                outcome: VmHttpDrainOutcome::Drained,
                polls: completed_polls,
                completed_handlers: self.completed_total.saturating_sub(initial_completed_total),
                forced_handlers: 0,
                cleanup: Vec::new(),
            });
        }

        match tls {
            Some(tls) => {
                self.poll_retained_tls_handlers(processes, tcp, tls, usize::MAX, handler)?
            }
            None => self.poll_retained_handlers(processes, tcp, usize::MAX, handler)?,
        };
        let completed_polls = completed_polls + 1;
        let completed_handlers = self.completed_total.saturating_sub(initial_completed_total);
        if self.handlers.is_empty() {
            self.lifecycle = VmHttpLifecycleState::Stopped;
            self.next_handler_index = 0;
            return Ok(VmHttpDrainReport {
                outcome: VmHttpDrainOutcome::Drained,
                polls: completed_polls,
                completed_handlers,
                forced_handlers: 0,
                cleanup: Vec::new(),
            });
        }

        if remaining_polls > 1 {
            self.lifecycle = VmHttpLifecycleState::Draining {
                initial_completed_total,
                remaining_polls: remaining_polls - 1,
                completed_polls,
            };
            return Ok(VmHttpDrainReport {
                outcome: VmHttpDrainOutcome::Pending,
                polls: completed_polls,
                completed_handlers,
                forced_handlers: 0,
                cleanup: Vec::new(),
            });
        }

        let forced_handlers = self.handlers.len();
        let mut cleanup = Vec::new();
        let retained_handlers = std::mem::take(&mut self.handlers);
        for retained in retained_handlers {
            cleanup.extend(self.finish_handler(
                processes,
                tcp,
                &retained,
                forced_reason.clone(),
            )?);
        }
        self.next_handler_index = 0;
        self.lifecycle = VmHttpLifecycleState::Stopped;
        Ok(VmHttpDrainReport {
            outcome: VmHttpDrainOutcome::Forced,
            polls: completed_polls,
            completed_handlers,
            forced_handlers,
            cleanup,
        })
    }

    /// Shuts down the listener and every active handler process.
    #[cfg(test)]
    pub(crate) fn shutdown(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        reason: VmExitReason,
    ) -> Result<Vec<String>, String> {
        let event = VmHttpLifecycleEvent::ShutdownHandoff {
            mode: VmHttpShutdownMode::Immediate,
            active_handlers: self.handlers.len(),
        };
        tcp.close_listener(self.listener)?;
        let mut cleanup = Vec::new();
        let handlers = std::mem::take(&mut self.handlers);
        for handler in handlers {
            cleanup.extend(self.finish_handler(processes, tcp, &handler, reason.clone())?);
        }
        self.next_handler_index = 0;
        self.lifecycle = VmHttpLifecycleState::Stopped;
        self.observe_lifecycle(&event)?;
        Ok(cleanup)
    }

    /// Shuts down HTTP state and removes listener-bound TLS metadata.
    #[cfg(test)]
    pub(crate) fn shutdown_with_tls(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        tls: &mut VmTlsRuntime,
        reason: VmExitReason,
    ) -> Result<(Vec<String>, Option<VmTlsPlan>), String> {
        let cleanup = self.shutdown(processes, tcp, reason)?;
        let tls_plan = tls.remove_listener_plan(self.listener);
        Ok((cleanup, tls_plan))
    }
}
