//! Precise managed roots, native stack maps, and owned continuations.

use std::collections::{BTreeMap, BTreeSet};

use super::{ActorId, ManagedMemoryError, TvmRef};

/// Runtime location that owns one precise actor-local root.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RootLocation {
    NativeStack { function_id: u64, slot: u16 },
    Continuation { continuation_id: u64, slot: u16 },
    Mailbox { fragment: u32, slot: u16 },
    RuntimeFrame { frame_id: u64, slot: u16 },
    ActorState { slot: u16 },
}

/// One mutable precise root supplied to actor-local collection.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedRoot {
    owner: ActorId,
    location: RootLocation,
    reference: TvmRef<()>,
}

impl ManagedRoot {
    /// Creates one root with explicit actor ownership and runtime location.
    pub fn new(owner: ActorId, location: RootLocation, reference: TvmRef<()>) -> Self {
        Self {
            owner,
            location,
            reference,
        }
    }

    /// Returns the actor that owns this root.
    pub fn owner(&self) -> ActorId {
        self.owner
    }

    /// Returns the precise runtime location of this root.
    pub fn location(&self) -> &RootLocation {
        &self.location
    }

    /// Returns the current relocatable managed reference.
    pub fn reference(&self) -> TvmRef<()> {
        self.reference
    }

    /// Replaces a root after successful relocation.
    pub(super) fn relocate(&mut self, reference: TvmRef<()>) {
        self.reference = reference;
    }
}

/// Reference classification emitted for one native stack slot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StackRootKind {
    ActorLocal,
    Shared,
    Derived { base_slot: u16, byte_offset: u32 },
    Borrowed,
}

/// One compact precise root entry at a native safepoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StackMapEntry {
    pub slot: u16,
    pub kind: StackRootKind,
}

/// Precise stack map for one finalized native function safepoint.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StackMapRecord {
    function_id: u64,
    safepoint_id: u32,
    frame_words: u16,
    entries: Box<[StackMapEntry]>,
}

impl StackMapRecord {
    /// Creates a validated compact stack map.
    pub fn new(
        function_id: u64,
        safepoint_id: u32,
        frame_words: u16,
        entries: Vec<StackMapEntry>,
    ) -> Result<Self, ManagedMemoryError> {
        if function_id == 0 || frame_words == 0 || entries.is_empty() {
            return Err(ManagedMemoryError::InvalidStackMap);
        }
        let mut slots = BTreeSet::new();
        for entry in &entries {
            if entry.slot >= frame_words || !slots.insert(entry.slot) {
                return Err(ManagedMemoryError::InvalidStackMap);
            }
            if matches!(entry.kind, StackRootKind::Borrowed) {
                return Err(ManagedMemoryError::BorrowedValueAtSafepoint);
            }
        }
        for entry in &entries {
            if let StackRootKind::Derived { base_slot, .. } = entry.kind {
                if base_slot == entry.slot || !slots.contains(&base_slot) {
                    return Err(ManagedMemoryError::InvalidStackMap);
                }
            }
        }
        Ok(Self {
            function_id,
            safepoint_id,
            frame_words,
            entries: entries.into_boxed_slice(),
        })
    }

    /// Returns the finalized native function identity.
    pub fn function_id(&self) -> u64 {
        self.function_id
    }

    /// Returns the function-local safepoint identity.
    pub fn safepoint_id(&self) -> u32 {
        self.safepoint_id
    }

    /// Returns the native frame size measured in pointer-width words.
    pub fn frame_words(&self) -> u16 {
        self.frame_words
    }

    /// Returns the precise root entries in canonical slot order.
    pub fn entries(&self) -> &[StackMapEntry] {
        &self.entries
    }
}

/// Immutable lookup table of all admitted native safepoint maps.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct StackMapTable {
    records: BTreeMap<(u64, u32), StackMapRecord>,
}

impl StackMapTable {
    /// Builds a table while rejecting duplicate safepoint identities.
    pub fn new(records: Vec<StackMapRecord>) -> Result<Self, ManagedMemoryError> {
        let mut table = BTreeMap::new();
        for record in records {
            let key = (record.function_id(), record.safepoint_id());
            if table.insert(key, record).is_some() {
                return Err(ManagedMemoryError::InvalidStackMap);
            }
        }
        Ok(Self { records: table })
    }

    /// Requires the precise map for one native safepoint.
    pub fn require(
        &self,
        function_id: u64,
        safepoint_id: u32,
    ) -> Result<&StackMapRecord, ManagedMemoryError> {
        self.records
            .get(&(function_id, safepoint_id))
            .ok_or(ManagedMemoryError::MissingStackMap)
    }

    /// Returns the number of admitted safepoints.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Reports whether the table contains no safepoints.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Owned managed references captured when native execution parks.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedContinuation {
    owner: ActorId,
    continuation_id: u64,
    captures: Vec<ManagedRoot>,
}

impl ManagedContinuation {
    /// Captures actor-local roots using a precise continuation slot layout.
    pub fn capture(
        owner: ActorId,
        continuation_id: u64,
        references: Vec<TvmRef<()>>,
    ) -> Result<Self, ManagedMemoryError> {
        if continuation_id == 0 || references.is_empty() || references.len() > u16::MAX as usize {
            return Err(ManagedMemoryError::InvalidContinuation);
        }
        let captures = references
            .into_iter()
            .enumerate()
            .map(|(slot, reference)| {
                ManagedRoot::new(
                    owner,
                    RootLocation::Continuation {
                        continuation_id,
                        slot: slot as u16,
                    },
                    reference,
                )
            })
            .collect();
        Ok(Self {
            owner,
            continuation_id,
            captures,
        })
    }

    /// Returns the actor that owns every capture.
    pub fn owner(&self) -> ActorId {
        self.owner
    }

    /// Returns the stable continuation entry identity.
    pub fn continuation_id(&self) -> u64 {
        self.continuation_id
    }

    /// Returns captured roots for precise collection and relocation.
    pub fn captures(&self) -> &[ManagedRoot] {
        &self.captures
    }

    /// Returns mutable captured roots to the owning actor collector.
    pub fn captures_mut(&mut self) -> &mut [ManagedRoot] {
        &mut self.captures
    }
}
