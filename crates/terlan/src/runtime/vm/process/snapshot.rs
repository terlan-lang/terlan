//! Read-only process and mailbox inspection records.

use crate::runtime::vm::ReplValue;

use super::{VmMessagePriority, VmProcessId, VmProcessLocation, VmProcessSource, VmProcessState};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmProcessTableMetrics {
    pub(crate) total_processes: usize,
    pub(crate) live_processes: usize,
    pub(crate) exited_processes: usize,
    pub(crate) mailbox_messages: usize,
    pub(crate) heap_bytes: usize,
    pub(crate) resource_handles: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmProcessSnapshot {
    pub(crate) pid: VmProcessId,
    pub(crate) parent: Option<VmProcessId>,
    pub(crate) source: VmProcessSource,
    pub(crate) state: VmProcessState,
    pub(crate) reductions: u64,
    pub(crate) heap_bytes: usize,
    pub(crate) mailbox_messages: usize,
    pub(crate) cancellation_requested: bool,
    pub(crate) resource_handles: Vec<String>,
    pub(crate) registered_names: Vec<String>,
    pub(crate) current_location: VmProcessLocation,
    pub(crate) current_stacktrace: Vec<VmProcessLocation>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmMailboxMessageSnapshot {
    pub(crate) id: u64,
    pub(crate) publication_sequence: u64,
    pub(crate) sender: VmProcessId,
    pub(crate) payload: ReplValue,
    pub(crate) managed: bool,
    pub(crate) accounted_bytes: usize,
    pub(crate) priority: VmMessagePriority,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmMailboxSnapshot {
    pub(crate) process: VmProcessId,
    pub(crate) selective_receive_cursor: usize,
    pub(crate) messages: Vec<VmMailboxMessageSnapshot>,
    pub(crate) omitted_messages: usize,
}
