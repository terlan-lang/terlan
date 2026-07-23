//! Packed actor lifecycle and ownership state helpers.

use super::{
    VmActorCell, VmActorDirectoryError, VmActorLifecycle, VmActorMutatorToken, GENERATION_BITS,
    GENERATION_MASK, LIFECYCLE_BITS, OWNER_SHIFT,
};

/// Decoded fields from one actor cell's packed atomic state word.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct VmActorState {
    /// Current scheduler-visible actor lifecycle.
    pub(super) lifecycle: VmActorLifecycle,
    /// Generation of the actor occupying its directory slot.
    pub(super) actor_generation: u64,
    /// Generation of the most recent scheduler ownership grant.
    pub(super) owner_generation: u64,
    /// Current scheduler owner word, or zero while unowned.
    pub(super) owner: u64,
}

/// Packs lifecycle, actor generation, owner generation, and owner identity.
pub(super) fn pack_state(
    lifecycle: VmActorLifecycle,
    actor_generation: u64,
    owner_generation: u64,
    owner: u64,
) -> u64 {
    (lifecycle as u64)
        | ((actor_generation & GENERATION_MASK) << LIFECYCLE_BITS)
        | ((owner_generation & GENERATION_MASK) << (LIFECYCLE_BITS + GENERATION_BITS))
        | ((owner & GENERATION_MASK) << OWNER_SHIFT)
}

/// Decodes and validates one packed actor ownership state word.
pub(super) fn unpack_state(word: u64) -> Result<VmActorState, VmActorDirectoryError> {
    Ok(VmActorState {
        lifecycle: VmActorLifecycle::from_word(word)?,
        actor_generation: (word >> LIFECYCLE_BITS) & GENERATION_MASK,
        owner_generation: (word >> (LIFECYCLE_BITS + GENERATION_BITS)) & GENERATION_MASK,
        owner: (word >> OWNER_SHIFT) & GENERATION_MASK,
    })
}

/// Advances a bounded generation while reserving zero as invalid.
pub(super) fn next_generation(current: u64) -> u64 {
    let next = (current + 1) & GENERATION_MASK;
    next.max(1)
}

/// Classifies a failed ownership compare-exchange without repairing state.
pub(super) fn ownership_race_error(word: u64) -> VmActorDirectoryError {
    match unpack_state(word) {
        Ok(state) if state.owner != 0 => VmActorDirectoryError::AlreadyOwned {
            owner: state.owner,
            owner_generation: state.owner_generation,
        },
        Ok(_) | Err(_) => VmActorDirectoryError::StaleMutator,
    }
}

/// Confirms that a mutator token still names the active owner generation.
pub(super) fn validate_token<T, P>(
    cell: &VmActorCell<T, P>,
    token: &VmActorMutatorToken,
) -> Result<(), VmActorDirectoryError> {
    let state = cell.state()?;
    if state.lifecycle != token.lifecycle
        || state.owner != token.owner
        || state.owner_generation != token.owner_generation
        || state.actor_generation != token.handle.actor_generation
    {
        return Err(VmActorDirectoryError::StaleMutator);
    }
    Ok(())
}
