//! Checked aggregate predicates used by generated structured matching.

use std::num::NonZeroUsize;

use super::super::{ActorHeap, ManagedLayoutRegistry, ManagedMemoryError, SemanticTypeId, TvmRef};

const MAGIC: &[u8; 4] = b"TVMP";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 8;
const TYPE_IS: u8 = 1;
const VARIANT_IS: u8 = 2;

pub(super) fn is_pattern_operation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
}

pub fn encode_managed_type_is_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(TYPE_IS, semantic, None)
}

pub fn encode_managed_variant_is_operation(semantic: SemanticTypeId, discriminant: u32) -> Vec<u8> {
    operation(VARIANT_IS, semantic, Some(discriminant))
}

pub(super) fn execute_pattern_operation(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    let [word] = words else {
        return Err(ManagedMemoryError::InvalidAggregateArity);
    };
    let (tag, semantic, discriminant) = decode(encoded)?;
    if u64::from_ne_bytes(word.to_ne_bytes()) >> 32 == 0 {
        return Ok(0);
    }
    let reference = reference(*word)?;
    let actual = heap.descriptor(reference)?;
    if actual.semantic_id() != semantic {
        return Ok(0);
    }
    match tag {
        TYPE_IS => Ok(1),
        VARIANT_IS => {
            let expected = discriminant.ok_or(ManagedMemoryError::InvalidAggregateAbi)?;
            let layout = layouts
                .layout_for_reference(heap, semantic, reference)
                .map_err(|_| ManagedMemoryError::ManagedTypeMismatch)?;
            Ok(u64::from(layout.discriminant() == Some(expected)))
        }
        _ => Err(ManagedMemoryError::InvalidAggregateAbi),
    }
}

fn operation(tag: u8, semantic: SemanticTypeId, discriminant: Option<u32>) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(28);
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&VERSION.to_le_bytes());
    encoded.push(tag);
    encoded.push(0);
    encoded.extend_from_slice(&semantic.bytes());
    if let Some(discriminant) = discriminant {
        encoded.extend_from_slice(&discriminant.to_le_bytes());
    }
    encoded
}

fn decode(encoded: &[u8]) -> Result<(u8, SemanticTypeId, Option<u32>), ManagedMemoryError> {
    if !matches!(encoded.len(), 24 | 28)
        || encoded.get(..4) != Some(MAGIC)
        || encoded.get(4..6) != Some(&VERSION.to_le_bytes())
        || encoded[7] != 0
    {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    let semantic = encoded[HEADER_BYTES..24]
        .try_into()
        .map(SemanticTypeId::from_bytes)
        .map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    let discriminant = encoded
        .get(24..28)
        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
        .map(u32::from_le_bytes);
    if (encoded[6] == TYPE_IS && discriminant.is_some())
        || (encoded[6] == VARIANT_IS && discriminant.is_none())
    {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    Ok((encoded[6], semantic, discriminant))
}

fn reference(word: i64) -> Result<TvmRef<()>, ManagedMemoryError> {
    usize::try_from(u64::from_ne_bytes(word.to_ne_bytes()))
        .ok()
        .and_then(NonZeroUsize::new)
        .map(TvmRef::from_encoded)
        .ok_or(ManagedMemoryError::InvalidAggregateField)
}
