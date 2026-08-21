use std::collections::{BTreeMap, VecDeque};

use super::actor_directory::VmActorDirectory;
use super::ReplValue;

#[path = "process/actor_ownership.rs"]
mod actor_ownership;
#[path = "process/identity.rs"]
mod identity;
#[path = "process/parking.rs"]
mod parking;
mod snapshot;
#[path = "process/transfer.rs"]
pub(crate) mod transfer;

pub(crate) use identity::VmProcessId;
#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(crate) use snapshot::VmMailboxSnapshot;
pub(crate) use snapshot::VmProcessSnapshot;
#[cfg(test)]
pub(crate) use snapshot::VmProcessTableMetrics;

/// Local VM process execution state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmProcessState {
    Runnable,
    Blocked,
    Hibernated,
    Suspended(VmProcessResumeState),
    Exited(VmExitReason),
}

/// Process state restored when an explicit suspension ends.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmProcessResumeState {
    Runnable,
    Blocked,
    Hibernated,
}

/// Stable reason recorded when a VM process exits.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmExitReason {
    Normal,
    Error(String),
    Killed,
    ShutdownTimeout {
        timeout_ms: u64,
    },
    MemoryLimitExceeded {
        requested_bytes: usize,
        previous_bytes: usize,
        projected_bytes: usize,
    },
}

/// Stable process name registry error.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum VmProcessRegistryError {
    EmptyName,
    NameNotRegistered(String),
    MissingProcess(VmProcessId),
    ExitedProcess(VmProcessId),
    Conflict { name: String, existing: VmProcessId },
}

/// Stable process inspection error.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmProcessInspectionError {
    MissingProcess(VmProcessId),
}

/// Source identity for runtime inspection and diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmProcessSource {
    pub(crate) module: String,
    pub(crate) function: String,
    pub(crate) arity: usize,
    pub(crate) source_path: Option<String>,
}

/// Current VM execution location retained for inspection and debugging.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct VmProcessLocation {
    pub(crate) source: VmProcessSource,
    pub(crate) instruction_offset: usize,
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
            source_path: None,
        }
    }

    /// Attaches an explicit source path to runtime-owned source metadata.
    #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
    pub(crate) fn with_source_path(mut self, source_path: impl Into<String>) -> Self {
        self.source_path = Some(source_path.into());
        self
    }
}

impl VmProcessLocation {
    /// Renders one stable source-facing VM stack frame.
    #[cfg(test)]
    pub(crate) fn render(&self) -> String {
        let identity = format!(
            "{}.{}/{}",
            self.source.module, self.source.function, self.source.arity
        );
        match &self.source.source_path {
            Some(path) => format!(
                "{identity} [{}] @vm:{}",
                escape_source_path(path),
                self.instruction_offset
            ),
            None => format!("{identity} @vm:{}", self.instruction_offset),
        }
    }
}

#[cfg(test)]
fn escape_source_path(path: &str) -> String {
    path.chars().flat_map(char::escape_debug).collect()
}

/// Message stored in a VM process mailbox.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmMessage {
    pub(crate) id: u64,
    pub(crate) publication_sequence: u64,
    pub(crate) sender: VmProcessId,
    pub(crate) payload: ReplValue,
    /// Exact native boundary identity for typed AOT mailbox traffic.
    pub(crate) boundary_type: Option<crate::runtime::native_image::TvmBoundaryType>,
    /// Opaque receiver-owned graph token retained without public materialization.
    pub(crate) managed_fragment: Option<VmManagedMailboxToken>,
    pub(crate) accounted_bytes: usize,
    pub(crate) priority: VmMessagePriority,
}

/// VM mailbox handle for one precise graph owned by the managed shard runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmManagedMailboxToken {
    /// Nonzero shard-local fragment identity.
    fragment_id: u32,
    /// Actor that initiated graph transfer.
    sender: u64,
    /// Actor that exclusively owns the registered graph.
    receiver: u64,
    /// Receiver graph bytes plus fixed VM mailbox-token storage.
    accounted_bytes: usize,
}

