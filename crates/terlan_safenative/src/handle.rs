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
mod tests {
    use super::*;

    /// Builds a live slot and matching handle used by liveness tests.
    ///
    /// Inputs:
    /// - No external input.
    ///
    /// Output:
    /// - A tuple containing a live slot and a matching handle.
    ///
    /// Transformation:
    /// - Creates fixed test data with id `7` and generation `3`.
    fn matching_pair() -> (HandleSlot, SafeNativeHandle) {
        (
            HandleSlot {
                id: 7,
                generation: 3,
                live: true,
            },
            SafeNativeHandle {
                id: 7,
                generation: 3,
            },
        )
    }

    /// Verifies a live slot accepts its matching handle.
    ///
    /// Inputs:
    /// - A live slot and matching handle.
    ///
    /// Output:
    /// - Test passes when `is_live` returns `true`.
    ///
    /// Transformation:
    /// - Exercises the success branch for exact id/generation ownership.
    #[test]
    fn is_live_accepts_matching_live_handle() {
        let (slot, handle) = matching_pair();

        assert!(is_live(slot, handle));
    }

    /// Verifies stale generations are rejected.
    ///
    /// Inputs:
    /// - A live slot and a handle with the same id but older generation.
    ///
    /// Output:
    /// - Test passes when `is_live` returns `false`.
    ///
    /// Transformation:
    /// - Exercises the stale-handle rejection path used after slot reuse.
    #[test]
    fn is_live_rejects_stale_generation() {
        let (slot, mut handle) = matching_pair();
        handle.generation = 2;

        assert!(!is_live(slot, handle));
    }

    /// Verifies disposal turns a matching live slot into a non-live slot.
    ///
    /// Inputs:
    /// - A live slot and matching handle.
    ///
    /// Output:
    /// - Test passes when disposal succeeds and the resulting slot is not live.
    ///
    /// Transformation:
    /// - Exercises deterministic disposal without changing slot identity.
    #[test]
    fn dispose_marks_matching_handle_not_live() {
        let (slot, handle) = matching_pair();
        let next = HandleSlot {
            live: false,
            ..slot
        };

        assert_eq!(dispose(slot, handle), Some(next));
        assert!(!is_live(next, handle));
    }

    /// Verifies stale handles cannot dispose slots.
    ///
    /// Inputs:
    /// - A live slot and a handle with a mismatched generation.
    ///
    /// Output:
    /// - Test passes when disposal returns `None`.
    ///
    /// Transformation:
    /// - Exercises the same stale-handle rejection rule used by runtime calls.
    #[test]
    fn dispose_rejects_stale_handle() {
        let (slot, mut handle) = matching_pair();
        handle.generation = 2;

        assert_eq!(dispose(slot, handle), None);
    }

    /// Verifies generation tags do not wrap.
    ///
    /// Inputs:
    /// - The maximum `u64` generation.
    ///
    /// Output:
    /// - Test passes when incrementing returns `None`.
    ///
    /// Transformation:
    /// - Uses checked addition to preserve stale-handle rejection across slot
    ///   reuse boundaries.
    #[test]
    fn next_generation_rejects_overflow() {
        assert_eq!(next_generation(u64::MAX), None);
        assert_eq!(next_generation(3), Some(4));
    }
}
