//! Bounded generated-code ABI for managed closure allocation.

use super::{ActorHeap, ManagedClosureDispatchTable, ManagedMemoryError};

const MAGIC: &[u8; 8] = b"TVMCLA01";
const VERSION: u16 = 1;
const ENCODED_BYTES: usize = 24;

/// Encodes one admitted callable identity for generated closure allocation.
pub(crate) fn encode_closure_allocation(callable_id: u64) -> Result<Vec<u8>, ManagedMemoryError> {
    if callable_id == 0 {
        return Err(ManagedMemoryError::InvalidClosure);
    }
    let mut encoded = Vec::with_capacity(ENCODED_BYTES);
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&VERSION.to_le_bytes());
    encoded.extend_from_slice(&[0; 6]);
    encoded.extend_from_slice(&callable_id.to_le_bytes());
    Ok(encoded)
}

/// Reports whether bytes identify the managed closure-allocation ABI.
pub(crate) fn is_closure_allocation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
}

/// Allocates one owned closure after resolving its shape through admitted metadata.
pub(crate) fn execute_closure_allocation(
    heap: &mut ActorHeap,
    dispatch: &ManagedClosureDispatchTable,
    encoded: &[u8],
    capture_words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    let callable_id = decode_closure_allocation(encoded)?;
    let descriptor = dispatch.closure_descriptor(callable_id)?;
    heap.allocate_closure(&descriptor, capture_words)
        .map(|closure| closure.encoded_abi_word())
}

fn decode_closure_allocation(encoded: &[u8]) -> Result<u64, ManagedMemoryError> {
    if encoded.len() != ENCODED_BYTES
        || encoded.get(..8) != Some(MAGIC)
        || encoded.get(8..10) != Some(&VERSION.to_le_bytes())
        || encoded.get(10..16) != Some(&[0; 6])
    {
        return Err(ManagedMemoryError::InvalidClosure);
    }
    let callable_id = u64::from_le_bytes(
        encoded[16..24]
            .try_into()
            .map_err(|_| ManagedMemoryError::InvalidClosure)?,
    );
    if callable_id == 0 {
        return Err(ManagedMemoryError::InvalidClosure);
    }
    Ok(callable_id)
}
