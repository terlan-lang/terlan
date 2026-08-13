use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Mutex;

use super::process::VmProcessId;

#[path = "actor_directory/mailbox.rs"]
mod mailbox;
#[path = "actor_directory/migration.rs"]
mod migration;
#[path = "actor_directory/state.rs"]
mod state;
#[path = "actor_directory/transfer.rs"]
mod transfer;

use mailbox::VmActorMailbox;
pub(crate) use mailbox::VmMailboxWake;
#[cfg(test)]
pub(crate) use mailbox::ACTOR_MAILBOX_CAPACITY;
pub(crate) use migration::VmActorMigrationStamp;
use state::{
    next_generation, ownership_race_error, pack_state, unpack_state, validate_token, VmActorState,
};

const LIFECYCLE_BITS: u32 = 4;
const GENERATION_BITS: u32 = 20;
const GENERATION_MASK: u64 = (1_u64 << GENERATION_BITS) - 1;
const OWNER_SHIFT: u32 = LIFECYCLE_BITS + GENERATION_BITS + GENERATION_BITS;

/// Atomic lifecycle states shared by actor lookup, scheduling, and retirement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum VmActorLifecycle {
    /// The actor has a runnable queue entry.
    Queued = 1,
    /// A scheduler owns the actor's mutable state.
    Executing = 2,
    /// The actor released ownership and remains runnable.
    Yielding = 3,
    /// The actor is waiting for an external publication.
    Parked = 4,
    /// Ownership is being transferred between schedulers.
    Migrating = 5,
    /// The actor is running its terminal cleanup pipeline.
    Exiting = 6,
    /// The actor is an inspectable tombstone.
    Retired = 7,
    /// The actor cell has been removed from its directory slot.
    Reclaimed = 8,
}

impl VmActorLifecycle {
    /// Decodes a validated lifecycle tag from the packed actor state word.
    fn from_word(word: u64) -> Result<Self, VmActorDirectoryError> {
        match (word & ((1_u64 << LIFECYCLE_BITS) - 1)) as u8 {
            1 => Ok(Self::Queued),
            2 => Ok(Self::Executing),
            3 => Ok(Self::Yielding),
            4 => Ok(Self::Parked),
            5 => Ok(Self::Migrating),
            6 => Ok(Self::Exiting),
            7 => Ok(Self::Retired),
            8 => Ok(Self::Reclaimed),
            value => Err(VmActorDirectoryError::CorruptLifecycle(value)),
        }
    }
}

/// Generation-qualified reference to one actor directory cell.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmActorHandle {
    pid: VmProcessId,
    slot: u32,
    actor_generation: u64,
}

impl VmActorHandle {
    /// Returns the stable runtime process identity carried by this handle.
    pub(crate) fn pid(self) -> VmProcessId {
        self.pid
    }

    /// Returns the slot generation used to reject stale handles.
    pub(crate) fn actor_generation(self) -> u64 {
        self.actor_generation
    }
}

/// Exclusive permission to mutate one actor on behalf of one scheduler.
#[derive(Debug, Eq, PartialEq)]
pub(crate) struct VmActorMutatorToken {
    handle: VmActorHandle,
    owner: u64,
    owner_generation: u64,
    lifecycle: VmActorLifecycle,
}

impl VmActorMutatorToken {
    /// Returns the actor owned by this token.
    pub(crate) fn handle(&self) -> VmActorHandle {
        self.handle
    }

    /// Returns the scheduler identity that acquired this token.
    #[cfg(all(test, not(feature = "multicore-tsan-harness")))]
    pub(crate) fn owner(&self) -> u64 {
        self.owner
    }

    /// Returns the ownership generation used to reject stale releases.
    pub(crate) fn owner_generation(&self) -> u64 {
        self.owner_generation
    }

    /// Duplicates a token so tests can prove stale-generation rejection.
    #[cfg(all(test, not(feature = "multicore-tsan-harness")))]
    fn duplicate_for_test(&self) -> Self {
        Self {
            handle: self.handle,
            owner: self.owner,
            owner_generation: self.owner_generation,
            lifecycle: self.lifecycle,
        }
    }
}

