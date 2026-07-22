use super::VmHttpTcpServer;
use crate::runtime::vm::{
    memory::{
        VmMemoryAccountant, VmMemoryLimits, VmMemoryPressureOutcome, VmProcessMemoryMetrics,
        VmSharedAllocationId, VmSharedAllocationKind,
    },
    process::{VmProcessId, VmProcessSource, VmProcessTable},
    scheduler::VmScheduler,
    tcp::VmTcpListener,
};

const HTTP_RESPONSE_WRITE_REDUCTIONS: u64 = 1;

pub(super) struct VmHttpResponseMemory {
    memory: VmMemoryAccountant,
    scheduler: VmScheduler,
}

impl VmHttpResponseMemory {
    pub(super) fn with_default_limits() -> Self {
        Self::new(
            VmMemoryLimits::new(64 * 1024 * 1024, 256 * 1024 * 1024)
                .expect("default HTTP response memory limits are valid"),
        )
    }

    pub(super) fn new(limits: VmMemoryLimits) -> Self {
        Self {
            memory: VmMemoryAccountant::new(limits),
            scheduler: VmScheduler::default(),
        }
    }

    pub(super) fn reserve(
        &mut self,
        processes: &mut VmProcessTable,
        owner: VmProcessId,
        logical_bytes: usize,
    ) -> Result<VmSharedAllocationId, String> {
        let decision = self.memory.register_shared_allocation(
            processes,
            owner,
            VmSharedAllocationKind::ResponseBuffer,
            logical_bytes,
        )?;
        self.scheduler
            .charge_memory_reductions(processes, owner, logical_bytes)?;
        if decision.pressure.outcome == VmMemoryPressureOutcome::HardLimitRejected {
            return Err(format!(
                "VM HTTP handler process {} exceeded its response memory hard limit",
                owner.as_u64()
            ));
        }
        decision.allocation_id.ok_or_else(|| {
            "accounted VM HTTP response did not produce an allocation id".to_string()
        })
    }

    pub(super) fn complete_write(
        &mut self,
        processes: &mut VmProcessTable,
        owner: VmProcessId,
        allocation: VmSharedAllocationId,
        logical_bytes: usize,
        write_succeeded: bool,
    ) -> Result<(), String> {
        let memory_result = self
            .memory
            .release_shared_allocation(processes, allocation, owner)
            .and_then(|_| {
                self.scheduler
                    .charge_memory_reductions(processes, owner, logical_bytes)
                    .map(|_| ())
            });
        let write_result = if write_succeeded {
            self.scheduler
                .charge_runtime_reductions(processes, owner, HTTP_RESPONSE_WRITE_REDUCTIONS)
                .map(|_| ())
        } else {
            Ok(())
        };
        memory_result.and(write_result)
    }

    pub(super) fn metrics(&self, process: VmProcessId) -> Option<&VmProcessMemoryMetrics> {
        self.memory.process_metrics(process)
    }

    pub(super) fn reductions(&self, process: VmProcessId) -> u64 {
        self.scheduler.memory_reductions(process)
    }
}

impl VmHttpTcpServer {
    pub(crate) fn with_response_memory_limits(
        listener: VmTcpListener,
        handler_source: VmProcessSource,
        limits: VmMemoryLimits,
    ) -> Self {
        let mut server = Self::new(listener, handler_source);
        server.response_memory = VmHttpResponseMemory::new(limits);
        server
    }

    pub(crate) fn response_memory_metrics(
        &self,
        process: VmProcessId,
    ) -> Option<&VmProcessMemoryMetrics> {
        self.response_memory.metrics(process)
    }

    pub(crate) fn response_memory_reductions(&self, process: VmProcessId) -> u64 {
        self.response_memory.reductions(process)
    }
}
