#[cfg(test)]
use super::process::VmProcessId;
use super::process::VmProcessSource;
#[cfg(test)]
use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[cfg(test)]
struct VmCallMemoryKey {
    module: String,
    function: String,
    arity: usize,
}

#[cfg(test)]
impl From<&VmProcessSource> for VmCallMemoryKey {
    fn from(source: &VmProcessSource) -> Self {
        Self {
            module: source.module.clone(),
            function: source.function.clone(),
            arity: source.arity,
        }
    }
}

/// Per-process cumulative allocation attributed to one function identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmCallMemoryProcessSnapshot {
    pub(crate) pid: u64,
    pub(crate) calls: u64,
    pub(crate) allocated_bytes: u64,
}

/// Typed inspection result for one exact function identity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmCallMemoryState {
    Disabled,
    Enabled {
        processes: Vec<VmCallMemoryProcessSnapshot>,
    },
}

/// Immutable source-ordered function allocation profile.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmCallMemorySnapshot {
    pub(crate) source: VmProcessSource,
    pub(crate) processes: Vec<VmCallMemoryProcessSnapshot>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
struct VmCallMemoryEntry {
    source: VmProcessSource,
    processes: BTreeMap<u64, VmCallMemoryProcessSnapshot>,
}

/// VM-owned function allocation attribution using logical byte units.
///
/// Allocation sites call `record_allocations` only after the normal memory
/// accountant accepts ownership. Heap release and process exit therefore do
/// not erase the cumulative diagnostic history.
#[derive(Debug, Default)]
pub(crate) struct VmCallMemoryRegistry {
    #[cfg(test)]
    entries: BTreeMap<VmCallMemoryKey, VmCallMemoryEntry>,
}

impl VmCallMemoryRegistry {
    /// Enables exact function allocation attribution without resetting it.
    #[cfg(test)]
    pub(crate) fn enable(&mut self, source: VmProcessSource) {
        let key = VmCallMemoryKey::from(&source);
        match self.entries.get_mut(&key) {
            Some(entry) => entry.source = source,
            None => {
                self.entries.insert(
                    key,
                    VmCallMemoryEntry {
                        source,
                        processes: BTreeMap::new(),
                    },
                );
            }
        }
    }

    /// Disables a function profile and removes all retained process rows.
    #[cfg(test)]
    pub(crate) fn disable(&mut self, source: &VmProcessSource) -> bool {
        self.entries
            .remove(&VmCallMemoryKey::from(source))
            .is_some()
    }

    /// Clears retained rows while leaving function attribution enabled.
    #[cfg(test)]
    pub(crate) fn restart(&mut self, source: &VmProcessSource) -> Result<(), String> {
        let entry = self
            .entries
            .get_mut(&VmCallMemoryKey::from(source))
            .ok_or_else(|| disabled_error("restart", source))?;
        entry.processes.clear();
        Ok(())
    }

    /// Records a validated batch of calls and allocated logical bytes.
    ///
    /// Both counters are checked before mutation so either overflow leaves the
    /// previous process row unchanged. Disabled functions are a no-op.
    #[cfg(test)]
    pub(crate) fn record_allocations(
        &mut self,
        source: &VmProcessSource,
        pid: VmProcessId,
        calls: u64,
        allocated_bytes: u64,
    ) -> Result<bool, String> {
        let key = VmCallMemoryKey::from(source);
        let Some(entry) = self.entries.get_mut(&key) else {
            return Ok(false);
        };
        let current =
            entry
                .processes
                .get(&pid.as_u64())
                .copied()
                .unwrap_or(VmCallMemoryProcessSnapshot {
                    pid: pid.as_u64(),
                    calls: 0,
                    allocated_bytes: 0,
                });
        let next_calls = current
            .calls
            .checked_add(calls)
            .ok_or_else(|| overflow_error(source, pid, "call", current.calls))?;
        let next_bytes = current
            .allocated_bytes
            .checked_add(allocated_bytes)
            .ok_or_else(|| {
                overflow_error(source, pid, "allocated-byte", current.allocated_bytes)
            })?;
        entry.processes.insert(
            pid.as_u64(),
            VmCallMemoryProcessSnapshot {
                pid: pid.as_u64(),
                calls: next_calls,
                allocated_bytes: next_bytes,
            },
        );
        Ok(true)
    }

    /// Returns typed state for one exact function identity.
    #[cfg(test)]
    pub(crate) fn state(&self, source: &VmProcessSource) -> VmCallMemoryState {
        match self.entries.get(&VmCallMemoryKey::from(source)) {
            None => VmCallMemoryState::Disabled,
            Some(entry) => VmCallMemoryState::Enabled {
                processes: entry.processes.values().copied().collect(),
            },
        }
    }

    /// Returns immutable function and process ordered allocation rows.
    #[cfg(test)]
    pub(crate) fn snapshots(&self) -> Vec<VmCallMemorySnapshot> {
        self.entries
            .values()
            .map(|entry| VmCallMemorySnapshot {
                source: entry.source.clone(),
                processes: entry.processes.values().copied().collect(),
            })
            .collect()
    }
}

#[cfg(test)]
fn disabled_error(action: &str, source: &VmProcessSource) -> String {
    format!(
        "cannot {action} disabled VM call memory for {}.{}/{}",
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
        "VM call memory {counter} overflow for {}.{}/{} process {} at {current}",
        source.module,
        source.function,
        source.arity,
        pid.as_u64()
    )
}
