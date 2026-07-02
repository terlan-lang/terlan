#![allow(dead_code)]

use std::collections::{BTreeMap, VecDeque};

use super::ReplValue;

/// VM-owned process identifier.
///
/// Inputs:
/// - Monotonic runtime allocation.
///
/// Output:
/// - Stable process id value used by local VM tables.
///
/// Transformation:
/// - Keeps process identity independent from OTP pid syntax or any host
///   runtime handle.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmProcessId(u64);

impl VmProcessId {
    /// Returns the numeric process id.
    pub(crate) fn as_u64(self) -> u64 {
        self.0
    }

    /// Creates a process id for adversarial VM runtime tests.
    #[cfg(test)]
    pub(crate) fn from_raw_for_test(value: u64) -> Self {
        Self(value)
    }
}

/// Local VM process execution state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmProcessState {
    Runnable,
    Blocked,
    Exited(VmExitReason),
}

/// Stable reason recorded when a VM process exits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmExitReason {
    Normal,
    Error(String),
    Killed,
}

/// Source identity for runtime inspection and diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmProcessSource {
    pub(crate) module: String,
    pub(crate) function: String,
    pub(crate) arity: usize,
}

impl VmProcessSource {
    /// Creates source identity metadata for a process.
    pub(crate) fn new(
        module: impl Into<String>,
        function: impl Into<String>,
        arity: usize,
    ) -> Self {
        Self {
            module: module.into(),
            function: function.into(),
            arity,
        }
    }
}

/// Message stored in a VM process mailbox.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmMessage {
    pub(crate) id: u64,
    pub(crate) sender: VmProcessId,
    pub(crate) payload: ReplValue,
}

/// VM-owned local process record.
///
/// Inputs:
/// - Source identity, parent relation, messages, reductions, and owned
///   resources.
///
/// Output:
/// - Runtime-inspectable process state.
///
/// Transformation:
/// - Centralizes process bookkeeping that was previously implicit in the host
///   VM so Terlan can own scheduling, mailbox, and cleanup semantics.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmProcess {
    pub(crate) pid: VmProcessId,
    pub(crate) parent: Option<VmProcessId>,
    pub(crate) source: VmProcessSource,
    pub(crate) state: VmProcessState,
    pub(crate) reductions: u64,
    pub(crate) heap_bytes: usize,
    pub(crate) cancellation_requested: bool,
    pub(crate) resource_handles: Vec<String>,
    mailbox: VecDeque<VmMessage>,
}

impl VmProcess {
    /// Returns the number of queued mailbox messages.
    pub(crate) fn mailbox_len(&self) -> usize {
        self.mailbox.len()
    }

    /// Marks a runnable process as blocked.
    pub(crate) fn block(&mut self) {
        if self.state == VmProcessState::Runnable {
            self.state = VmProcessState::Blocked;
        }
    }

    /// Wakes a blocked process.
    pub(crate) fn wake(&mut self) {
        if self.state == VmProcessState::Blocked {
            self.state = VmProcessState::Runnable;
        }
    }

    /// Requests cooperative cancellation for the process.
    pub(crate) fn request_cancellation(&mut self) {
        self.cancellation_requested = true;
    }

    /// Charges scheduler reductions to the process.
    pub(crate) fn charge_reductions(&mut self, reductions: u64) {
        self.reductions = self.reductions.saturating_add(reductions);
    }

    /// Adds a VM-owned resource handle to process ownership.
    pub(crate) fn add_resource_handle(&mut self, handle: impl Into<String>) {
        self.resource_handles.push(handle.into());
    }

    /// Removes one resource handle from process ownership.
    pub(crate) fn remove_resource_handle(&mut self, handle: &str) {
        if let Some(index) = self
            .resource_handles
            .iter()
            .position(|resource| resource == handle)
        {
            self.resource_handles.remove(index);
        }
    }

    /// Exits the process and returns handles that must be cleaned up.
    pub(crate) fn exit(&mut self, reason: VmExitReason) -> Vec<String> {
        self.state = VmProcessState::Exited(reason);
        self.mailbox.clear();
        self.resource_handles.drain(..).collect()
    }

