use std::collections::VecDeque;

use super::{VmDriverCloseReport, VmDriverId, VmDriverQueuePlacement};
use crate::runtime::vm::process::VmProcessId;

#[cfg(test)]
const VM_DRIVER_TRACE_CAPACITY: usize = 4_096;

/// Coarse event classes selected by an external diagnostics adapter.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmDriverTraceClass {
    Lifecycle,
    Io,
    Timer,
    Callback,
}

impl VmDriverTraceClass {
    const fn mask(self) -> u8 {
        match self {
            Self::Lifecycle => 1 << 0,
            Self::Io => 1 << 1,
            Self::Timer => 1 << 2,
            Self::Callback => 1 << 3,
        }
    }
}

/// Allocation-free trace selection used on the driver hot path.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct VmDriverTraceConfig {
    enabled_mask: u8,
}

impl VmDriverTraceConfig {
    /// Disables driver tracing without clearing already retained diagnostics.
    pub(crate) const fn disabled() -> Self {
        Self { enabled_mask: 0 }
    }

    /// Enables every portable VM driver event class.
    pub(crate) const fn all() -> Self {
        Self {
            enabled_mask: (1 << 4) - 1,
        }
    }

    /// Enables exactly the requested portable event classes.
    pub(crate) fn selected(classes: impl IntoIterator<Item = VmDriverTraceClass>) -> Self {
        let mut enabled_mask = 0;
        for class in classes {
            enabled_mask |= class.mask();
        }
        Self { enabled_mask }
    }

    const fn includes(self, class: VmDriverTraceClass) -> bool {
        self.enabled_mask & class.mask() != 0
    }
}

/// Typed portable driver transition, independent of any host trace provider.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmDriverTraceEventKind {
    Opened {
        name: String,
    },
    ControllerChanged {
        previous: VmProcessId,
        next: VmProcessId,
    },
    Command {
        segments: usize,
        bytes: usize,
    },
    Queued {
        placement: VmDriverQueuePlacement,
        bytes: usize,
        queued_bytes: usize,
    },
    Dequeued {
        bytes: usize,
        queued_bytes: usize,
    },
    TimerSet {
        deadline_tick: u64,
    },
    TimerCancelled {
        was_pending: bool,
    },
    TimerFired {
        deadline_tick: u64,
    },
    CallbackSubmitted {
        callback_sequence: u64,
        bytes: usize,
    },
    CallbacksDrained {
        count: usize,
    },
    Closed {
        process_cleanup: bool,
        released_queue_bytes: usize,
        released_callbacks: usize,
        cancelled_timer: bool,
        released_environment_entries: usize,
    },
}

#[cfg(test)]
impl VmDriverTraceEventKind {
    const fn class(&self) -> VmDriverTraceClass {
        match self {
            Self::Opened { .. } | Self::ControllerChanged { .. } | Self::Closed { .. } => {
                VmDriverTraceClass::Lifecycle
            }
            Self::Command { .. } | Self::Queued { .. } | Self::Dequeued { .. } => {
                VmDriverTraceClass::Io
            }
            Self::TimerSet { .. } | Self::TimerCancelled { .. } | Self::TimerFired { .. } => {
                VmDriverTraceClass::Timer
            }
            Self::CallbackSubmitted { .. } | Self::CallbacksDrained { .. } => {
                VmDriverTraceClass::Callback
            }
        }
    }
}

/// One ordered VM driver diagnostic with exact actor attribution.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmDriverTraceEvent {
    pub(crate) sequence: u64,
    pub(crate) logical_tick: u64,
    pub(crate) driver: VmDriverId,
    pub(crate) owner: VmProcessId,
    pub(crate) caller: VmProcessId,
    pub(crate) kind: VmDriverTraceEventKind,
}

/// Position of the next event expected by one diagnostics consumer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmDriverTraceCursor(u64);

#[cfg(test)]
impl VmDriverTraceCursor {
    pub(crate) const fn from_position(position: u64) -> Self {
        Self(position)
    }

    pub(crate) const fn position(self) -> u64 {
        self.0
    }
}

/// Immutable bounded read from the driver diagnostic stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmDriverTraceRead {
    pub(crate) events: Vec<VmDriverTraceEvent>,
    pub(crate) next_cursor: VmDriverTraceCursor,
    pub(crate) dropped_events: u64,
}

/// Owner-local bounded trace recorder.
#[derive(Debug)]
pub(super) struct VmDriverTraceLog {
    config: VmDriverTraceConfig,
    events: VecDeque<VmDriverTraceEvent>,
    next_sequence: u64,
    dropped_events: u64,
}

impl Default for VmDriverTraceLog {
    fn default() -> Self {
        Self {
            config: VmDriverTraceConfig::disabled(),
            events: VecDeque::new(),
            next_sequence: 1,
            dropped_events: 0,
        }
    }
}

#[cfg(test)]
impl VmDriverTraceLog {
    pub(super) fn configure(&mut self, config: VmDriverTraceConfig) {
        self.config = config;
    }

    pub(super) const fn cursor(&self) -> VmDriverTraceCursor {
        VmDriverTraceCursor(self.next_sequence)
    }

    pub(super) fn oldest_cursor(&self) -> VmDriverTraceCursor {
        VmDriverTraceCursor(
            self.events
                .front()
                .map(|event| event.sequence)
                .unwrap_or(self.next_sequence),
        )
    }

    pub(super) fn record(
        &mut self,
        logical_tick: u64,
        driver: VmDriverId,
        owner: VmProcessId,
        caller: VmProcessId,
        kind: VmDriverTraceEventKind,
    ) {
        if !self.config.includes(kind.class()) {
            return;
        }
        let Some(next_sequence) = self.next_sequence.checked_add(1) else {
            self.dropped_events = self.dropped_events.saturating_add(1);
            return;
        };
        let event = VmDriverTraceEvent {
            sequence: self.next_sequence,
            logical_tick,
            driver,
            owner,
            caller,
            kind,
        };
        self.next_sequence = next_sequence;
        if self.events.len() == VM_DRIVER_TRACE_CAPACITY {
            self.events.pop_front();
            self.dropped_events = self.dropped_events.saturating_add(1);
        }
        self.events.push_back(event);
    }

    pub(super) fn record_close(
        &mut self,
        logical_tick: u64,
        caller: VmProcessId,
        process_cleanup: bool,
        report: &VmDriverCloseReport,
    ) {
        self.record(
            logical_tick,
            report.id,
            report.owner,
            caller,
            VmDriverTraceEventKind::Closed {
                process_cleanup,
                released_queue_bytes: report.released_queue_bytes,
                released_callbacks: report.released_callbacks,
                cancelled_timer: report.cancelled_timer,
                released_environment_entries: report.released_environment_entries,
            },
        );
    }

    pub(super) fn since(&self, cursor: VmDriverTraceCursor) -> Result<VmDriverTraceRead, String> {
        if cursor.0 > self.next_sequence {
            return Err(format!(
                "VM driver trace cursor {} exceeds next sequence {}",
                cursor.0, self.next_sequence
            ));
        }
        let oldest = self.oldest_cursor().0;
        if cursor.0 < oldest {
            return Err(format!(
                "VM driver trace cursor {} expired; oldest retained sequence is {oldest}",
                cursor.0
            ));
        }
        Ok(VmDriverTraceRead {
            events: self
                .events
                .iter()
                .filter(|event| event.sequence >= cursor.0)
                .cloned()
                .collect(),
            next_cursor: self.cursor(),
            dropped_events: self.dropped_events,
        })
    }
}