/// Stable actor ownership transition identity retained for replay and diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmActorTransitionEvent {
    /// Monotonic event sequence within one directory.
    pub(crate) sequence: u64,
    /// Actor affected by the transition.
    pub(crate) handle: VmActorHandle,
    /// Lifecycle before the transition.
    pub(crate) from: VmActorLifecycle,
    /// Lifecycle after the transition.
    pub(crate) to: VmActorLifecycle,
    /// Scheduler owner, or zero for unowned transitions.
    pub(crate) owner: u64,
    /// Ownership generation visible after the transition.
    pub(crate) owner_generation: u64,
}

/// Generation-qualified identity assigned before mailbox storage is mutated.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VmActorPublication {
    /// Actor generation accepting the publication.
    pub(crate) handle: VmActorHandle,
    /// Monotonic sequence within the receiving actor generation.
    pub(crate) sequence: u64,
}

/// Typed rejection emitted by actor ownership and reclamation operations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum VmActorDirectoryError {
    /// No live directory entry exists for the process identity.
    MissingActor(VmProcessId),
    /// A handle refers to an older occupant of a reused slot.
    StaleHandle(VmActorHandle),
    /// The packed lifecycle value is not defined by the VM contract.
    CorruptLifecycle(u8),
    /// A lifecycle operation was attempted from an invalid state.
    InvalidTransition {
        /// State observed before the rejected transition.
        from: VmActorLifecycle,
        /// Requested destination state.
        to: VmActorLifecycle,
    },
    /// Another scheduler already owns the mutable actor state.
    AlreadyOwned {
        /// Current scheduler owner.
        owner: u64,
        /// Current ownership generation.
        owner_generation: u64,
    },
    /// A token does not match the actor's current scheduler or generation.
    StaleMutator,
    /// Scheduler identities must be representable in the packed ownership word.
    InvalidOwner(u64),
    /// A cell cannot be reclaimed while lookups remain pinned.
    LookupPinned(u32),
    /// Slot identity cannot be represented by the stable handle format.
    SlotCapacityExceeded,
    /// A bounded actor mailbox rejected producer pressure.
    MailboxFull(VmActorHandle),
    /// Actor state cannot move while published fragments remain outside it.
    TransferMailboxNotDrained {
        /// Number of complete publications still retained by the directory.
        pending: usize,
    },
}

/// Independently addressable actor state plus its atomic ownership boundary.
#[derive(Debug)]
struct VmActorCell<T, P> {
    handle: VmActorHandle,
    state: AtomicU64,
    lookup_pins: AtomicU32,
    mailbox: VmActorMailbox<P>,
    value: T,
}

impl<T, P> VmActorCell<T, P> {
    /// Creates one unowned actor cell in its initial runnable boundary state.
    fn new(handle: VmActorHandle, value: T) -> Self {
        Self {
            handle,
            state: AtomicU64::new(pack_state(
                VmActorLifecycle::Yielding,
                handle.actor_generation,
                0,
                0,
            )),
            lookup_pins: AtomicU32::new(0),
            mailbox: VmActorMailbox::default(),
            value,
        }
    }

    /// Loads and validates the packed lifecycle and ownership word.
    fn state(&self) -> Result<VmActorState, VmActorDirectoryError> {
        unpack_state(self.state.load(Ordering::Acquire))
    }
}

/// One reusable directory slot with a monotonically increasing generation.
#[derive(Debug)]
struct VmActorSlot<T, P> {
    generation: u64,
    cell: Option<VmActorCell<T, P>>,
}

/// Generational storage for independently addressable VM actors.
#[derive(Debug)]
pub(crate) struct VmActorDirectory<T, P = ()> {
    slots: Vec<VmActorSlot<T, P>>,
    free_slots: Vec<u32>,
    actor_slots: BTreeMap<VmProcessId, u32>,
    events: Mutex<Vec<VmActorTransitionEvent>>,
    next_event_sequence: AtomicU64,
}

impl<T, P> Default for VmActorDirectory<T, P> {
    /// Creates an empty actor directory with no allocated slots or events.
    fn default() -> Self {
        Self {
            slots: Vec::new(),
            free_slots: Vec::new(),
            actor_slots: BTreeMap::new(),
            events: Mutex::new(Vec::new()),
            next_event_sequence: AtomicU64::new(0),
        }
    }
}

