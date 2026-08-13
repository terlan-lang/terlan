use super::process_environment::VmRuntimeEnvironmentSnapshot;

/// Cumulative VM work performed between two immutable environment snapshots.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmRuntimeStatisticsDelta {
    pub(crate) processes_created: usize,
    pub(crate) processes_exited: usize,
    pub(crate) timers_started: u64,
    pub(crate) timers_fired: u64,
    pub(crate) timers_cancelled: u64,
    pub(crate) reductions: u64,
    pub(crate) memory_reductions: u64,
    pub(crate) scheduler_slices: u64,
    pub(crate) scheduler_preemptions: u64,
}

impl VmRuntimeEnvironmentSnapshot {
    /// Returns deterministic cumulative work since an earlier compatible
    /// snapshot. Runtime gauges such as run-queue depth and live-process count
    /// remain available on the snapshots themselves.
    #[cfg(test)]
    pub(crate) fn statistics_delta_since(
        &self,
        earlier: &Self,
    ) -> Result<VmRuntimeStatisticsDelta, String> {
        if self.process_limit != earlier.process_limit
            || self.scheduler_count != earlier.scheduler_count
            || self.word_size_bytes != earlier.word_size_bytes
        {
            return Err("cannot compare VM statistics from different runtime profiles".to_string());
        }

        Ok(VmRuntimeStatisticsDelta {
            processes_created: difference(
                "total process count",
                self.total_processes,
                earlier.total_processes,
            )?,
            processes_exited: difference(
                "exited process count",
                self.exited_processes,
                earlier.exited_processes,
            )?,
            timers_started: difference(
                "started timer count",
                self.timers_started,
                earlier.timers_started,
            )?,
            timers_fired: difference("fired timer count", self.timers_fired, earlier.timers_fired)?,
            timers_cancelled: difference(
                "cancelled timer count",
                self.timers_cancelled,
                earlier.timers_cancelled,
            )?,
            reductions: difference(
                "scheduler reduction count",
                self.total_reductions,
                earlier.total_reductions,
            )?,
            memory_reductions: difference(
                "memory reduction count",
                self.memory_reductions,
                earlier.memory_reductions,
            )?,
            scheduler_slices: difference(
                "scheduler slice count",
                self.scheduler_slices,
                earlier.scheduler_slices,
            )?,
            scheduler_preemptions: difference(
                "scheduler preemption count",
                self.scheduler_preemptions,
                earlier.scheduler_preemptions,
            )?,
        })
    }
}

#[cfg(test)]
fn difference<T>(label: &str, current: T, earlier: T) -> Result<T, String>
where
    T: Copy + std::fmt::Display + std::ops::Sub<Output = T> + PartialOrd,
{
    if current < earlier {
        return Err(format!("VM {label} regressed from {earlier} to {current}"));
    }
    Ok(current - earlier)
}
