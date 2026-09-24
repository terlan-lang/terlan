//! Typed existential storage for actor-owned deferred values.

use std::sync::Arc;

use crate::runtime::native_image::TvmBoundaryType;

use super::closures::{managed_semantic_id, validate_scalar_capture};
use super::{
    ActorHeap, AllocationClass, ManagedMemoryError, ManagedTypeDescriptor, SemanticTypeId, TvmRef,
};

const MAGIC: &[u8; 8] = b"TVMEV001";
const VALUE_OFFSET: usize = 32;
const OBJECT_BYTES: usize = VALUE_OFFSET + size_of::<i64>();

/// Marker for one immutable value retaining its concrete native ABI type.
#[derive(Debug)]
pub struct ManagedErasedValue;

/// Returns the shared identity of type-tagged, actor-owned value envelopes.
pub fn managed_erased_value_semantic_id() -> Result<SemanticTypeId, ManagedMemoryError> {
    SemanticTypeId::from_canonical("terlan.runtime.ErasedValue.v1")
}

impl ActorHeap {
    /// Boxes one native word without losing its scalar kind or GC reference map.
    ///
    /// Managed inputs must belong to this actor and match the exact declared
    /// semantic identity. Host JSON, external resource handles and borrowed or
    /// untyped pointers are rejected; resource ownership needs its own context.
    pub fn allocate_erased_value(
        &mut self,
        ty: &TvmBoundaryType,
        word: i64,
    ) -> Result<TvmRef<ManagedErasedValue>, ManagedMemoryError> {
        validate_value_type(ty)?;
        validate_word(ty, word)?;
        let references = match managed_semantic_id(ty)? {
            Some(semantic) => vec![(
                VALUE_OFFSET,
                self.validate_abi_reference(u64::from_ne_bytes(word.to_ne_bytes()), semantic)?,
            )],
            None => Vec::new(),
        };
        let mut payload = shape(ty);
        payload.extend_from_slice(&word.to_ne_bytes());
        self.allocate(descriptor(ty)?, &payload, &references)
    }

    /// Recovers a boxed word only when the caller supplies its exact native type.
    ///
    /// Checks actor ownership, generation, envelope layout and payload type
    /// before returning a scalar or a validated actor-local reference.
    pub fn unbox_erased_value(
        &self,
        value: TvmRef<ManagedErasedValue>,
        expected: &TvmBoundaryType,
    ) -> Result<i64, ManagedMemoryError> {
        validate_value_type(expected)?;
        let (ty, word) = self.erased_value(value)?;
        if &ty != expected {
            return Err(ManagedMemoryError::ManagedTypeMismatch);
        }
        Ok(word)
    }

    /// Reads an existential's actual type and word after validating its complete
    /// envelope and actor-local payload. Returned references are borrowed native
    /// words, not roots; callers must not retain them across a heap safepoint.
    pub(crate) fn erased_value(
        &self,
        value: TvmRef<ManagedErasedValue>,
    ) -> Result<(TvmBoundaryType, i64), ManagedMemoryError> {
        let stored = self.descriptor(value)?;
        if stored.semantic_id() != managed_erased_value_semantic_id()? {
            return Err(ManagedMemoryError::ManagedTypeMismatch);
        }
        let payload = self.read(value)?;
        if payload.len() != OBJECT_BYTES || payload.get(..8) != Some(MAGIC) {
            return Err(ManagedMemoryError::InvalidManagedOperation);
        }
        let ty = decode_value_type(&payload[8..VALUE_OFFSET])?;
        if stored.fingerprint() != descriptor(&ty)?.fingerprint() {
            return Err(ManagedMemoryError::LayoutMismatch);
        }
        let word = i64::from_ne_bytes(
            payload[VALUE_OFFSET..]
                .try_into()
                .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?,
        );
        validate_word(&ty, word)?;
        if let Some(semantic) = managed_semantic_id(&ty)? {
            self.validate_abi_reference(u64::from_ne_bytes(word.to_ne_bytes()), semantic)?;
        }
        Ok((ty, word))
    }
}

fn descriptor(ty: &TvmBoundaryType) -> Result<Arc<ManagedTypeDescriptor>, ManagedMemoryError> {
    let references = if ty.is_managed_reference() {
        vec![VALUE_OFFSET]
    } else {
        Vec::new()
    };
    Ok(Arc::new(ManagedTypeDescriptor::new_specialized(
        managed_erased_value_semantic_id()?,
        OBJECT_BYTES,
        align_of::<i64>(),
        references,
        AllocationClass::Young,
        &shape(ty),
    )?))
}

fn shape(ty: &TvmBoundaryType) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(OBJECT_BYTES);
    bytes.extend_from_slice(MAGIC);
    for word in ty.transition_words() {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes
}

pub(super) fn validate_value_type(ty: &TvmBoundaryType) -> Result<(), ManagedMemoryError> {
    if matches!(
        ty,
        TvmBoundaryType::Json | TvmBoundaryType::NativeResource(_)
    ) {
        return Err(ManagedMemoryError::InvalidManagedOperation);
    }
    Ok(())
}

fn validate_word(ty: &TvmBoundaryType, word: i64) -> Result<(), ManagedMemoryError> {
    validate_scalar_capture(ty, word)?;
    if matches!(ty, TvmBoundaryType::Atom) && u32::try_from(word).is_err() {
        return Err(ManagedMemoryError::InvalidManagedScalar);
    }
    Ok(())
}

pub(super) fn decode_value_type(bytes: &[u8]) -> Result<TvmBoundaryType, ManagedMemoryError> {
    if bytes.len() != 24 {
        return Err(ManagedMemoryError::InvalidManagedOperation);
    }
    let mut words = [0_i64; 3];
    for (word, bytes) in words.iter_mut().zip(bytes.chunks_exact(8)) {
        *word = i64::from_le_bytes(
            bytes
                .try_into()
                .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?,
        );
    }
    let ty = TvmBoundaryType::from_transition_words(&words)
        .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?;
    validate_value_type(&ty)?;
    if ty.transition_words() != words {
        return Err(ManagedMemoryError::InvalidManagedOperation);
    }
    Ok(ty)
}

#[cfg(test)]
#[path = "erased_values_test.rs"]
mod tests;
