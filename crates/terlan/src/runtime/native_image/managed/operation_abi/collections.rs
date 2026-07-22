//! Generated-code operations for concrete persistent collection values.

use super::super::{
    ActorHeap, ManagedFieldType, ManagedMap, ManagedMemoryError, ManagedScalarKeySemantics,
    ManagedStringKeySemantics, SemanticTypeId, TvmRef,
};
use super::{field_value, reference_word};
use crate::runtime::native_image::managed::ManagedLayoutRegistry;

const MAGIC: &[u8; 4] = b"TVMC";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 8;
const LIST_FROM_ELEMENTS: u8 = 1;
const LIST_PREPEND: u8 = 2;
const MAP_FROM_ENTRIES: u8 = 3;
const LIST_IS_EMPTY: u8 = 4;
const LIST_FIRST: u8 = 5;
const LIST_REST: u8 = 6;
const MAP_CONTAINS: u8 = 7;
const MAP_GET: u8 = 8;
const OPERATION_BYTES: usize = HEADER_BYTES + 16;

pub(super) fn is_collection_operation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
}

pub fn encode_list_from_elements_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(LIST_FROM_ELEMENTS, semantic)
}

pub fn encode_list_prepend_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(LIST_PREPEND, semantic)
}

pub fn encode_map_from_entries_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(MAP_FROM_ENTRIES, semantic)
}

pub fn encode_list_is_empty_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_result(LIST_IS_EMPTY, semantic, false)
}

pub fn encode_list_first_operation(semantic: SemanticTypeId, reference: bool) -> Vec<u8> {
    operation_with_result(LIST_FIRST, semantic, reference)
}

pub fn encode_list_rest_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_result(LIST_REST, semantic, true)
}

pub fn encode_map_contains_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_result(MAP_CONTAINS, semantic, false)
}

pub fn encode_map_get_operation(semantic: SemanticTypeId, reference: bool) -> Vec<u8> {
    operation_with_result(MAP_GET, semantic, reference)
}

pub(super) fn collection_operation_result_is_reference(encoded: &[u8]) -> bool {
    encoded.get(7) == Some(&1)
        || matches!(
            encoded.get(6).copied(),
            Some(LIST_FROM_ELEMENTS | LIST_PREPEND | MAP_FROM_ENTRIES)
        )
}

pub(super) fn execute_collection_operation(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    let (operation, semantic, _) = decode(encoded)?;
    let collection = layouts
        .collection(semantic)
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
    match operation {
        LIST_FROM_ELEMENTS => {
            let descriptor = collection
                .list_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let elements = words
                .iter()
                .map(|word| field_value(*word, descriptor.element_type()))
                .collect::<Result<Vec<_>, _>>()?;
            heap.list_from_elements(descriptor, &elements)
                .map(TvmRef::encoded_abi_word)
        }
        LIST_PREPEND => {
            let [head, tail] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .list_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let tail = reference_word(*tail)?.cast();
            let mut elements = heap.list_elements(descriptor, tail)?;
            elements.insert(0, field_value(*head, descriptor.element_type())?);
            heap.list_from_elements(descriptor, &elements)
                .map(TvmRef::encoded_abi_word)
        }
        MAP_FROM_ENTRIES => {
            if words.len() % 2 != 0 {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            }
            let descriptor = collection
                .map_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let entries = words
                .chunks_exact(2)
                .map(|pair| {
                    Ok((
                        field_value(pair[0], descriptor.key_type())?,
                        field_value(pair[1], descriptor.value_type())?,
                    ))
                })
                .collect::<Result<Vec<_>, ManagedMemoryError>>()?;
            let map = if descriptor.key_type()
                == ManagedFieldType::Reference(SemanticTypeId::from_canonical("std.core.String")?)
            {
                heap.map_from_entries(descriptor, &entries, &mut ManagedStringKeySemantics)?
            } else {
                heap.map_from_entries(descriptor, &entries, &mut ManagedScalarKeySemantics)?
            };
            Ok(map.cast::<ManagedMap>().encoded_abi_word())
        }
        LIST_IS_EMPTY => {
            let [list] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .list_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            heap.list_is_empty(descriptor, reference_word(*list)?.cast())
                .map(u64::from)
        }
        LIST_FIRST => {
            let [list] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .list_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            heap.list_first(descriptor, reference_word(*list)?.cast())?
                .map(super::field_word)
                .ok_or(ManagedMemoryError::InvalidSequenceLength)
        }
        LIST_REST => {
            let [list] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .list_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            heap.list_rest(descriptor, reference_word(*list)?.cast())?
                .map(TvmRef::encoded_abi_word)
                .ok_or(ManagedMemoryError::InvalidSequenceLength)
        }
        MAP_CONTAINS | MAP_GET => {
            let [map, key] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .map_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let map = reference_word(*map)?.cast();
            let key = field_value(*key, descriptor.key_type())?;
            let value = if descriptor.key_type()
                == ManagedFieldType::Reference(SemanticTypeId::from_canonical("std.core.String")?)
            {
                heap.map_get(descriptor, map, key, &mut ManagedStringKeySemantics)?
            } else {
                heap.map_get(descriptor, map, key, &mut ManagedScalarKeySemantics)?
            };
            if operation == MAP_CONTAINS {
                Ok(u64::from(value.is_some()))
            } else {
                value
                    .map(super::field_word)
                    .ok_or(ManagedMemoryError::InvalidAggregateField)
            }
        }
        _ => Err(ManagedMemoryError::InvalidAggregateAbi),
    }
}

fn operation(tag: u8, semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_result(tag, semantic, true)
}

fn operation_with_result(tag: u8, semantic: SemanticTypeId, reference: bool) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(OPERATION_BYTES);
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&VERSION.to_le_bytes());
    encoded.push(tag);
    encoded.push(u8::from(reference));
    encoded.extend_from_slice(&semantic.bytes());
    encoded
}

fn decode(encoded: &[u8]) -> Result<(u8, SemanticTypeId, bool), ManagedMemoryError> {
    if encoded.len() != OPERATION_BYTES
        || encoded.get(..4) != Some(MAGIC)
        || encoded.get(4..6) != Some(&VERSION.to_le_bytes())
        || encoded[7] > 1
    {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    let semantic = encoded[HEADER_BYTES..]
        .try_into()
        .map(SemanticTypeId::from_bytes)
        .map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    Ok((encoded[6], semantic, encoded[7] == 1))
}
