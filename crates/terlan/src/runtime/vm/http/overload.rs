use super::{
    finish_http1_tcp_handler, VmHttpQueue, VmHttpTcpHandler, VmHttpTcpServer, VmHttpTcpServerPoll,
};
use crate::runtime::vm::{
    http_router::VmHttpRouter,
    process::{VmExitReason, VmProcessSource, VmProcessTable},
    tcp::{VmTcpListener, VmTcpRuntime},
};

/// Policy applied when the VM HTTP worker queue reaches its bound.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmHttpOverloadPolicy {
    #[cfg(test)]
    Queue,
    #[cfg(test)]
    Reject,
    #[cfg(test)]
    Spill,
}

/// Validated source-level overload configuration owned by the VM router.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmHttpOverloadConfig {
    pub(crate) policy: VmHttpOverloadPolicy,
    pub(crate) max_pending: usize,
}

impl VmHttpOverloadConfig {
    /// Validates one bounded pending-work configuration.
    pub(crate) fn new(policy: VmHttpOverloadPolicy, max_pending: usize) -> Result<Self, String> {
        if max_pending == 0 {
            return Err("max_pending must be greater than 0".to_string());
        }
        Ok(Self {
            policy,
            max_pending,
        })
    }
}

impl VmHttpTcpServer {
    /// Creates server state from one validated materialized router.
    #[cfg(test)]
    pub(crate) fn from_router(
        listener: VmTcpListener,
        handler_source: VmProcessSource,
        router: &VmHttpRouter,
    ) -> Self {
        let mut server = Self::new(listener, handler_source);
        server.overload = router.overload_config();
        server
    }

    /// Returns the saturated policy, if the configured pending-work bound has
    /// been reached.
    #[cfg(test)]
    pub(super) fn saturated_overload_policy(&self) -> Option<VmHttpOverloadPolicy> {
        self.overload
            .filter(|config| self.handlers.len() >= config.max_pending)
            .map(|config| config.policy)
    }

    /// Transfers one accepted handler into the configured server admission
    /// lane while preserving explicit reject and spill accounting.
    #[cfg(test)]
    pub(super) fn admit_handler(
        &mut self,
        processes: &mut VmProcessTable,
        tcp: &mut VmTcpRuntime,
        handler: VmHttpTcpHandler,
        report: &mut VmHttpTcpServerPoll,
    ) -> Result<(), String> {
        self.accepted_total += 1;
        report.accepted += 1;
        match self.saturated_overload_policy() {
            Some(VmHttpOverloadPolicy::Reject) => {
                finish_http1_tcp_handler(
                    processes,
                    tcp,
                    &handler,
                    VmExitReason::Error("VM HTTP server overloaded".to_string()),
                )?;
                self.rejected_total += 1;
                report.rejected += 1;
            }
            Some(VmHttpOverloadPolicy::Spill) => {
                self.retain_handler(processes, tcp, handler)?;
                self.spilled_total += 1;
                report.spilled += 1;
            }
            Some(VmHttpOverloadPolicy::Queue) | None => {
                self.retain_handler(processes, tcp, handler)?;
            }
        }
        Ok(())
    }
}

/// Typed ownership result from one VM HTTP queue admission attempt.
#[must_use = "rejected and spilled HTTP work remains caller-owned"]
#[derive(Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum VmHttpEnqueueOutcome<T> {
    Enqueued,
    Rejected(T),
    Spilled(T),
}

impl<T> VmHttpQueue<T> {
    /// Admits work according to an explicit bounded-overload policy.
    #[cfg(test)]
    pub(crate) fn enqueue_with_policy(
        &self,
        item: T,
        policy: VmHttpOverloadPolicy,
    ) -> Result<VmHttpEnqueueOutcome<T>, String> {
        match policy {
            VmHttpOverloadPolicy::Queue => {
                self.enqueue(item)?;
                return Ok(VmHttpEnqueueOutcome::Enqueued);
            }
            VmHttpOverloadPolicy::Reject | VmHttpOverloadPolicy::Spill => {}
        }

        let mut state = self
            .state
            .lock()
            .map_err(|_| "VM HTTP queue lock poisoned".to_string())?;
        if state.items.len() >= self.capacity {
            return Ok(if policy == VmHttpOverloadPolicy::Reject {
                VmHttpEnqueueOutcome::Rejected(item)
            } else {
                VmHttpEnqueueOutcome::Spilled(item)
            });
        }

        state.items.push_back(item);
        state.enqueue_count += 1;
        state.max_depth = state.max_depth.max(state.items.len());
        self.not_empty.notify_one();
        Ok(VmHttpEnqueueOutcome::Enqueued)
    }
}