impl<T, P> VmActorDirectory<T, P> {
    /// Inserts a new actor and returns its generation-qualified handle.
    pub(crate) fn insert(
        &mut self,
        pid: VmProcessId,
        value: T,
    ) -> Result<VmActorHandle, VmActorDirectoryError> {
        if self.actor_slots.contains_key(&pid) {
            return Err(VmActorDirectoryError::InvalidTransition {
                from: self.lifecycle(pid)?,
                to: VmActorLifecycle::Yielding,
            });
        }
        let slot = match self.free_slots.pop() {
            Some(slot) => slot,
            None => {
                let slot = u32::try_from(self.slots.len())
                    .map_err(|_| VmActorDirectoryError::SlotCapacityExceeded)?;
                self.slots.push(VmActorSlot {
                    generation: 0,
                    cell: None,
                });
                slot
            }
        };
        let entry = self
            .slots
            .get_mut(slot as usize)
            .expect("allocated actor slot exists");
        entry.generation = next_generation(entry.generation);
        let handle = VmActorHandle {
            pid,
            slot,
            actor_generation: entry.generation,
        };
        entry.cell = Some(VmActorCell::new(handle, value));
        self.actor_slots.insert(pid, slot);
        Ok(handle)
    }

    /// Returns an actor value without exposing mutable ownership.
    pub(crate) fn get(&self, pid: VmProcessId) -> Option<&T> {
        self.cell(pid).ok().map(|cell| &cell.value)
    }

    /// Returns mutable control-plane access only while no scheduler owns the actor.
    pub(crate) fn get_mut_unowned(&mut self, pid: VmProcessId) -> Option<&mut T> {
        let cell = self.cell_mut(pid).ok()?;
        let state = cell.state().ok()?;
        (state.owner == 0 && state.lifecycle != VmActorLifecycle::Reclaimed)
            .then_some(&mut cell.value)
    }

    /// Returns whether the directory currently resolves an actor identity.
    pub(crate) fn contains(&self, pid: VmProcessId) -> bool {
        self.actor_slots.contains_key(&pid)
    }