    /// Receives the oldest mailbox message.
    pub(crate) fn receive_next(&mut self) -> Option<VmMessage> {
        self.mailbox.pop_front()
    }

    /// Receives the first mailbox message accepted by the predicate.
    pub(crate) fn selective_receive(
        &mut self,
        mut predicate: impl FnMut(&VmMessage) -> bool,
    ) -> Option<VmMessage> {
        let index = self.mailbox.iter().position(|message| predicate(message))?;
        self.mailbox.remove(index)
    }
}

/// Local VM process table.
///
/// Inputs:
/// - Spawn requests and message sends.
///
/// Output:
/// - Process records with stable ids and ordered mailboxes.
///
/// Transformation:
/// - Allocates Terlan VM process identity and enforces local message delivery
///   rules without relying on OTP process ids or mailboxes.
#[derive(Debug, Default)]
pub(crate) struct VmProcessTable {
    next_pid: u64,
    next_message_id: u64,
    processes: BTreeMap<VmProcessId, VmProcess>,
}

impl VmProcessTable {
    /// Spawns a root VM process.
    pub(crate) fn spawn_root(&mut self, source: VmProcessSource) -> VmProcessId {
        self.spawn(None, source)
    }

    /// Spawns a child VM process.
    pub(crate) fn spawn_child(
        &mut self,
        parent: VmProcessId,
        source: VmProcessSource,
    ) -> Result<VmProcessId, String> {
        if !self.processes.contains_key(&parent) {
            return Err(format!("missing parent process {}", parent.as_u64()));
        }
        Ok(self.spawn(Some(parent), source))
    }

    /// Returns an immutable process record.
    pub(crate) fn get(&self, pid: VmProcessId) -> Option<&VmProcess> {
        self.processes.get(&pid)
    }

    /// Returns a mutable process record.
    pub(crate) fn get_mut(&mut self, pid: VmProcessId) -> Option<&mut VmProcess> {
        self.processes.get_mut(&pid)
    }

    /// Sends a message from one VM process to another.
    pub(crate) fn send(
        &mut self,
        sender: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
    ) -> Result<u64, String> {
        if !self.processes.contains_key(&sender) {
            return Err(format!("missing sender process {}", sender.as_u64()));
        }

        let recipient_process = self
            .processes
            .get(&recipient)
            .ok_or_else(|| format!("missing recipient process {}", recipient.as_u64()))?;
        if matches!(recipient_process.state, VmProcessState::Exited(_)) {
            return Err(format!(
                "recipient process {} has exited",
                recipient.as_u64()
            ));
        }

        self.next_message_id = self.next_message_id.saturating_add(1);
        let message_id = self.next_message_id;
        let recipient_process = self
            .processes
            .get_mut(&recipient)
            .expect("recipient process was checked before message allocation");
        recipient_process.mailbox.push_back(VmMessage {
            id: message_id,
            sender,
            payload,
        });
        recipient_process.wake();
        Ok(message_id)
    }

    /// Exits a process and returns resources that must be cleaned up.
    pub(crate) fn exit_process(
        &mut self,
        pid: VmProcessId,
        reason: VmExitReason,
    ) -> Result<Vec<String>, String> {
        let process = self
            .processes
            .get_mut(&pid)
            .ok_or_else(|| format!("missing process {}", pid.as_u64()))?;
        Ok(process.exit(reason))
    }

    fn spawn(&mut self, parent: Option<VmProcessId>, source: VmProcessSource) -> VmProcessId {
        self.next_pid = self.next_pid.saturating_add(1);
        let pid = VmProcessId(self.next_pid);
        self.processes.insert(
            pid,
            VmProcess {
                pid,
                parent,
                source,
                state: VmProcessState::Runnable,
                reductions: 0,
                heap_bytes: 0,
                cancellation_requested: false,
                resource_handles: Vec::new(),
                mailbox: VecDeque::new(),
            },
        );
        pid
    }
}

#[cfg(test)]
#[path = "process_test.rs"]
mod process_test;
