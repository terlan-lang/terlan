pub(crate) use super::call_metric::VmCallMetricMode as VmCallTimeMode;
#[cfg(test)]
use super::process::VmProcessId;
use super::process::VmProcessSource;
#[cfg(test)]
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg(test)]
struct VmCallTimeKey {
    module: String,
    function: String,
    arity: usize,
}

#[cfg(test)]
impl From<&VmProcessSource> for VmCallTimeKey {
    fn from(source: &VmProcessSource) -> Self {
        Self {
            module: source.module.clone(),
            function: source.function.clone(),
            arity: source.arity,
        }
    }
}

/// Per-process cumulative exclusive execution attributed to one function.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmCallTimeProcessSnapshot {
    pub(crate) pid: u64,
    pub(crate) calls: u64,
    pub(crate) exclusive_ticks: u64,
}

/// Typed inspection result for one exact function identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmCallTimeState {
    Disabled,
    Active {
        processes: Vec<VmCallTimeProcessSnapshot>,
    },
    Paused {
        processes: Vec<VmCallTimeProcessSnapshot>,
    },
}

/// Immutable source-ordered function execution-time profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmCallTimeSnapshot {
    pub(crate) source: VmProcessSource,
    pub(crate) mode: VmCallTimeMode,
    pub(crate) processes: Vec<VmCallTimeProcessSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
struct VmCallTimeEntry {
    source: VmProcessSource,
    mode: VmCallTimeMode,
    processes: BTreeMap<u64, VmCallTimeProcessSnapshot>,
}

/// VM-owned exclusive function-time attribution using logical scheduler ticks.
///
/// Dispatch boundaries submit already-partitioned exclusive ticks, so nested
/// function execution is charged to the active callee rather than duplicated
/// in every caller. Inspection never reads a host clock or mutates a profile.
#[derive(Debug, Default)]
pub(crate) struct VmCallTimeRegistry {
    #[cfg(test)]
    entries: BTreeMap<VmCallTimeKey, VmCallTimeEntry>,
}

impl VmCallTimeRegistry {
    /// Enables a profile without discarding its retained process rows.
    #[cfg(test)]
    pub(crate) fn enable(&mut self, source: VmProcessSource) {
        let key = VmCallTimeKey::from(&source);
        match self.entries.get_mut(&key) {
            Some(entry) => {
                entry.source = source;
                entry.mode = VmCallTimeMode::Active;
            }
            None => {
                self.entries.insert(
                    key,
                    VmCallTimeEntry {
                        source,
                        mode: VmCallTimeMode::Active,
                        processes: BTreeMap::new(),
                    },
                );
            }
        }
    }

    /// Disables a profile and removes all retained process rows.
    #[cfg(test)]
    pub(crate) fn disable(&mut self, source: &VmProcessSource) -> bool {
        self.entries.remove(&VmCallTimeKey::from(source)).is_some()
    }

    /// Pauses recording while preserving all retained process rows.
    #[cfg(test)]
    pub(crate) fn pause(&mut self, source: &VmProcessSource) -> Result<(), String> {
        let entry = self.entry_mut(source, "pause")?;
        entry.mode = VmCallTimeMode::Paused;
        Ok(())
    }

    /// Clears retained rows and resumes an enabled profile.
    #[cfg(test)]
    pub(crate) fn restart(&mut self, source: &VmProcessSource) -> Result<(), String> {
        let entry = self.entry_mut(source, "restart")?;
        entry.processes.clear();
        entry.mode = VmCallTimeMode::Active;
        Ok(())
    }

    /// Records a validated batch of calls and exclusive logical ticks.
    ///
    /// Both counters are checked before mutation. Disabled and paused profiles
    /// are no-ops, while overflow leaves the previous row unchanged.
    #[cfg(test)]
    pub(crate) fn record_execution(
        &mut self,
        source: &VmProcessSource,
        pid: VmProcessId,
        calls: u64,
        exclusive_ticks: u64,
    ) -> Result<bool, String> {
        let Some(entry) = self.entries.get_mut(&VmCallTimeKey::from(source)) else {
            return Ok(false);
        };
        if entry.mode == VmCallTimeMode::Paused {
            return Ok(false);
        }
        let current =
            entry
                .processes
                .get(&pid.as_u64())
                .copied()
                .unwrap_or(VmCallTimeProcessSnapshot {
                    pid: pid.as_u64(),
                    calls: 0,
                    exclusive_ticks: 0,
                });
        let next_calls = current
            .calls
            .checked_add(calls)
            .ok_or_else(|| overflow_error(source, pid, "call", current.calls))?;
        let next_ticks = current
            .exclusive_ticks
            .checked_add(exclusive_ticks)
            .ok_or_else(|| {
                overflow_error(source, pid, "exclusive-tick", current.exclusive_ticks)
            })?;
        entry.processes.insert(
            pid.as_u64(),
            VmCallTimeProcessSnapshot {
                pid: pid.as_u64(),
                calls: next_calls,
                exclusive_ticks: next_ticks,
            },
        );
        Ok(true)
    }

    /// Returns typed state for one exact function identity.
    #[cfg(test)]
    pub(crate) fn state(&self, source: &VmProcessSource) -> VmCallTimeState {
        let Some(entry) = self.entries.get(&VmCallTimeKey::from(source)) else {
            return VmCallTimeState::Disabled;
        };
        let processes = entry.processes.values().copied().collect();
        match entry.mode {
            VmCallTimeMode::Active => VmCallTimeState::Active { processes },
            VmCallTimeMode::Paused => VmCallTimeState::Paused { processes },
        }
    }

    /// Returns immutable function- and process-ordered execution profiles.
    #[cfg(test)]
    pub(crate) fn snapshots(&self) -> Vec<VmCallTimeSnapshot> {
        self.entries
            .values()
            .map(|entry| VmCallTimeSnapshot {
                source: entry.source.clone(),
                mode: entry.mode,
                processes: entry.processes.values().copied().collect(),
            })
            .collect()
    }

    #[cfg(test)]
    fn entry_mut(
        &mut self,
        source: &VmProcessSource,
        action: &str,
    ) -> Result<&mut VmCallTimeEntry, String> {
        self.entries
            .get_mut(&VmCallTimeKey::from(source))
            .ok_or_else(|| disabled_error(action, source))
    }
}

#[cfg(test)]
fn disabled_error(action: &str, source: &VmProcessSource) -> String {
    format!(
        "cannot {action} disabled VM call time for {}.{}/{}",
        source.module, source.function, source.arity
    )
}

#[cfg(test)]
fn overflow_error(
    source: &VmProcessSource,
    pid: VmProcessId,
    counter: &str,
    current: u64,
) -> String {
    format!(
        "VM call time {counter} overflow for {}.{}/{} process {} at {current}",
        source.module,
        source.function,
        source.arity,
        pid.as_u64()
    )
}
