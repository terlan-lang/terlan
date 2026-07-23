//! Linear transfer of actor-local scheduler placement.

use std::fmt;

use super::telemetry::VmSchedulerProcessMetrics;
use super::{VmProcessId, VmProcessState, VmProcessTable, VmScheduler, VmSchedulerClass};

/// Scheduler-owned state that follows one actor between explicit owners.
#[derive(Debug)]
pub(crate) struct VmSchedulerPlacementTransfer {
    process: VmProcessId,
    class: VmSchedulerClass,
    queued: bool,
    metrics: Option<VmSchedulerProcessMetrics>,
}

impl VmSchedulerPlacementTransfer {
    /// Returns the process whose placement this transfer controls.
    pub(crate) const fn process_id(&self) -> VmProcessId {
        self.process
    }

    /// Returns the scheduling class preserved across migration.
    pub(crate) const fn class(&self) -> VmSchedulerClass {
        self.class
    }

    /// Returns whether the source had published runnable queue membership.
    pub(crate) const fn was_queued(&self) -> bool {
        self.queued
    }
}

/// Failed scheduler placement import retaining rollback ownership.
#[derive(Debug)]
pub(crate) struct VmSchedulerImportFailure {
    reason: String,
    transfer: VmSchedulerPlacementTransfer,
}

impl VmSchedulerImportFailure {
    /// Returns the stable destination rejection.
    pub(crate) fn reason(&self) -> &str {
        &self.reason
    }

    /// Returns the complete placement transfer for source restoration.
    pub(crate) fn into_transfer(self) -> VmSchedulerPlacementTransfer {
        self.transfer
    }
}

impl fmt::Display for VmSchedulerImportFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.reason)
    }
}

impl std::error::Error for VmSchedulerImportFailure {}

impl VmScheduler {
    /// Detaches queue placement and process-local accounting from this owner.
    pub(crate) fn detach_process_placement(
        &mut self,
        process: VmProcessId,
    ) -> VmSchedulerPlacementTransfer {
        let class = self
            .classes
            .get(&process)
            .copied()
            .unwrap_or(VmSchedulerClass::Normal);
        let queued = self.queued.contains(&process);
        self.remove_queued(process, "migrate");
        self.classes.remove(&process);
        let metrics = self.metrics.processes.remove(&process.as_u64());
        VmSchedulerPlacementTransfer {
            process,
            class,
            queued,
            metrics,
        }
    }

    /// Verifies destination placement before consuming migration authority.
    pub(crate) fn validate_process_placement_import(
        &self,
        processes: &VmProcessTable,
        transfer: &VmSchedulerPlacementTransfer,
    ) -> Result<(), String> {
        let process = processes.get(transfer.process).ok_or_else(|| {
            format!(
                "scheduler transfer process {} is missing",
                transfer.process.as_u64()
            )
        })?;
        if self.classes.contains_key(&transfer.process) || self.queued.contains(&transfer.process) {
            return Err(format!(
                "scheduler transfer process {} already has destination placement",
                transfer.process.as_u64()
            ));
        }
        if transfer.queued && process.state != VmProcessState::Runnable {
            return Err(format!(
                "scheduler transfer process {} was queued but is not runnable",
                transfer.process.as_u64()
            ));
        }
        if self
            .metrics
            .processes
            .contains_key(&transfer.process.as_u64())
        {
            return Err(format!(
                "scheduler transfer process {} already has destination metrics",
                transfer.process.as_u64()
            ));
        }
        Ok(())
    }

    /// Imports actor placement or returns it unchanged for source rollback.
    pub(crate) fn import_process_placement(
        &mut self,
        processes: &VmProcessTable,
        transfer: VmSchedulerPlacementTransfer,
    ) -> Result<(), VmSchedulerImportFailure> {
        if let Err(reason) = self.validate_process_placement_import(processes, &transfer) {
            return Err(VmSchedulerImportFailure { reason, transfer });
        }
        self.classes.insert(transfer.process, transfer.class);
        if let Some(metrics) = transfer.metrics {
            self.metrics
                .processes
                .insert(transfer.process.as_u64(), metrics);
        }
        if transfer.queued {
            self.enqueue_unchecked(processes, transfer.process)
                .expect("validated runnable process can be queued");
        }
        Ok(())
    }
}
