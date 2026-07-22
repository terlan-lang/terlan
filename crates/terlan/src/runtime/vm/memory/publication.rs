//! Typed ordering boundary between mailbox insertion and scheduler wakeup.

use super::VmMemoryPressureDecision;
use crate::runtime::vm::process::VmProcessId;

/// Result of one mailbox send governed by VM memory pressure.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VmAccountedMessageSend {
    /// Receipt present only after complete queue publication.
    pub(crate) publication: Option<VmMailboxPublication>,
    /// Logical memory decision made before queue insertion.
    pub(crate) pressure: VmMemoryPressureDecision,
}

impl VmAccountedMessageSend {
    /// Returns the message identity only after complete mailbox publication.
    pub(crate) fn published_message_id(&self) -> Option<u64> {
        self.publication.map(VmMailboxPublication::message_id)
    }
}

/// Proof that a complete message is present before scheduler wakeup.
///
/// The process table and managed runtime are exclusively borrowed while the
/// message and any receiver-owned graph are installed. Future concurrent queue
/// implementations must create this receipt with release publication and
/// consume it with acquire ordering before reading the message graph.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmMailboxPublication {
    /// Allocated mailbox message identity.
    message_id: u64,
    /// Exact recipient whose queue contains the complete message.
    recipient: VmProcessId,
    /// Logical bytes already charged to the recipient.
    accounted_bytes: usize,
}

impl VmMailboxPublication {
    /// Records successful queue insertion after all payload state is complete.
    pub(super) fn after_enqueue(
        message_id: u64,
        recipient: VmProcessId,
        accounted_bytes: usize,
    ) -> Self {
        Self {
            message_id,
            recipient,
            accounted_bytes,
        }
    }

    /// Returns the fully published message identity.
    pub(crate) const fn message_id(self) -> u64 {
        self.message_id
    }

    /// Returns the process whose mailbox owns the publication.
    pub(crate) const fn recipient(self) -> VmProcessId {
        self.recipient
    }

    /// Returns the logical bytes committed before publication.
    pub(crate) const fn accounted_bytes(self) -> usize {
        self.accounted_bytes
    }
}