    /// Returns the number of resolvable actor cells.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.actor_slots.len()
    }

    /// Iterates over actor values in stable process-identity order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = (VmProcessId, &T)> {
        self.actor_slots.iter().filter_map(|(pid, slot)| {
            self.slots[*slot as usize]
                .cell
                .as_ref()
                .map(|cell| (*pid, &cell.value))
        })
    }

    /// Iterates over actor values in stable process-identity order.
    pub(crate) fn values(&self) -> impl Iterator<Item = &T> {
        self.iter().map(|(_, value)| value)
    }

    /// Returns the current lifecycle for one actor.
    pub(crate) fn lifecycle(
        &self,
        pid: VmProcessId,
    ) -> Result<VmActorLifecycle, VmActorDirectoryError> {
        Ok(self.cell(pid)?.state()?.lifecycle)
    }

    /// Pins one generation-qualified lookup against reclamation.
    #[cfg(all(test, not(feature = "multicore-tsan-harness")))]
    pub(crate) fn pin_lookup(
        &self,
        pid: VmProcessId,
    ) -> Result<VmActorHandle, VmActorDirectoryError> {
        let cell = self.cell(pid)?;
        let before = cell.state()?;
        if before.lifecycle == VmActorLifecycle::Migrating {
            return Err(VmActorDirectoryError::InvalidTransition {
                from: before.lifecycle,
                to: before.lifecycle,
            });
        }
        cell.lookup_pins.fetch_add(1, Ordering::AcqRel);
        let after = cell.state()?;
        if after.lifecycle == VmActorLifecycle::Migrating {
            cell.lookup_pins.fetch_sub(1, Ordering::AcqRel);
            return Err(VmActorDirectoryError::InvalidTransition {
                from: after.lifecycle,
                to: after.lifecycle,
            });
        }
        Ok(cell.handle)
    }

    /// Publishes one complete fragment without acquiring receiver mutation.
    pub(crate) fn publish_fragment(
        &self,
        pid: VmProcessId,
        payload: P,
    ) -> Result<(VmActorPublication, VmMailboxWake), VmActorDirectoryError> {
        let cell = self.cell(pid)?;
        let lifecycle = cell.state()?.lifecycle;
        if matches!(
            lifecycle,
            VmActorLifecycle::Exiting | VmActorLifecycle::Retired | VmActorLifecycle::Reclaimed
        ) {
            return Err(VmActorDirectoryError::InvalidTransition {
                from: lifecycle,
                to: lifecycle,
            });
        }
        let publication = cell.mailbox.publish(cell.handle, payload)?;
        if publication.1 == VmMailboxWake::Enqueue {
            self.enqueue_notified_parked(pid)?;
        }
        Ok(publication)
    }

    /// Integrates complete fragments only while the receiver owns mutation.
    pub(crate) fn drain_publications(
        &mut self,
        token: &VmActorMutatorToken,
        mut integrate: impl FnMut(&mut T, VmActorPublication, P),
    ) -> Result<usize, VmActorDirectoryError> {
        let cell = self.cell_for_handle_mut(token.handle)?;
        validate_token(cell, token)?;
        let handle = cell.handle;
        Ok(cell.mailbox.drain(|fragment| {
            debug_assert_eq!(fragment.publication.handle, handle);
            integrate(&mut cell.value, fragment.publication, fragment.payload);
        }))
    }

    /// Drains payloads without mutating directory-owned actor data.
    pub(crate) fn drain_payloads(
        &self,
        token: &VmActorMutatorToken,
    ) -> Result<Vec<(VmActorPublication, P)>, VmActorDirectoryError> {
        let cell = self.cell_for_handle(token.handle)?;
        validate_token(cell, token)?;
        let handle = cell.handle;
        let mut drained = Vec::new();
        cell.mailbox.drain(|fragment| {
            debug_assert_eq!(fragment.publication.handle, handle);
            drained.push((fragment.publication, fragment.payload));
        });
        Ok(drained)
    }

    /// Returns the number of fully published fragments awaiting integration.
    #[cfg(test)]
    pub(crate) fn pending_publications(
        &self,
        pid: VmProcessId,
    ) -> Result<usize, VmActorDirectoryError> {
        Ok(self.cell(pid)?.mailbox.len())
    }

    /// Marks the mailbox consumer active before actor execution.
    pub(crate) fn activate_mailbox(&self, pid: VmProcessId) -> Result<(), VmActorDirectoryError> {
        self.cell(pid)?.mailbox.activate();
        Ok(())
    }

    /// Resolves a pinned handle only when its slot generation remains current.
    #[cfg(all(test, not(feature = "multicore-tsan-harness")))]
    pub(crate) fn resolve_handle(
        &self,
        handle: VmActorHandle,
    ) -> Result<&T, VmActorDirectoryError> {
        Ok(&self.cell_for_handle(handle)?.value)
    }

    /// Releases one generation-qualified lookup pin.
    #[cfg(all(test, not(feature = "multicore-tsan-harness")))]
    pub(crate) fn unpin_lookup(&self, handle: VmActorHandle) -> Result<(), VmActorDirectoryError> {
        let cell = self.cell_for_handle(handle)?;
        cell.lookup_pins
            .try_update(Ordering::AcqRel, Ordering::Acquire, |pins| {
                pins.checked_sub(1)
            })
            .map(|_| ())
            .map_err(|_| VmActorDirectoryError::StaleHandle(handle))
    }

    /// Marks one runnable actor as queued for scheduler ownership.
    pub(crate) fn mark_queued(&self, pid: VmProcessId) -> Result<(), VmActorDirectoryError> {
        let lifecycle = self.lifecycle(pid)?;
        if lifecycle == VmActorLifecycle::Queued {
            return Ok(());
        }
        self.transition_unowned(
            pid,
            &[VmActorLifecycle::Yielding, VmActorLifecycle::Parked],
            VmActorLifecycle::Queued,
        )
    }

    /// Acquires exclusive mutable actor ownership for one scheduler.
    pub(crate) fn acquire_mutator(
        &self,
        pid: VmProcessId,
        owner: u64,
    ) -> Result<VmActorMutatorToken, VmActorDirectoryError> {
        if owner == 0 || owner > GENERATION_MASK {
            return Err(VmActorDirectoryError::InvalidOwner(owner));
        }
        let cell = self.cell(pid)?;
        let before_word = cell.state.load(Ordering::Acquire);
        let before = unpack_state(before_word)?;
        if before.owner != 0 || before.lifecycle == VmActorLifecycle::Executing {
            return Err(VmActorDirectoryError::AlreadyOwned {
                owner: before.owner,
                owner_generation: before.owner_generation,
            });
        }
        if before.lifecycle != VmActorLifecycle::Queued {
            return Err(VmActorDirectoryError::InvalidTransition {
                from: before.lifecycle,
                to: VmActorLifecycle::Executing,
            });
        }
        let owner_generation = next_generation(before.owner_generation);
        let after_word = pack_state(
            VmActorLifecycle::Executing,
            before.actor_generation,
            owner_generation,
            owner,
        );
        cell.state
            .compare_exchange(before_word, after_word, Ordering::AcqRel, Ordering::Acquire)
            .map_err(ownership_race_error)?;
        let token = VmActorMutatorToken {
            handle: cell.handle,
            owner,
            owner_generation,
            lifecycle: VmActorLifecycle::Executing,
        };
        self.record_transition(
            token.handle,
            before.lifecycle,
            VmActorLifecycle::Executing,
            owner,
            owner_generation,
        );
        Ok(token)
    }

    /// Acquires one ownership generation without changing actor lifecycle.
    pub(crate) fn acquire_control_mutator(
        &self,
        pid: VmProcessId,
        owner: u64,
    ) -> Result<VmActorMutatorToken, VmActorDirectoryError> {
        if owner == 0 || owner > GENERATION_MASK {
            return Err(VmActorDirectoryError::InvalidOwner(owner));
        }
        let cell = self.cell(pid)?;
        let before_word = cell.state.load(Ordering::Acquire);
        let before = unpack_state(before_word)?;
        if before.owner != 0 || before.lifecycle == VmActorLifecycle::Executing {
            return Err(VmActorDirectoryError::AlreadyOwned {
                owner: before.owner,
                owner_generation: before.owner_generation,
            });
        }
        if matches!(
            before.lifecycle,
            VmActorLifecycle::Migrating | VmActorLifecycle::Reclaimed
        ) {
            return Err(VmActorDirectoryError::InvalidTransition {
                from: before.lifecycle,
                to: before.lifecycle,
            });
        }
        let owner_generation = next_generation(before.owner_generation);
        let after_word = pack_state(
            before.lifecycle,
            before.actor_generation,
            owner_generation,
            owner,
        );
        cell.state
            .compare_exchange(before_word, after_word, Ordering::AcqRel, Ordering::Acquire)
            .map_err(ownership_race_error)?;
        let token = VmActorMutatorToken {
            handle: cell.handle,
            owner,
            owner_generation,
            lifecycle: before.lifecycle,
        };
        self.record_transition(
            token.handle,
            before.lifecycle,
            before.lifecycle,
            owner,
            owner_generation,
        );
        Ok(token)
    }

    /// Runs a closure with mutable actor state after validating its owner token.
    pub(crate) fn with_mutator<R>(
        &mut self,
        token: &VmActorMutatorToken,
        mutate: impl FnOnce(&mut T) -> R,
    ) -> Result<R, VmActorDirectoryError> {
        let cell = self.cell_for_handle_mut(token.handle)?;
        validate_token(cell, token)?;
        Ok(mutate(&mut cell.value))
    }

    /// Releases exclusive ownership into a valid scheduler boundary state.
    pub(crate) fn release_mutator(
        &self,
        token: VmActorMutatorToken,
        next: VmActorLifecycle,
    ) -> Result<VmActorLifecycle, VmActorDirectoryError> {
        if !matches!(
            next,
            VmActorLifecycle::Yielding | VmActorLifecycle::Parked | VmActorLifecycle::Exiting
        ) {
            return Err(VmActorDirectoryError::InvalidTransition {
                from: VmActorLifecycle::Executing,
                to: next,
            });
        }
        let cell = self.cell_for_handle(token.handle)?;
        validate_token(cell, &token)?;
        let park_ready = next != VmActorLifecycle::Parked || cell.mailbox.prepare_park();
        let released = if park_ready {
            next
        } else {
            VmActorLifecycle::Yielding
        };
        let before_word = cell.state.load(Ordering::Acquire);
        let state = unpack_state(before_word)?;
        let after_word = pack_state(released, state.actor_generation, state.owner_generation, 0);
        cell.state
            .compare_exchange(
                before_word,
                after_word,
                Ordering::Release,
                Ordering::Acquire,
            )
            .map_err(|_| VmActorDirectoryError::StaleMutator)?;
        let notified = released == VmActorLifecycle::Parked && cell.mailbox.is_notified();
        self.record_transition(
            token.handle,
            VmActorLifecycle::Executing,
            released,
            token.owner,
            token.owner_generation,
        );
        if notified {
            self.enqueue_notified_parked(token.handle.pid)?;
            return Ok(VmActorLifecycle::Queued);
        }
        Ok(released)
    }

    /// Releases scoped control ownership back to its unchanged lifecycle.
    pub(crate) fn release_control_mutator(
        &self,
        token: VmActorMutatorToken,
    ) -> Result<(), VmActorDirectoryError> {
        let cell = self.cell_for_handle(token.handle)?;
        validate_token(cell, &token)?;
        let before_word = cell.state.load(Ordering::Acquire);
        let state = unpack_state(before_word)?;
        let after_word = pack_state(
            token.lifecycle,
            state.actor_generation,
            state.owner_generation,
            0,
        );
        cell.state
            .compare_exchange(
                before_word,
                after_word,
                Ordering::Release,
                Ordering::Acquire,
            )
            .map_err(|_| VmActorDirectoryError::StaleMutator)?;
        self.record_transition(
            token.handle,
            token.lifecycle,
            token.lifecycle,
            token.owner,
            token.owner_generation,
        );
        Ok(())
    }

    /// Moves an unowned actor into its terminal cleanup state.
    pub(crate) fn mark_exiting(&mut self, pid: VmProcessId) -> Result<(), VmActorDirectoryError> {
        let lifecycle = self.lifecycle(pid)?;
        if lifecycle == VmActorLifecycle::Exiting {
            return Ok(());
        }
        self.transition_unowned(
            pid,
            &[
                VmActorLifecycle::Queued,
                VmActorLifecycle::Yielding,
                VmActorLifecycle::Parked,
                VmActorLifecycle::Migrating,
            ],
            VmActorLifecycle::Exiting,
        )
    }

    /// Begins migration only after the current scheduler released mutation.
    pub(crate) fn begin_migration(
        &mut self,
        pid: VmProcessId,
    ) -> Result<(), VmActorDirectoryError> {
        self.transition_unowned(
            pid,
            &[
                VmActorLifecycle::Queued,
                VmActorLifecycle::Yielding,
                VmActorLifecycle::Parked,
            ],
            VmActorLifecycle::Migrating,
        )
    }

    /// Completes migration with the actor unowned and ready to be queued.
    #[cfg(all(test, not(feature = "multicore-tsan-harness")))]
    pub(crate) fn finish_migration(
        &mut self,
        pid: VmProcessId,
    ) -> Result<(), VmActorDirectoryError> {
        self.transition_unowned(
            pid,
            &[VmActorLifecycle::Migrating],
            VmActorLifecycle::Yielding,
        )
    }

    /// Completes migration in the actor's exact pre-transfer boundary state.
    pub(crate) fn finish_migration_as(
        &mut self,
        pid: VmProcessId,
        lifecycle: VmActorLifecycle,
    ) -> Result<VmActorLifecycle, VmActorDirectoryError> {
        if !matches!(
            lifecycle,
            VmActorLifecycle::Queued | VmActorLifecycle::Yielding | VmActorLifecycle::Parked
        ) {
            return Err(VmActorDirectoryError::InvalidTransition {
                from: VmActorLifecycle::Migrating,
                to: lifecycle,
            });
        }
        let restored = if lifecycle == VmActorLifecycle::Queued {
            VmActorLifecycle::Yielding
        } else {
            lifecycle
        };
        self.transition_unowned(pid, &[VmActorLifecycle::Migrating], restored)?;
        if matches!(
            lifecycle,
            VmActorLifecycle::Queued | VmActorLifecycle::Yielding
        ) {
            self.mark_queued(pid)?;
            return Ok(VmActorLifecycle::Queued);
        }
        if self.cell(pid)?.mailbox.is_notified() {
            self.enqueue_notified_parked(pid)?;
            return Ok(VmActorLifecycle::Queued);
        }
        Ok(VmActorLifecycle::Parked)
    }

    /// Captures the generation stamp required to complete one migration.
    pub(crate) fn migration_stamp(
        &self,
        pid: VmProcessId,
    ) -> Result<VmActorMigrationStamp, VmActorDirectoryError> {
        let cell = self.cell(pid)?;
        let state = cell.state()?;
        if state.lifecycle != VmActorLifecycle::Migrating || state.owner != 0 {
            return Err(VmActorDirectoryError::InvalidTransition {
                from: state.lifecycle,
                to: VmActorLifecycle::Migrating,
            });
        }
        Ok(VmActorMigrationStamp {
            handle: cell.handle,
            owner_generation: state.owner_generation,
        })
    }

    /// Retains an exited actor as an inspectable tombstone.
    pub(crate) fn mark_retired(&mut self, pid: VmProcessId) -> Result<(), VmActorDirectoryError> {
        self.transition_unowned(pid, &[VmActorLifecycle::Exiting], VmActorLifecycle::Retired)
    }

    /// Reclaims a retired cell after all generation-qualified lookups finish.
    pub(crate) fn reclaim(&mut self, pid: VmProcessId) -> Result<T, VmActorDirectoryError> {
        let handle = self.cell(pid)?.handle;
        let state = self.cell(pid)?.state()?;
        if state.lifecycle != VmActorLifecycle::Retired {
            return Err(VmActorDirectoryError::InvalidTransition {
                from: state.lifecycle,
                to: VmActorLifecycle::Reclaimed,
            });
        }
        let pins = self.cell(pid)?.lookup_pins.load(Ordering::Acquire);
        if pins != 0 {
            return Err(VmActorDirectoryError::LookupPinned(pins));
        }
        self.record_transition(
            handle,
            VmActorLifecycle::Retired,
            VmActorLifecycle::Reclaimed,
            0,
            state.owner_generation,
        );
        self.actor_slots.remove(&pid);
        let slot = self
            .slots
            .get_mut(handle.slot as usize)
            .expect("resolved actor slot exists");
        let cell = slot.cell.take().expect("resolved actor cell exists");
        cell.state.store(
            pack_state(
                VmActorLifecycle::Reclaimed,
                handle.actor_generation,
                state.owner_generation,
                0,
            ),
            Ordering::Release,
        );
        self.free_slots.push(handle.slot);
        Ok(cell.value)
    }

    /// Returns the immutable transition log used by replay and diagnostics.
    #[cfg(test)]
    pub(crate) fn transition_events(&self) -> Vec<VmActorTransitionEvent> {
        self.events
            .lock()
            .expect("actor transition log mutex must not be poisoned")
            .clone()
    }

    /// Replaces the packed state so tests can verify fail-stop corruption handling.
    #[cfg(all(test, not(feature = "multicore-tsan-harness")))]
    fn corrupt_state_for_test(&self, pid: VmProcessId, word: u64) {
        self.cell(pid)
            .expect("test actor exists")
            .state
            .store(word, Ordering::Release);
    }

    /// Performs an atomic transition that requires no active mutator owner.
    fn transition_unowned(
        &self,
        pid: VmProcessId,
        allowed: &[VmActorLifecycle],
        next: VmActorLifecycle,
    ) -> Result<(), VmActorDirectoryError> {
        let cell = self.cell(pid)?;
        let before_word = cell.state.load(Ordering::Acquire);
        let before = unpack_state(before_word)?;
        if before.owner != 0 {
            return Err(VmActorDirectoryError::AlreadyOwned {
                owner: before.owner,
                owner_generation: before.owner_generation,
            });
        }
        if !allowed.contains(&before.lifecycle) {
            return Err(VmActorDirectoryError::InvalidTransition {
                from: before.lifecycle,
                to: next,
            });
        }
        let after_word = pack_state(next, before.actor_generation, before.owner_generation, 0);
        cell.state
            .compare_exchange(before_word, after_word, Ordering::AcqRel, Ordering::Acquire)
            .map_err(ownership_race_error)?;
        let handle = cell.handle;
        self.record_transition(handle, before.lifecycle, next, 0, before.owner_generation);
        Ok(())
    }

    /// Converts a published wake into at most one runnable lifecycle entry.
    fn enqueue_notified_parked(&self, pid: VmProcessId) -> Result<bool, VmActorDirectoryError> {
        let cell = self.cell(pid)?;
        let before_word = cell.state.load(Ordering::Acquire);
        let before = unpack_state(before_word)?;
        if before.lifecycle != VmActorLifecycle::Parked || before.owner != 0 {
            return Ok(false);
        }
        let after_word = pack_state(
            VmActorLifecycle::Queued,
            before.actor_generation,
            before.owner_generation,
            0,
        );
        match cell.state.compare_exchange(
            before_word,
            after_word,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                self.record_transition(
                    cell.handle,
                    VmActorLifecycle::Parked,
                    VmActorLifecycle::Queued,
                    0,
                    before.owner_generation,
                );
                Ok(true)
            }
            Err(observed) => {
                let observed = unpack_state(observed)?;
                Ok(observed.lifecycle == VmActorLifecycle::Queued && observed.owner == 0)
            }
        }
    }

    /// Resolves a process identity to its current actor cell.
    fn cell(&self, pid: VmProcessId) -> Result<&VmActorCell<T, P>, VmActorDirectoryError> {
        let slot = *self
            .actor_slots
            .get(&pid)
            .ok_or(VmActorDirectoryError::MissingActor(pid))?;
        self.slots[slot as usize]
            .cell
            .as_ref()
            .ok_or(VmActorDirectoryError::MissingActor(pid))
    }

    /// Resolves mutable directory-owned cell storage for control operations.
    fn cell_mut(
        &mut self,
        pid: VmProcessId,
    ) -> Result<&mut VmActorCell<T, P>, VmActorDirectoryError> {
        let slot = *self
            .actor_slots
            .get(&pid)
            .ok_or(VmActorDirectoryError::MissingActor(pid))?;
        self.slots[slot as usize]
            .cell
            .as_mut()
            .ok_or(VmActorDirectoryError::MissingActor(pid))
    }

    /// Resolves a generation-qualified handle without accepting slot reuse.
    fn cell_for_handle(
        &self,
        handle: VmActorHandle,
    ) -> Result<&VmActorCell<T, P>, VmActorDirectoryError> {
        let cell = self
            .slots
            .get(handle.slot as usize)
            .and_then(|slot| slot.cell.as_ref())
            .ok_or(VmActorDirectoryError::StaleHandle(handle))?;
        if cell.handle != handle {
            return Err(VmActorDirectoryError::StaleHandle(handle));
        }
        Ok(cell)
    }

    /// Resolves mutable cell storage after validating a qualified handle.
    fn cell_for_handle_mut(
        &mut self,
        handle: VmActorHandle,
    ) -> Result<&mut VmActorCell<T, P>, VmActorDirectoryError> {
        let cell = self
            .slots
            .get_mut(handle.slot as usize)
            .and_then(|slot| slot.cell.as_mut())
            .ok_or(VmActorDirectoryError::StaleHandle(handle))?;
        if cell.handle != handle {
            return Err(VmActorDirectoryError::StaleHandle(handle));
        }
        Ok(cell)
    }

    /// Appends one stable transition identity to the ownership history.
    fn record_transition(
        &self,
        handle: VmActorHandle,
        from: VmActorLifecycle,
        to: VmActorLifecycle,
        owner: u64,
        owner_generation: u64,
    ) {
        let sequence = self
            .next_event_sequence
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1);
        self.events
            .lock()
            .expect("actor transition log mutex must not be poisoned")
            .push(VmActorTransitionEvent {
                sequence,
                handle,
                from,
                to,
                owner,
                owner_generation,
            });
    }
}

#[cfg(all(test, not(feature = "multicore-tsan-harness")))]
#[cfg(test)]
#[path = "actor_directory_test.rs"]
#[cfg(test)]
mod actor_directory_test;

#[cfg(test)]
#[path = "actor_parallel_messages_beam_suite_parity_test.rs"]
#[cfg(test)]
mod actor_parallel_messages_beam_suite_parity_test;
