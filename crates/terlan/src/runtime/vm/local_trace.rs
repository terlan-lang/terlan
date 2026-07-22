use std::collections::BTreeMap;

use super::process::{VmProcessId, VmProcessLocation, VmProcessSource};

#[cfg(test)]
#[path = "local_trace_test.rs"]
mod local_trace_test;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct VmLocalTraceKey {
    module: String,
    function: String,
    arity: usize,
}

impl From<&VmProcessSource> for VmLocalTraceKey {
    fn from(source: &VmProcessSource) -> Self {
        Self {
            module: source.module.clone(),
            function: source.function.clone(),
            arity: source.arity,
        }
    }
}

/// Event classes admitted for one exact local function identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmLocalTraceConfig {
    calls: bool,
    returns: bool,
    exceptions: bool,
}

/// Stable event position for incremental local diagnostics inspection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmLocalTraceCursor {
    event_index: usize,
}

/// One typed local function transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmLocalTraceEventKind {
    Call {
        location: VmProcessLocation,
    },
    Return {
        source: VmProcessSource,
        caller: VmProcessLocation,
    },
    Exception {
        source: VmProcessSource,
        class: String,
        reason: String,
        stack: Vec<VmProcessLocation>,
    },
}

/// One globally ordered VM-owned local trace event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmLocalTraceEvent {
    pub(crate) sequence: u64,
    pub(crate) pid: u64,
    pub(crate) kind: VmLocalTraceEventKind,
}

/// Immutable incremental trace result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmLocalTraceSnapshot {
    pub(crate) events: Vec<VmLocalTraceEvent>,
    pub(crate) next_cursor: VmLocalTraceCursor,
}

/// VM-owned exact-function local diagnostic stream.
///
/// The single runtime owner serializes enablement and event publication. This
/// replaces BEAM breakpoint mutation and tracer mailboxes with deterministic
/// typed state and replayable cursors.
#[derive(Debug, Default)]
pub(crate) struct VmLocalTraceRegistry {
    enabled: BTreeMap<VmLocalTraceKey, (VmProcessSource, VmLocalTraceConfig)>,
    events: Vec<VmLocalTraceEvent>,
    next_sequence: u64,
}

impl VmLocalTraceRegistry {
    /// Enables or updates one exact function without disturbing history.
    pub(crate) fn enable(&mut self, source: VmProcessSource, config: VmLocalTraceConfig) -> bool {
        let key = VmLocalTraceKey::from(&source);
        self.enabled.insert(key, (source, config)).is_none()
    }

    /// Disables one exact function without deleting already published events.
    pub(crate) fn disable(&mut self, source: &VmProcessSource) -> bool {
        self.enabled
            .remove(&VmLocalTraceKey::from(source))
            .is_some()
    }

    pub(crate) fn is_enabled(&self, source: &VmProcessSource) -> bool {
        self.enabled.contains_key(&VmLocalTraceKey::from(source))
    }

    pub(crate) fn cursor(&self) -> VmLocalTraceCursor {
        VmLocalTraceCursor {
            event_index: self.events.len(),
        }
    }

    pub(crate) fn record_call(
        &mut self,
        pid: VmProcessId,
        location: VmProcessLocation,
    ) -> Result<bool, String> {
        let Some((_, config)) = self.enabled.get(&VmLocalTraceKey::from(&location.source)) else {
            return Ok(false);
        };
        if !config.calls {
            return Ok(false);
        }
        self.publish(pid, VmLocalTraceEventKind::Call { location })?;
        Ok(true)
    }

    pub(crate) fn record_return(
        &mut self,
        pid: VmProcessId,
        source: VmProcessSource,
        caller: VmProcessLocation,
    ) -> Result<bool, String> {
        let Some((_, config)) = self.enabled.get(&VmLocalTraceKey::from(&source)) else {
            return Ok(false);
        };
        if !config.returns {
            return Ok(false);
        }
        self.publish(pid, VmLocalTraceEventKind::Return { source, caller })?;
        Ok(true)
    }

    pub(crate) fn record_exception(
        &mut self,
        pid: VmProcessId,
        source: VmProcessSource,
        class: impl Into<String>,
        reason: impl Into<String>,
        stack: Vec<VmProcessLocation>,
    ) -> Result<bool, String> {
        let Some((_, config)) = self.enabled.get(&VmLocalTraceKey::from(&source)) else {
            return Ok(false);
        };
        if !config.exceptions {
            return Ok(false);
        }
        self.publish(
            pid,
            VmLocalTraceEventKind::Exception {
                source,
                class: class.into(),
                reason: reason.into(),
                stack,
            },
        )?;
        Ok(true)
    }

    /// Returns an immutable event suffix without consuming diagnostic state.
    pub(crate) fn since(&self, cursor: VmLocalTraceCursor) -> Result<VmLocalTraceSnapshot, String> {
        if cursor.event_index > self.events.len() {
            return Err(format!(
                "VM local trace cursor {} exceeds event length {}",
                cursor.event_index,
                self.events.len()
            ));
        }
        Ok(VmLocalTraceSnapshot {
            events: self.events[cursor.event_index..].to_vec(),
            next_cursor: VmLocalTraceCursor {
                event_index: self.events.len(),
            },
        })
    }

    fn publish(&mut self, pid: VmProcessId, kind: VmLocalTraceEventKind) -> Result<(), String> {
        let sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or_else(|| "VM local trace sequence overflow".to_string())?;
        self.next_sequence = sequence;
        self.events.push(VmLocalTraceEvent {
            sequence,
            pid: pid.as_u64(),
            kind,
        });
        Ok(())
    }
}
