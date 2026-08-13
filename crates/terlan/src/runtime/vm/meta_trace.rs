use std::collections::BTreeMap;

use super::process::{VmProcessId, VmProcessLocation, VmProcessSource};

#[cfg(test)]
#[path = "meta_trace_test.rs"]
#[cfg(test)]
mod meta_trace_test;

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct VmMetaTraceKey {
    module: String,
    function: String,
    arity: usize,
}

impl From<&VmProcessSource> for VmMetaTraceKey {
    fn from(source: &VmProcessSource) -> Self {
        Self {
            module: source.module.clone(),
            function: source.function.clone(),
            arity: source.arity,
        }
    }
}

/// Event classes admitted for one observer-scoped function subscription.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmMetaTraceConfig {
    returns: bool,
}

/// Typed state for exact-function observer inspection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum VmMetaTraceState {
    Disabled,
    Enabled {
        observer: u64,
        config: VmMetaTraceConfig,
    },
}

/// Stable global position for incremental observer inspection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmMetaTraceCursor {
    event_index: usize,
}

/// Token that pins a return event to the observer that received its call.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmMetaTraceCallToken {
    pub(crate) subject: VmProcessId,
    pub(crate) observer: VmProcessId,
    source: VmProcessSource,
}

/// One typed observer-scoped function transition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmMetaTraceEventKind {
    Call {
        location: VmProcessLocation,
    },
    Return {
        source: VmProcessSource,
        caller: VmProcessLocation,
    },
}

/// One globally ordered meta-observation event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmMetaTraceEvent {
    pub(crate) sequence: u64,
    pub(crate) observer: u64,
    pub(crate) subject: u64,
    pub(crate) kind: VmMetaTraceEventKind,
}

/// Immutable event suffix for one exact observer identity.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmMetaTraceSnapshot {
    pub(crate) events: Vec<VmMetaTraceEvent>,
    pub(crate) next_cursor: VmMetaTraceCursor,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct VmMetaTraceEntry {
    source: VmProcessSource,
    observer: VmProcessId,
    config: VmMetaTraceConfig,
}

/// VM-owned observer-scoped diagnostic stream.
///
/// Subscriptions are explicit VM state. Return tokens retain the observer
/// chosen at call publication, so later subscription replacement cannot steal
/// an in-flight return event.
#[derive(Debug, Default)]
pub(crate) struct VmMetaTraceRegistry {
    entries: BTreeMap<VmMetaTraceKey, VmMetaTraceEntry>,
    events: Vec<VmMetaTraceEvent>,
    next_sequence: u64,
}

impl VmMetaTraceRegistry {
    /// Returns whether no function currently has an observer subscription.
    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    #[cfg(test)]
    pub(crate) fn enable(
        &mut self,
        source: VmProcessSource,
        observer: VmProcessId,
        config: VmMetaTraceConfig,
    ) -> bool {
        let key = VmMetaTraceKey::from(&source);
        self.entries
            .insert(
                key,
                VmMetaTraceEntry {
                    source,
                    observer,
                    config,
                },
            )
            .is_none()
    }

    #[cfg(test)]
    pub(crate) fn disable(&mut self, source: &VmProcessSource) -> bool {
        self.entries.remove(&VmMetaTraceKey::from(source)).is_some()
    }

    pub(crate) fn observer_for(&self, source: &VmProcessSource) -> Option<VmProcessId> {
        self.entries
            .get(&VmMetaTraceKey::from(source))
            .map(|entry| entry.observer)
    }

    #[cfg(test)]
    pub(crate) fn state(&self, source: &VmProcessSource) -> VmMetaTraceState {
        match self.entries.get(&VmMetaTraceKey::from(source)) {
            None => VmMetaTraceState::Disabled,
            Some(entry) => VmMetaTraceState::Enabled {
                observer: entry.observer.as_u64(),
                config: entry.config,
            },
        }
    }

    #[cfg(test)]
    pub(crate) fn cursor(&self) -> VmMetaTraceCursor {
        VmMetaTraceCursor {
            event_index: self.events.len(),
        }
    }

    pub(crate) fn record_call(
        &mut self,
        subject: VmProcessId,
        location: VmProcessLocation,
    ) -> Result<Option<VmMetaTraceCallToken>, String> {
        let Some(entry) = self.entries.get(&VmMetaTraceKey::from(&location.source)) else {
            return Ok(None);
        };
        let observer = entry.observer;
        let source = entry.source.clone();
        let returns = entry.config.returns;
        self.publish(observer, subject, VmMetaTraceEventKind::Call { location })?;
        Ok(returns.then_some(VmMetaTraceCallToken {
            subject,
            observer,
            source,
        }))
    }

    pub(crate) fn record_return(
        &mut self,
        token: VmMetaTraceCallToken,
        caller: VmProcessLocation,
        observer_alive: bool,
    ) -> Result<bool, String> {
        if !observer_alive {
            return Ok(false);
        }
        self.publish(
            token.observer,
            token.subject,
            VmMetaTraceEventKind::Return {
                source: token.source,
                caller,
            },
        )?;
        Ok(true)
    }

    pub(crate) fn observer_exited(&mut self, observer: VmProcessId) {
        self.entries.retain(|_, entry| entry.observer != observer);
    }

    #[cfg(test)]
    pub(crate) fn since(
        &self,
        cursor: VmMetaTraceCursor,
        observer: VmProcessId,
    ) -> Result<VmMetaTraceSnapshot, String> {
        if cursor.event_index > self.events.len() {
            return Err(format!(
                "VM meta trace cursor {} exceeds event length {}",
                cursor.event_index,
                self.events.len()
            ));
        }
        Ok(VmMetaTraceSnapshot {
            events: self.events[cursor.event_index..]
                .iter()
                .filter(|event| event.observer == observer.as_u64())
                .cloned()
                .collect(),
            next_cursor: VmMetaTraceCursor {
                event_index: self.events.len(),
            },
        })
    }

    fn publish(
        &mut self,
        observer: VmProcessId,
        subject: VmProcessId,
        kind: VmMetaTraceEventKind,
    ) -> Result<(), String> {
        let sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or_else(|| "VM meta trace sequence overflow".to_string())?;
        self.next_sequence = sequence;
        self.events.push(VmMetaTraceEvent {
            sequence,
            observer: observer.as_u64(),
            subject: subject.as_u64(),
            kind,
        });
        Ok(())
    }
}