/// Payload, ownership, accounting, and priority admitted to one mailbox.
struct VmMessageDelivery {
    payload: ReplValue,
    boundary_type: Option<crate::runtime::native_image::TvmBoundaryType>,
    managed_fragment: Option<VmManagedMailboxToken>,
    accounted_bytes: usize,
    priority: VmMessagePriority,
}

impl VmManagedMailboxToken {
    /// Creates one validated token with checked mailbox accounting.
    pub(crate) fn new(
        fragment_id: u32,
        sender: u64,
        receiver: u64,
        receiver_heap_bytes: usize,
    ) -> Result<Self, String> {
        if fragment_id == 0 || sender == 0 || receiver == 0 {
            return Err(
                "error[vm_managed_mailbox_token]: fragment and actor identities must be nonzero"
                    .to_string(),
            );
        }
        let accounted_bytes = receiver_heap_bytes
            .checked_add(std::mem::size_of::<Self>())
            .ok_or_else(|| {
                "error[vm_managed_mailbox_token]: mailbox accounting exceeds usize".to_string()
            })?;
        Ok(Self {
            fragment_id,
            sender,
            receiver,
            accounted_bytes,
        })
    }

    /// Returns the shard-local managed fragment identity.
    pub(crate) fn fragment_id(self) -> u32 {
        self.fragment_id
    }

    /// Returns the actor that initiated graph transfer.
    pub(crate) fn sender(self) -> u64 {
        self.sender
    }

    /// Returns the actor that exclusively owns the graph.
    pub(crate) fn receiver(self) -> u64 {
        self.receiver
    }

    /// Returns the logical mailbox charge for this token and graph.
    pub(crate) fn accounted_bytes(self) -> usize {
        self.accounted_bytes
    }
}

/// Ordering lane used by one VM mailbox message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmMessagePriority {
    Ordinary,
    Priority,
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
    execution_stack: Vec<VmProcessLocation>,
    mailbox: VecDeque<VmMessage>,
}

impl VmProcess {
    /// Returns the current VM execution location.
    pub(crate) fn current_location(&self) -> &VmProcessLocation {
        self.execution_stack
            .last()
            .expect("every process retains a root execution frame")
    }

    /// Updates the current source frame and VM instruction offset.
    pub(crate) fn set_current_location(
        &mut self,
        source: VmProcessSource,
        instruction_offset: usize,
    ) {
        *self
            .execution_stack
            .last_mut()
            .expect("every process retains a root execution frame") = VmProcessLocation {
            source,
            instruction_offset,
        };
    }

    /// Enters a called function and records the caller continuation atomically.
    #[cfg(test)]
    pub(crate) fn enter_execution_frame(
        &mut self,
        source: VmProcessSource,
        instruction_offset: usize,
        return_instruction_offset: usize,
    ) -> Result<(), String> {
        if matches!(self.state, VmProcessState::Exited(_)) {
            return Err("cannot enter an execution frame for an exited process".to_string());
        }
        self.execution_stack
            .last_mut()
            .expect("every process retains a root execution frame")
            .instruction_offset = return_instruction_offset;
        self.execution_stack.push(VmProcessLocation {
            source,
            instruction_offset,
        });
        Ok(())
    }

    /// Returns from the current function while preserving the root frame.
    #[cfg(test)]
    pub(crate) fn pop_execution_frame(&mut self) -> Result<VmProcessLocation, String> {
        if self.execution_stack.len() == 1 {
            return Err("cannot pop the root process execution frame".to_string());
        }
        Ok(self
            .execution_stack
            .pop()
            .expect("non-root execution stack contains a frame"))
    }

    /// Returns current-first execution frames for diagnostics.
    pub(crate) fn current_stacktrace(&self) -> Vec<VmProcessLocation> {
        self.execution_stack.iter().rev().cloned().collect()
    }

    /// Returns the number of queued mailbox messages.
    pub(crate) fn mailbox_len(&self) -> usize {
        self.mailbox.len()
    }

    /// Returns logical bytes retained by messages currently in the mailbox.
    #[cfg(test)]
    pub(crate) fn mailbox_accounted_bytes(&self) -> Result<usize, String> {
        self.mailbox.iter().try_fold(0usize, |total, message| {
            total.checked_add(message.accounted_bytes).ok_or_else(|| {
                format!(
                    "process {} mailbox accounted byte overflow",
                    self.pid.as_u64()
                )
            })
        })
    }

