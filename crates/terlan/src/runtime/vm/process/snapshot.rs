//! Read-only process and mailbox inspection records.

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use crate::runtime::vm::ReplValue;

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use super::VmMessagePriority;
use super::{VmProcessId, VmProcessLocation, VmProcessSource, VmProcessState};
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
use super::{VmProcessInspectionError, VmProcessTable};

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
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
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
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) struct VmMailboxSnapshot {
    pub(crate) process: VmProcessId,
    pub(crate) selective_receive_cursor: usize,
    pub(crate) messages: Vec<VmMailboxMessageSnapshot>,
    pub(crate) omitted_messages: usize,
}

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
impl VmProcessTable {
    /// Captures at most `limit` messages without changing selective-receive order.
    pub(crate) fn mailbox_snapshot(
        &self,
        pid: VmProcessId,
        limit: usize,
    ) -> Result<VmMailboxSnapshot, VmProcessInspectionError> {
        let process = self
            .processes
            .get(pid)
            .ok_or(VmProcessInspectionError::MissingProcess(pid))?;
        let messages = process
            .mailbox
            .iter()
            .take(limit)
            .map(|message| VmMailboxMessageSnapshot {
                id: message.id,
                publication_sequence: message.publication_sequence,
                sender: message.sender,
                payload: message.payload.clone(),
                managed: message.managed_fragment.is_some(),
                accounted_bytes: message.accounted_bytes,
                priority: message.priority,
            })
            .collect();
        Ok(VmMailboxSnapshot {
            process: pid,
            selective_receive_cursor: 0,
            messages,
            omitted_messages: process.mailbox.len().saturating_sub(limit),
        })
    }
}
