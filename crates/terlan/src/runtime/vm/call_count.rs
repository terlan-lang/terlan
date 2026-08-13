pub(crate) use super::call_metric::VmCallMetricMode as VmCallCountMode;
use super::process::VmProcessSource;
#[cfg(test)]
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg(test)]
struct VmCallCountKey {
    module: String,
    function: String,
    arity: usize,
}

#[cfg(test)]
impl From<&VmProcessSource> for VmCallCountKey {
    fn from(source: &VmProcessSource) -> Self {
        Self {
            module: source.module.clone(),
            function: source.function.clone(),
            arity: source.arity,
        }
    }
}

/// Current state returned for one exact function identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmCallCountState {
    Disabled,
    Active { count: u64 },
    Paused { count: u64 },
}

/// Immutable row returned by deterministic call-count inspection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmCallCountSnapshot {
    pub(crate) source: VmProcessSource,
    pub(crate) mode: VmCallCountMode,
    pub(crate) count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
struct VmCallCountEntry {
    source: VmProcessSource,
    mode: VmCallCountMode,
    count: u64,
}

/// VM-owned per-function call accounting used by diagnostics and profiling.
///
/// The registry deliberately uses typed source identities instead of mutable
/// Erlang trace patterns or tracer processes. Recording is an explicit VM
/// function-entry hook, and inspection never changes counter state.
#[derive(Debug, Default)]
pub(crate) struct VmCallCountRegistry {
    #[cfg(test)]
    entries: BTreeMap<VmCallCountKey, VmCallCountEntry>,
}

impl VmCallCountRegistry {
    /// Enables an exact function identity without resetting an existing count.
    #[cfg(test)]
    pub(crate) fn enable(&mut self, source: VmProcessSource) {
        let key = VmCallCountKey::from(&source);
        match self.entries.get_mut(&key) {
            Some(entry) => {
                entry.source = source;
                entry.mode = VmCallCountMode::Active;
            }
            None => {
                self.entries.insert(
                    key,
                    VmCallCountEntry {
                        source,
                        mode: VmCallCountMode::Active,
                        count: 0,
                    },
                );
            }
        }
    }

    /// Disables an exact function identity and removes its retained count.
    #[cfg(test)]
    pub(crate) fn disable(&mut self, source: &VmProcessSource) -> bool {
        self.entries.remove(&VmCallCountKey::from(source)).is_some()
    }

    /// Pauses an enabled counter without changing its retained value.
    #[cfg(test)]
    pub(crate) fn pause(&mut self, source: &VmProcessSource) -> Result<(), String> {
        let entry = self.entry_mut(source, "pause")?;
        entry.mode = VmCallCountMode::Paused;
        Ok(())
    }

    /// Restarts an enabled counter at zero and resumes call recording.
    #[cfg(test)]
    pub(crate) fn restart(&mut self, source: &VmProcessSource) -> Result<(), String> {
        let entry = self.entry_mut(source, "restart")?;
        entry.count = 0;
        entry.mode = VmCallCountMode::Active;
        Ok(())
    }

    /// Records a batch of entries at the VM function-dispatch boundary.
    ///
    /// Disabled and paused functions are intentionally mutation-free. An
    /// overflowing active counter is rejected before its value changes.
    #[cfg(test)]
    pub(crate) fn record_entries(
        &mut self,
        source: &VmProcessSource,
        entries: u64,
    ) -> Result<bool, String> {
        let key = VmCallCountKey::from(source);
        let Some(entry) = self.entries.get_mut(&key) else {
            return Ok(false);
        };
        if entry.mode == VmCallCountMode::Paused {
            return Ok(false);
        }
        let next = entry.count.checked_add(entries).ok_or_else(|| {
            format!(
                "VM call count overflow for {}.{}/{} at {}",
                source.module, source.function, source.arity, entry.count
            )
        })?;
        entry.count = next;
        Ok(true)
    }

    /// Returns current state for one exact module/function/arity identity.
    #[cfg(test)]
    pub(crate) fn state(&self, source: &VmProcessSource) -> VmCallCountState {
        match self.entries.get(&VmCallCountKey::from(source)) {
            None => VmCallCountState::Disabled,
            Some(entry) if entry.mode == VmCallCountMode::Active => {
                VmCallCountState::Active { count: entry.count }
            }
            Some(entry) => VmCallCountState::Paused { count: entry.count },
        }
    }

    /// Returns stable, source-identity-ordered counter rows without mutation.
    #[cfg(test)]
    pub(crate) fn snapshots(&self) -> Vec<VmCallCountSnapshot> {
        self.entries
            .values()
            .map(|entry| VmCallCountSnapshot {
                source: entry.source.clone(),
                mode: entry.mode,
                count: entry.count,
            })
            .collect()
    }

    #[cfg(test)]
    fn entry_mut(
        &mut self,
        source: &VmProcessSource,
        action: &str,
    ) -> Result<&mut VmCallCountEntry, String> {
        self.entries
            .get_mut(&VmCallCountKey::from(source))
            .ok_or_else(|| {
                format!(
                    "cannot {action} disabled VM call count for {}.{}/{}",
                    source.module, source.function, source.arity
                )
            })
    }
}