    /// Marks a runnable process as blocked.
    #[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
    pub(crate) fn block(&mut self) {
        if self.state == VmProcessState::Runnable {
            self.state = VmProcessState::Blocked;
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
        self.heap_bytes = 0;
        self.resource_handles.drain(..).collect()
    }

    /// Receives the oldest mailbox message.
    #[cfg(any(test, feature = "benchmark-tools"))]
    pub(crate) fn receive_next(&mut self) -> Option<VmMessage> {
        self.mailbox.pop_front()
    }

    /// Receives the first mailbox message accepted by the predicate.
    pub(crate) fn selective_receive(
        &mut self,
        predicate: impl FnMut(&VmMessage) -> bool,
    ) -> Option<VmMessage> {
        let index = self.mailbox.iter().position(predicate)?;
        self.mailbox.remove(index)
    }

    /// Integrates one complete MPSC fragment into priority/selective ordering.
    fn integrate_message(&mut self, message: VmMessage) {
        match message.priority {
            VmMessagePriority::Ordinary => self.mailbox.push_back(message),
            VmMessagePriority::Priority => {
                let insertion = self
                    .mailbox
                    .iter()
                    .position(|queued| queued.priority == VmMessagePriority::Ordinary)
                    .unwrap_or(self.mailbox.len());
                self.mailbox.insert(insertion, message);
            }
        }
        self.wake();
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
    processes: VmActorDirectory<VmProcess, VmMessage>,
    names: BTreeMap<String, VmProcessId>,
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
        let parent_process = self
            .processes
            .get(parent)
            .ok_or_else(|| format!("missing parent process {}", parent.as_u64()))?;
        if matches!(parent_process.state, VmProcessState::Exited(_)) {
            return Err(format!("parent process {} has exited", parent.as_u64()));
        }
        Ok(self.spawn(Some(parent), source))
    }

    /// Returns an immutable process record.
    pub(crate) fn get(&self, pid: VmProcessId) -> Option<&VmProcess> {
        self.processes.get(pid)
    }

    /// Returns a mutable process record for test fixture setup only.
    #[cfg(test)]
    pub(crate) fn get_mut(&mut self, pid: VmProcessId) -> Option<&mut VmProcess> {
        self.processes.get_mut_unowned(pid)
    }

    /// Returns all live process ids in deterministic allocation order.
    #[cfg(any(test, feature = "benchmark-tools"))]
    pub(crate) fn live_process_ids(&self) -> Vec<VmProcessId> {
        self.processes
            .iter()
            .filter(|(_, process)| !matches!(process.state, VmProcessState::Exited(_)))
            .map(|(pid, _)| pid)
            .collect()
    }

    /// Returns exited identities eligible for explicit postmortem reaping.
    pub(crate) fn exited_process_ids(&self) -> Vec<VmProcessId> {
        self.processes
            .iter()
            .filter(|(_, process)| matches!(process.state, VmProcessState::Exited(_)))
            .map(|(pid, _)| pid)
            .collect()
    }

    /// Returns whether a process identity currently names a live process.
    pub(crate) fn is_alive(&self, pid: VmProcessId) -> bool {
        self.processes
            .get(pid)
            .is_some_and(|process| !matches!(process.state, VmProcessState::Exited(_)))
    }

    /// Returns a stable read-only snapshot for runtime inspection.
    pub(crate) fn snapshot(
        &self,
        pid: VmProcessId,
    ) -> Result<VmProcessSnapshot, VmProcessInspectionError> {
        let process = self
            .processes
            .get(pid)
            .ok_or(VmProcessInspectionError::MissingProcess(pid))?;
        Ok(self.snapshot_process(process))
    }

    /// Returns the BEAM-facing process-info view for one live process.
    ///
    /// Missing and exited identities are intentionally indistinguishable at
    /// this boundary. Internal diagnostics continue to use `snapshot` so a
    /// completed process retains useful postmortem state.
    #[cfg(test)]
    pub(crate) fn live_snapshot(&self, pid: VmProcessId) -> Option<VmProcessSnapshot> {
        let process = self.processes.get(pid)?;
        if matches!(process.state, VmProcessState::Exited(_)) {
            return None;
        }
        Some(self.snapshot_process(process))
    }

    /// Returns every process snapshot in deterministic allocation order.
    ///
    /// Exited processes remain present so runtime inspection can explain a
    /// completed failure cascade without racing lifecycle cleanup.
    pub(crate) fn snapshots(&self) -> Vec<VmProcessSnapshot> {
        self.processes
            .values()
            .map(|process| self.snapshot_process(process))
            .collect()
    }

    fn snapshot_process(&self, process: &VmProcess) -> VmProcessSnapshot {
        VmProcessSnapshot {
            pid: process.pid,
            parent: process.parent,
            source: process.source.clone(),
            state: process.state.clone(),
            reductions: process.reductions,
            heap_bytes: process.heap_bytes,
            mailbox_messages: process.mailbox_len(),
            cancellation_requested: process.cancellation_requested,
            resource_handles: process.resource_handles.clone(),
            registered_names: self.names_for_process(process.pid),
            current_location: process.current_location().clone(),
            current_stacktrace: process.current_stacktrace(),
        }
    }

    /// Returns deterministic aggregate ownership for leak and soak checks.
    #[cfg(test)]
    pub(crate) fn metrics(&self) -> VmProcessTableMetrics {
        let mut metrics = VmProcessTableMetrics {
            total_processes: self.processes.len(),
            ..VmProcessTableMetrics::default()
        };
        for process in self.processes.values() {
            if matches!(process.state, VmProcessState::Exited(_)) {
                metrics.exited_processes += 1;
            } else {
                metrics.live_processes += 1;
            }
            metrics.mailbox_messages += process.mailbox_len();
            metrics.heap_bytes = metrics.heap_bytes.saturating_add(process.heap_bytes);
            metrics.resource_handles += process.resource_handles.len();
        }
        metrics
    }

    /// Registers a stable process name.
    #[cfg(test)]
    pub(crate) fn register_name(
        &mut self,
        name: impl Into<String>,
        pid: VmProcessId,
    ) -> Result<(), VmProcessRegistryError> {
        let name = name.into();
        if name.trim().is_empty() {
            return Err(VmProcessRegistryError::EmptyName);
        }
        self.ensure_live_process(pid)?;
        match self.names.get(&name).copied() {
            Some(existing) if existing == pid => Ok(()),
            Some(existing) => Err(VmProcessRegistryError::Conflict { name, existing }),
            None => {
                self.names.insert(name, pid);
                Ok(())
            }
        }
    }

    /// Looks up a registered process name.
    #[cfg(test)]
    pub(crate) fn lookup_name(&self, name: &str) -> Option<VmProcessId> {
        self.names.get(name).copied()
    }

    /// Returns the number of registered process names.
    #[cfg(test)]
    pub(crate) fn registered_name_count(&self) -> usize {
        self.names.len()
    }

    /// Returns all registered names in deterministic lexical order.
    #[cfg(test)]
    pub(crate) fn registered_names(&self) -> Vec<String> {
        self.names.keys().cloned().collect()
    }

    /// Returns all names registered to one process in deterministic order.
    pub(crate) fn names_for_process(&self, pid: VmProcessId) -> Vec<String> {
        self.names
            .iter()
            .filter(|&(_, registered_pid)| *registered_pid == pid)
            .map(|(name, _)| name.clone())
            .collect()
    }

    /// Removes one registered name and returns its process owner.
    #[cfg(test)]
    pub(crate) fn unregister_name(
        &mut self,
        name: &str,
    ) -> Result<VmProcessId, VmProcessRegistryError> {
        self.names
            .remove(name)
            .ok_or_else(|| VmProcessRegistryError::NameNotRegistered(name.to_string()))
    }

    /// Removes every registered name for one existing process.
    #[cfg(test)]
    pub(crate) fn unregister_process_names(
        &mut self,
        pid: VmProcessId,
    ) -> Result<Vec<String>, VmProcessRegistryError> {
        if !self.processes.contains(pid) {
            return Err(VmProcessRegistryError::MissingProcess(pid));
        }
        Ok(self.remove_registered_names(pid))
    }

    /// Sends a message from one VM process to another.
    #[cfg(any(test, feature = "benchmark-tools"))]
    pub(crate) fn send(
        &mut self,
        sender: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
    ) -> Result<u64, String> {
        self.send_accounted(sender, recipient, payload, 0)
    }

    /// Delivers a VM-owned system message from a known process identity.
    ///
    /// Failure signals may originate after the source process has transitioned
    /// to exited state. User actor sends must continue to use `send`.
    pub(crate) fn send_system_message(
        &mut self,
        origin: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
    ) -> Result<u64, String> {
        if !self.processes.contains(origin) {
            return Err(format!("missing system message origin {}", origin.as_u64()));
        }
        self.validate_recipient(recipient)?;
        self.enqueue_message(
            origin,
            recipient,
            VmMessageDelivery {
                payload,
                boundary_type: None,
                managed_fragment: None,
                accounted_bytes: 0,
                priority: VmMessagePriority::Ordinary,
            },
        )
    }

    /// Delivers a priority system message ahead of ordinary mailbox traffic.
    pub(crate) fn send_priority_system_message(
        &mut self,
        origin: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
    ) -> Result<u64, String> {
        if !self.processes.contains(origin) {
            return Err(format!("missing system message origin {}", origin.as_u64()));
        }
        self.validate_recipient(recipient)?;
        self.enqueue_message(
            origin,
            recipient,
            VmMessageDelivery {
                payload,
                boundary_type: None,
                managed_fragment: None,
                accounted_bytes: 0,
                priority: VmMessagePriority::Priority,
            },
        )
    }

    /// Validates one local message route without mutating either process.
    pub(crate) fn validate_send(
        &self,
        sender: VmProcessId,
        recipient: VmProcessId,
    ) -> Result<(), String> {
        self.validate_sender(sender)?;
        self.validate_recipient(recipient)
    }

    /// Validates a user-message sender before resolving its destination.
    pub(crate) fn validate_sender(&self, sender: VmProcessId) -> Result<(), String> {
        let sender_process = self
            .processes
            .get(sender)
            .ok_or_else(|| format!("missing sender process {}", sender.as_u64()))?;
        if matches!(sender_process.state, VmProcessState::Exited(_)) {
            return Err(format!("sender process {} has exited", sender.as_u64()));
        }
        Ok(())
    }

    /// Sends a message carrying an explicit VM logical-heap charge.
    pub(crate) fn send_accounted(
        &mut self,
        sender: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
        accounted_bytes: usize,
    ) -> Result<u64, String> {
        self.validate_send(sender, recipient)?;

        self.enqueue_message(
            sender,
            recipient,
            VmMessageDelivery {
                payload,
                boundary_type: None,
                managed_fragment: None,
                accounted_bytes,
                priority: VmMessagePriority::Ordinary,
            },
        )
    }

    /// Sends an explicitly priority message carrying a logical-heap charge.
    #[cfg(test)]
    pub(crate) fn send_priority_accounted(
        &mut self,
        sender: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
        accounted_bytes: usize,
    ) -> Result<u64, String> {
        self.validate_send(sender, recipient)?;

        self.enqueue_message(
            sender,
            recipient,
            VmMessageDelivery {
                payload,
                boundary_type: None,
                managed_fragment: None,
                accounted_bytes,
                priority: VmMessagePriority::Priority,
            },
        )
    }

    /// Sends a mailbox value carrying an exact native boundary identity.
    pub(crate) fn send_typed_accounted(
        &mut self,
        sender: VmProcessId,
        recipient: VmProcessId,
        payload: ReplValue,
        boundary_type: crate::runtime::native_image::TvmBoundaryType,
        accounted_bytes: usize,
    ) -> Result<u64, String> {
        self.validate_send(sender, recipient)?;
        self.enqueue_message(
            sender,
            recipient,
            VmMessageDelivery {
                payload,
                boundary_type: Some(boundary_type),
                managed_fragment: None,
                accounted_bytes,
                priority: VmMessagePriority::Ordinary,
            },
        )
    }

    /// Sends one receiver-owned managed graph with exact native type identity.
    pub(crate) fn send_typed_managed_accounted(
        &mut self,
        sender: VmProcessId,
        recipient: VmProcessId,
        fragment: VmManagedMailboxToken,
        boundary_type: crate::runtime::native_image::TvmBoundaryType,
        accounted_bytes: usize,
    ) -> Result<u64, String> {
        self.validate_send(sender, recipient)?;
        self.enqueue_message(
            sender,
            recipient,
            VmMessageDelivery {
                payload: ReplValue::Unit,
                boundary_type: Some(boundary_type),
                managed_fragment: Some(fragment),
                accounted_bytes,
                priority: VmMessagePriority::Ordinary,
            },
        )
    }

    fn validate_recipient(&self, recipient: VmProcessId) -> Result<(), String> {
        let recipient_process = self
            .processes
            .get(recipient)
            .ok_or_else(|| format!("missing recipient process {}", recipient.as_u64()))?;
        if matches!(recipient_process.state, VmProcessState::Exited(_)) {
            return Err(format!(
                "recipient process {} has exited",
                recipient.as_u64()
            ));
        }
        Ok(())
    }

    fn enqueue_message(
        &mut self,
        sender: VmProcessId,
        recipient: VmProcessId,
        delivery: VmMessageDelivery,
    ) -> Result<u64, String> {
        let VmMessageDelivery {
            payload,
            boundary_type,
            managed_fragment,
            accounted_bytes,
            priority,
        } = delivery;
        let message_id = self.next_message_id.saturating_add(1);
        self.processes
            .publish_fragment(
                recipient,
                VmMessage {
                    id: message_id,
                    publication_sequence: 0,
                    sender,
                    payload,
                    boundary_type,
                    managed_fragment,
                    accounted_bytes,
                    priority,
                },
            )
            .map_err(|error| format!("actor mailbox publication error: {error:?}"))?;
        self.next_message_id = message_id;
        self.integrate_process_mailbox_if_unowned(recipient)?;
        Ok(message_id)
    }

    fn remove_registered_names(&mut self, pid: VmProcessId) -> Vec<String> {
        let names = self.names_for_process(pid);
        for name in &names {
            self.names.remove(name);
        }
        names
    }

    #[cfg(test)]
    fn ensure_live_process(&self, pid: VmProcessId) -> Result<(), VmProcessRegistryError> {
        let process = self
            .processes
            .get(pid)
            .ok_or(VmProcessRegistryError::MissingProcess(pid))?;
        if matches!(process.state, VmProcessState::Exited(_)) {
            return Err(VmProcessRegistryError::ExitedProcess(pid));
        }
        Ok(())
    }

    fn spawn(&mut self, parent: Option<VmProcessId>, source: VmProcessSource) -> VmProcessId {
        self.next_pid = self.next_pid.saturating_add(1);
        let pid = VmProcessId::from_allocated(self.next_pid);
        let root_location = VmProcessLocation {
            source: source.clone(),
            instruction_offset: 0,
        };
        self.processes
            .insert(
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
                    execution_stack: vec![root_location],
                    mailbox: VecDeque::new(),
                },
            )
            .expect("new process identity must have a free actor directory slot");
        pid
    }
}

#[cfg(test)]
#[path = "process_test.rs"]
#[cfg(test)]
mod process_test;

#[cfg(test)]
#[path = "process_mailbox_storage_parity_test.rs"]
#[cfg(test)]
mod process_mailbox_storage_parity_test;

#[cfg(test)]
#[path = "process_inspection_test.rs"]
#[cfg(test)]
mod process_inspection_test;

#[cfg(test)]
#[path = "process_registry_test.rs"]
#[cfg(test)]
mod process_registry_test;

#[cfg(test)]
#[path = "process_location_test.rs"]
#[cfg(test)]
mod process_location_test;

#[cfg(test)]
#[path = "process_unicode_source_path_test.rs"]
#[cfg(test)]
mod process_unicode_source_path_test;

#[cfg(test)]
#[path = "process_transfer_test.rs"]
#[cfg(test)]
mod process_transfer_test;

#[cfg(test)]
#[path = "process_environment_parity_test.rs"]
#[cfg(test)]
mod process_environment_parity_test;
