//! Handle liveness and disposal for SafeNative resource registries.
//!
//! Generated native adapters will eventually keep opaque native resources in a
//! supervised registry. This module captures the smallest pure part of that
//! contract: a handle is usable only while its slot is live and its generation
//! matches, and disposal turns a live slot into a non-live slot.

/// Opaque resource handle handed to Terlan-side code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafeNativeHandle {
    /// Stable slot identifier inside the adapter-owned registry.
    pub id: u64,
    /// Generation tag used to reject stale handles after slot reuse.
    pub generation: u64,
}

/// Adapter-owned registry slot state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HandleSlot {
    /// Stable slot identifier inside the adapter-owned registry.
    pub id: u64,
    /// Current generation tag for this slot.
    pub generation: u64,
    /// Whether the resource currently held by this slot is usable.
    pub live: bool,
}

/// Returns whether a handle may access a slot.
///
/// Inputs:
/// - `slot`: adapter-owned slot state.
/// - `handle`: opaque handle supplied by caller-side code.
///
/// Output:
/// - `true` only when the slot is live and both id and generation match.
///
/// Transformation:
/// - Performs a pure structural liveness check with no mutation, allocation, or
///   backend-specific side effects.
pub fn is_live(slot: HandleSlot, handle: SafeNativeHandle) -> bool {
    slot.live && slot.id == handle.id && slot.generation == handle.generation
}

/// Disposes a live slot through a matching handle.
///
/// Inputs:
/// - `slot`: adapter-owned slot state.
/// - `handle`: opaque handle supplied by caller-side code.
///
/// Output:
/// - `Some(next)` when `handle` currently owns a live view of `slot`.
/// - `None` when the handle is stale, mismatched, or already disposed.
///
/// Transformation:
/// - Preserves the slot id and generation, and changes only `live` from `true`
///   to `false` for a matching live handle.
pub fn dispose(slot: HandleSlot, handle: SafeNativeHandle) -> Option<HandleSlot> {
    if is_live(slot, handle) {
        Some(HandleSlot {
            live: false,
            ..slot
        })
    } else {
        None
    }
}

/// Computes the next generation tag for slot reuse.
///
/// Inputs:
/// - `generation`: current slot generation.
///
/// Output:
/// - `Some(next)` when the generation can be incremented.
/// - `None` when incrementing would overflow the machine integer.
///
/// Transformation:
/// - Uses checked addition so generation reuse never wraps to a stale tag.
pub fn next_generation(generation: u64) -> Option<u64> {
    generation.checked_add(1)
}

#[cfg(test)]
#[path = "handle_test.rs"]
mod handle_test;
