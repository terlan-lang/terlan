//! Receiver-owned managed mailbox fragments.

use super::{ActorId, ManagedRoot, TvmRef};

/// One immutable graph copied into a receiver heap before mailbox publication.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedMailboxFragment {
    sender: ActorId,
    receiver: ActorId,
    fragment_id: u32,
    root: ManagedRoot,
    copied_objects: usize,
    copied_payload_bytes: usize,
    receiver_heap_bytes: usize,
}

impl ManagedMailboxFragment {
    /// Creates one validated receiver-owned mailbox fragment.
    pub(super) fn new(
        sender: ActorId,
        receiver: ActorId,
        fragment_id: u32,
        root: ManagedRoot,
        copied_objects: usize,
        copied_payload_bytes: usize,
        receiver_heap_bytes: usize,
    ) -> Self {
        Self {
            sender,
            receiver,
            fragment_id,
            root,
            copied_objects,
            copied_payload_bytes,
            receiver_heap_bytes,
        }
    }

    /// Returns the actor that initiated this immutable graph transfer.
    pub fn sender(&self) -> ActorId {
        self.sender
    }

    /// Returns the actor that exclusively owns the copied graph.
    pub fn receiver(&self) -> ActorId {
        self.receiver
    }

    /// Returns the receiver-local mailbox fragment identity.
    pub fn fragment_id(&self) -> u32 {
        self.fragment_id
    }

    /// Returns the receiver-owned root reference.
    pub fn root_reference(&self) -> TvmRef<()> {
        self.root.reference()
    }

    /// Returns the precise mailbox root used by actor-local collection.
    pub fn root(&self) -> &ManagedRoot {
        &self.root
    }

    /// Returns the mutable root slice required by moving collection.
    pub fn roots_mut(&mut self) -> &mut [ManagedRoot] {
        std::slice::from_mut(&mut self.root)
    }

    /// Returns the number of distinct graph objects copied from the sender.
    pub fn copied_objects(&self) -> usize {
        self.copied_objects
    }

    /// Returns source payload bytes copied without receiver alignment padding.
    pub fn copied_payload_bytes(&self) -> usize {
        self.copied_payload_bytes
    }

    /// Returns receiver heap bytes committed by the complete transfer.
    pub fn receiver_heap_bytes(&self) -> usize {
        self.receiver_heap_bytes
    }
}
