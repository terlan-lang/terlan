//! Shared validation of compact, zero-field variants against the admitted image.

use super::super::{
    AtomIndex, ManagedAggregateDescriptor, ManagedLayoutRegistry, ManagedMemoryError,
    SemanticTypeId,
};

/// Managed references have a nonzero token in the upper half of their ABI word.
/// A compact word is only a candidate variant, not proof of a valid union value.
pub(super) fn is_immediate_union_word(word: i64) -> bool {
    u64::from_ne_bytes(word.to_ne_bytes()) >> 32 == 0
}

/// Resolves a compact atom using its exact union identity and image-local table.
/// Payload-bearing, unknown and ambiguous variants must not become references.
pub(crate) fn immediate_variant(
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    word: i64,
) -> Result<&ManagedAggregateDescriptor, ManagedMemoryError> {
    let index = u32::try_from(word).map_err(|_| ManagedMemoryError::InvalidAggregateField)?;
    let identity = layouts.atom_identity(AtomIndex::from_runtime(index))?;
    let mut candidates = layouts.layouts(semantic).iter().filter(|layout| {
        layout.fields().is_empty()
            && layout
                .variant_name()
                .is_some_and(|name| name.eq_ignore_ascii_case(identity))
    });
    let candidate = candidates
        .next()
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
    if candidates.any(|other| other.managed().fingerprint() != candidate.managed().fingerprint()) {
        return Err(ManagedMemoryError::ManagedTypeMismatch);
    }
    Ok(candidate)
}
