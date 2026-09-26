//! Bounded boxing operations for typed, actor-local existential payloads.

use crate::runtime::native_image::TvmBoundaryType;

use super::super::erased_values::{decode_value_type, validate_value_type};
use super::super::{managed_erased_value_semantic_id, ManagedErasedValue};
use super::immediate_union::{immediate_variant, is_immediate_union_word};
use super::{ActorHeap, ManagedLayoutRegistry, ManagedMemoryError, SemanticTypeId};

const MAGIC: &[u8; 4] = b"TVMX";
const BOX: u8 = 1;
const UNBOX: u8 = 2;
const IS_TYPE: u8 = 3;
const ENCODED_BYTES: usize = 32;

/// Encodes boxing of one word with its concrete managed native ABI type.
pub fn encode_erased_value_box_operation(
    ty: &TvmBoundaryType,
) -> Result<Vec<u8>, ManagedMemoryError> {
    encode(BOX, ty)
}

/// Encodes checked recovery of one boxed word with the expected exact ABI type.
pub fn encode_erased_value_unbox_operation(
    ty: &TvmBoundaryType,
) -> Result<Vec<u8>, ManagedMemoryError> {
    encode(UNBOX, ty)
}

/// Encodes a checked type query without swallowing malformed or foreign values.
pub fn encode_erased_value_is_type_operation(
    ty: &TvmBoundaryType,
) -> Result<Vec<u8>, ManagedMemoryError> {
    encode(IS_TYPE, ty)
}

fn encode(operation: u8, ty: &TvmBoundaryType) -> Result<Vec<u8>, ManagedMemoryError> {
    validate_value_type(ty)?;
    let mut encoded = Vec::with_capacity(ENCODED_BYTES);
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&1_u16.to_le_bytes());
    encoded.extend_from_slice(&[operation, 0]);
    for word in ty.transition_words() {
        encoded.extend_from_slice(&word.to_le_bytes());
    }
    Ok(encoded)
}

pub(super) fn is_erased_operation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
}

fn decode(encoded: &[u8]) -> Result<(u8, TvmBoundaryType), ManagedMemoryError> {
    if encoded.len() != ENCODED_BYTES
        || !is_erased_operation(encoded)
        || encoded[4..6] != 1_u16.to_le_bytes()
        || !matches!(encoded[6], BOX | UNBOX | IS_TYPE)
        || encoded[7] != 0
    {
        return Err(ManagedMemoryError::InvalidManagedOperation);
    }
    Ok((encoded[6], decode_value_type(&encoded[8..])?))
}

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(super) fn result_is_reference(encoded: &[u8]) -> bool {
    decode(encoded).map_or(true, |(operation, ty)| {
        operation == BOX || (operation == UNBOX && ty.is_managed_reference())
    })
}

pub(super) fn execute(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    let (operation, ty) = decode(encoded)?;
    let [word] = words else {
        return Err(ManagedMemoryError::InvalidAggregateArity);
    };
    if operation == BOX {
        if let TvmBoundaryType::Managed(semantic) = &ty {
            if is_immediate_union_word(*word) {
                let variant =
                    immediate_variant(layouts, SemanticTypeId::from_bytes(*semantic), *word)?;
                // Normalize only validated zero-field variants. The envelope then
                // uses the ordinary reference layout and moving-GC root handling.
                return heap.with_allocation_transaction(|heap| {
                    let reference = heap.allocate_aggregate_ref(variant, &[])?;
                    heap.allocate_erased_value(
                        &ty,
                        i64::from_ne_bytes(reference.encoded_abi_word().to_ne_bytes()),
                    )
                    .map(|value| value.encoded_abi_word())
                });
            }
        }
        return heap
            .allocate_erased_value(&ty, *word)
            .map(|value| value.encoded_abi_word());
    }
    let value = heap.validate_abi_reference(
        u64::from_ne_bytes(word.to_ne_bytes()),
        managed_erased_value_semantic_id()?,
    )?;
    if operation == IS_TYPE {
        let (actual, _) = heap.erased_value(value.cast::<ManagedErasedValue>())?;
        return Ok(u64::from(actual == ty));
    }
    heap.unbox_erased_value(value.cast::<ManagedErasedValue>(), &ty)
        .map(|word| u64::from_ne_bytes(word.to_ne_bytes()))
}

#[cfg(test)]
#[path = "erased_test.rs"]
mod tests;
