//! Test-only corruption hooks for managed-heap rejection regressions.

use super::{write_reference, ActorHeap, TvmRef};

impl ActorHeap {
    /// Corrupts one reference field for an internal rejection regression.
    pub(crate) fn corrupt_reference<T>(
        &mut self,
        object: TvmRef<T>,
        reference_offset: usize,
        encoded: usize,
    ) {
        let offset = self
            .resolve_offset(object)
            .expect("test object must be live");
        write_reference(&mut self.space, offset + reference_offset, encoded)
            .expect("test corruption must address a valid field");
    }
}
