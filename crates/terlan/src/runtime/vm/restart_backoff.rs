/// Deterministic restart backoff shared by VM lifecycle owners.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmRestartBackoffSchedule {
    pub(crate) initial_delay_ms: u64,
    pub(crate) max_delay_ms: u64,
}

impl VmRestartBackoffSchedule {
    /// Creates an exponential backoff schedule capped by `max_delay_ms`.
    pub(crate) fn exponential(initial_delay_ms: u64, max_delay_ms: u64) -> Self {
        Self {
            initial_delay_ms,
            max_delay_ms,
        }
    }

    /// Returns the capped delay for a one-based restart attempt.
    pub(crate) fn delay_for_restart_count(&self, restart_count: u32) -> u64 {
        if restart_count == 0 || self.initial_delay_ms == 0 || self.max_delay_ms == 0 {
            return 0;
        }
        let mut delay = self.initial_delay_ms;
        for _ in 1..restart_count {
            delay = delay.saturating_mul(2).min(self.max_delay_ms);
        }
        delay.min(self.max_delay_ms)
    }
}
