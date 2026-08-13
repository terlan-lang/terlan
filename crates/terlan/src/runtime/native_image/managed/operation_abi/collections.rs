//! Generated-code operations for concrete persistent collection values.

use super::super::{
    ActorHeap, ManagedFieldType, ManagedFieldValue, ManagedMap, ManagedMemoryError,
    ManagedScalarKeySemantics, ManagedSet, ManagedStringKeySemantics, SemanticTypeId, TvmRef,
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
const LIST_LENGTH: u8 = 9;
const LIST_FIRST_OPTION: u8 = 10;
const LIST_REST_OPTION: u8 = 11;
const MAP_EMPTY: u8 = 12;
const MAP_IS_EMPTY: u8 = 13;
const MAP_LENGTH: u8 = 14;
const MAP_GET_OPTION: u8 = 15;
const MAP_PUT: u8 = 16;
const MAP_REMOVE: u8 = 17;
const MAP_CLEAR: u8 = 18;
const MAP_TAKE: u8 = 19;
const MAP_ITERATOR: u8 = 20;
const MAP_FROM_ENTRY_LIST: u8 = 21;
const ITERATOR_NEXT: u8 = 22;
const LIST_GET: u8 = 23;
const LIST_APPEND: u8 = 24;
const LIST_CONCAT: u8 = 25;
const LIST_SUBTRACT: u8 = 26;
const LIST_CLEAR: u8 = 27;
const SET_FROM_LIST: u8 = 28;
const SET_CONTAINS: u8 = 29;
const SET_EMPTY: u8 = 30;
const SET_IS_EMPTY: u8 = 31;
const SET_LENGTH: u8 = 32;
const SET_ADD: u8 = 33;
const SET_REMOVE: u8 = 34;
const SET_CLEAR: u8 = 35;
const SET_ITERATOR: u8 = 36;
const OPERATION_BYTES: usize = HEADER_BYTES + 16;
const OPTION_OPERATION_BYTES: usize = HEADER_BYTES + 32;
const TRIPLE_OPERATION_BYTES: usize = HEADER_BYTES + 48;

#[path = "collections/codec.rs"]
mod codec;
use codec::{decode, multi_semantic_operation, operation, operation_with_result, option_operation};

pub(super) fn is_collection_operation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
}

/// Encodes construction of the identified list from ABI element words.
pub fn encode_list_from_elements_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(LIST_FROM_ELEMENTS, semantic)
}

/// Encodes persistent insertion at the front of the identified list.
pub fn encode_list_prepend_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(LIST_PREPEND, semantic)
}

/// Encodes persistent insertion at the back of the identified list.
pub fn encode_list_append_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(LIST_APPEND, semantic)
}

/// Encodes persistent concatenation of two lists with the same schema.
pub fn encode_list_concat_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(LIST_CONCAT, semantic)
}

/// Encodes structural subtraction of one list from another.
pub fn encode_list_subtract_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(LIST_SUBTRACT, semantic)
}

/// Encodes replacement of a checked list by an empty list of the same schema.
pub fn encode_list_clear_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(LIST_CLEAR, semantic)
}

/// Encodes construction of the identified map from alternating key/value words.
pub fn encode_map_from_entries_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(MAP_FROM_ENTRIES, semantic)
}

/// Encodes an emptiness predicate for the identified list.
pub fn encode_list_is_empty_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_result(LIST_IS_EMPTY, semantic, false)
}

/// Encodes strict first-element lookup and its scalar/reference result shape.
pub fn encode_list_first_operation(semantic: SemanticTypeId, reference: bool) -> Vec<u8> {
    operation_with_result(LIST_FIRST, semantic, reference)
}

/// Encodes strict lookup of the list tail.
pub fn encode_list_rest_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_result(LIST_REST, semantic, true)
}

/// Encodes a scalar length query for the identified list.
pub fn encode_list_length_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_result(LIST_LENGTH, semantic, false)
}

/// Encodes bounds-checked indexed lookup and its scalar/reference result shape.
pub fn encode_list_get_operation(semantic: SemanticTypeId, reference: bool) -> Vec<u8> {
    operation_with_result(LIST_GET, semantic, reference)
}

/// Encodes optional first-element lookup using the identified option layout.
pub fn encode_list_first_option_operation(
    list_semantic: SemanticTypeId,
    option_semantic: SemanticTypeId,
) -> Vec<u8> {
    option_operation(LIST_FIRST_OPTION, list_semantic, option_semantic)
}

/// Encodes optional tail lookup using the identified option layout.
pub fn encode_list_rest_option_operation(
    list_semantic: SemanticTypeId,
    option_semantic: SemanticTypeId,
) -> Vec<u8> {
    option_operation(LIST_REST_OPTION, list_semantic, option_semantic)
}

/// Encodes a key-membership predicate for the identified map.
pub fn encode_map_contains_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_result(MAP_CONTAINS, semantic, false)
}

/// Encodes strict map lookup and its scalar/reference result shape.
pub fn encode_map_get_operation(semantic: SemanticTypeId, reference: bool) -> Vec<u8> {
    operation_with_result(MAP_GET, semantic, reference)
}

/// Encodes allocation of an empty map with the identified layout.
pub fn encode_map_empty_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(MAP_EMPTY, semantic)
}

/// Encodes an emptiness predicate for the identified map.
pub fn encode_map_is_empty_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_result(MAP_IS_EMPTY, semantic, false)
}

/// Encodes a scalar entry-count query for the identified map.
pub fn encode_map_length_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_result(MAP_LENGTH, semantic, false)
}

/// Encodes optional map lookup using the identified option layout.
pub fn encode_map_get_option_operation(
    map_semantic: SemanticTypeId,
    option_semantic: SemanticTypeId,
) -> Vec<u8> {
    multi_semantic_operation(MAP_GET_OPTION, &[map_semantic, option_semantic])
}

/// Encodes persistent key/value insertion into the identified map.
pub fn encode_map_put_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(MAP_PUT, semantic)
}

/// Encodes persistent key removal from the identified map.
pub fn encode_map_remove_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(MAP_REMOVE, semantic)
}

/// Encodes removal of every entry from the identified map.
pub fn encode_map_clear_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(MAP_CLEAR, semantic)
}

/// Encodes atomic lookup-and-removal with explicit option and result layouts.
pub fn encode_map_take_operation(
    map_semantic: SemanticTypeId,
    option_semantic: SemanticTypeId,
    result_semantic: SemanticTypeId,
) -> Vec<u8> {
    multi_semantic_operation(MAP_TAKE, &[map_semantic, option_semantic, result_semantic])
}

/// Encodes deterministic map iteration into the identified list/pair layouts.
pub fn encode_map_iterator_operation(
    map_semantic: SemanticTypeId,
    list_semantic: SemanticTypeId,
    pair_semantic: SemanticTypeId,
) -> Vec<u8> {
    multi_semantic_operation(MAP_ITERATOR, &[map_semantic, list_semantic, pair_semantic])
}

/// Encodes map construction from a managed list of managed pairs.
pub fn encode_map_from_entry_list_operation(
    map_semantic: SemanticTypeId,
    list_semantic: SemanticTypeId,
    pair_semantic: SemanticTypeId,
) -> Vec<u8> {
    multi_semantic_operation(
        MAP_FROM_ENTRY_LIST,
        &[map_semantic, list_semantic, pair_semantic],
    )
}

/// Encodes one iterator step with explicit option and step layouts.
pub fn encode_iterator_next_operation(
    list_semantic: SemanticTypeId,
    option_semantic: SemanticTypeId,
    step_semantic: SemanticTypeId,
) -> Vec<u8> {
    multi_semantic_operation(
        ITERATOR_NEXT,
        &[list_semantic, option_semantic, step_semantic],
    )
}

/// Encodes set construction from a managed list with the same element type.
pub fn encode_set_from_list_operation(
    set_semantic: SemanticTypeId,
    list_semantic: SemanticTypeId,
) -> Vec<u8> {
    multi_semantic_operation(SET_FROM_LIST, &[set_semantic, list_semantic])
}

/// Encodes a structural element-membership predicate for one set schema.
pub fn encode_set_contains_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_result(SET_CONTAINS, semantic, false)
}

/// Encodes allocation of an empty set with the identified layout.
pub fn encode_set_empty_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(SET_EMPTY, semantic)
}

/// Encodes an emptiness predicate for the identified set.
pub fn encode_set_is_empty_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_result(SET_IS_EMPTY, semantic, false)
}

/// Encodes a scalar unique-element count for the identified set.
pub fn encode_set_length_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_result(SET_LENGTH, semantic, false)
}

/// Encodes persistent insertion into the identified set.
pub fn encode_set_add_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(SET_ADD, semantic)
}

/// Encodes persistent removal from the identified set.
pub fn encode_set_remove_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(SET_REMOVE, semantic)
}

/// Encodes replacement of a checked set by an empty set of the same schema.
pub fn encode_set_clear_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(SET_CLEAR, semantic)
}

/// Encodes deterministic set iteration into a managed list.
pub fn encode_set_iterator_operation(
    set_semantic: SemanticTypeId,
    list_semantic: SemanticTypeId,
) -> Vec<u8> {
    multi_semantic_operation(SET_ITERATOR, &[set_semantic, list_semantic])
}

pub(super) fn collection_operation_result_is_reference(encoded: &[u8]) -> bool {
    encoded.get(7) == Some(&1)
        || matches!(
            encoded.get(6).copied(),
            Some(
                LIST_FROM_ELEMENTS
                    | LIST_PREPEND
                    | LIST_APPEND
                    | LIST_CONCAT
                    | LIST_SUBTRACT
                    | LIST_CLEAR
                    | MAP_FROM_ENTRIES
                    | MAP_EMPTY
                    | MAP_GET_OPTION
                    | MAP_PUT
                    | MAP_REMOVE
                    | MAP_CLEAR
                    | MAP_TAKE
                    | MAP_ITERATOR
                    | MAP_FROM_ENTRY_LIST
                    | ITERATOR_NEXT
                    | SET_FROM_LIST
                    | SET_EMPTY
                    | SET_ADD
                    | SET_REMOVE
                    | SET_CLEAR
                    | SET_ITERATOR
            )
        )
}

pub(super) fn execute_collection_operation(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    let (operation, semantics, _) = decode(encoded)?;
    let semantic = semantics[0];
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
            heap.list_prepend(
                descriptor,
                tail,
                field_value(*head, descriptor.element_type())?,
            )
            .map(TvmRef::encoded_abi_word)
        }
        LIST_APPEND => {
            let [list, value] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .list_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let list = reference_word(*list)?.cast();
            heap.list_append(
                descriptor,
                list,
                field_value(*value, descriptor.element_type())?,
            )
            .map(TvmRef::encoded_abi_word)
        }
        LIST_CONCAT => {
            let [left, right] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .list_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            heap.list_concat(
                descriptor,
                reference_word(*left)?.cast(),
                reference_word(*right)?.cast(),
            )
            .map(TvmRef::encoded_abi_word)
        }
        LIST_SUBTRACT => {
            let [left, right] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .list_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            heap.list_subtract(
                descriptor,
                reference_word(*left)?.cast(),
                reference_word(*right)?.cast(),
                |heap, left, right| {
                    super::equality::managed_field_values_equal(
                        heap,
                        layouts,
                        descriptor.element_type(),
                        left,
                        right,
                    )
                },
            )
            .map(TvmRef::encoded_abi_word)
        }
        LIST_CLEAR => {
            let [list] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .list_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            heap.list_length(descriptor, reference_word(*list)?.cast())?;
            heap.list_from_elements(descriptor, &[])
                .map(TvmRef::encoded_abi_word)
        }
        MAP_FROM_ENTRIES => {
            if !words.len().is_multiple_of(2) {
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
        LIST_LENGTH => {
            let [list] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .list_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let length = heap.list_length(descriptor, reference_word(*list)?.cast())?;
            i64::try_from(length)
                .map(|length| u64::from_ne_bytes(length.to_ne_bytes()))
                .map_err(|_| ManagedMemoryError::InvalidSequenceLength)
        }
        LIST_GET => {
            let [list, index] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .list_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let index = usize::try_from(*index)
                .map_err(|_| ManagedMemoryError::CollectionIndexOutOfBounds)?;
            heap.list_get(descriptor, reference_word(*list)?.cast(), index)
                .map(super::field_word)
        }
        LIST_FIRST_OPTION => {
            let [list] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .list_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let value = heap.list_first(descriptor, reference_word(*list)?.cast())?;
            allocate_option(heap, layouts, semantics[1], value)
        }
        LIST_REST_OPTION => {
            let [list] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .list_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let value = heap
                .list_rest(descriptor, reference_word(*list)?.cast())?
                .map(|list| ManagedFieldValue::Reference(list.erase()));
            allocate_option(heap, layouts, semantics[1], value)
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
        MAP_EMPTY => {
            let [] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .map_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            heap.map_empty(descriptor).map(TvmRef::encoded_abi_word)
        }
        MAP_IS_EMPTY | MAP_LENGTH => {
            let [map] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .map_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let map = reference_word(*map)?.cast();
            if operation == MAP_IS_EMPTY {
                heap.map_is_empty(descriptor, map).map(u64::from)
            } else {
                let length = heap.map_length(descriptor, map)?;
                i64::try_from(length)
                    .map(|length| u64::from_ne_bytes(length.to_ne_bytes()))
                    .map_err(|_| ManagedMemoryError::InvalidSequenceLength)
            }
        }
        MAP_GET_OPTION => {
            let [map, key] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .map_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let value = map_get(
                heap,
                descriptor,
                reference_word(*map)?.cast(),
                field_value(*key, descriptor.key_type())?,
            )?;
            allocate_option(heap, layouts, semantics[1], value)
        }
        MAP_PUT => {
            let [map, key, value] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .map_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let map = reference_word(*map)?.cast();
            let key = field_value(*key, descriptor.key_type())?;
            let value = field_value(*value, descriptor.value_type())?;
            map_put(heap, descriptor, map, key, value).map(TvmRef::encoded_abi_word)
        }
        MAP_REMOVE => {
            let [map, key] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .map_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let map = reference_word(*map)?.cast();
            let key = field_value(*key, descriptor.key_type())?;
            map_remove(heap, descriptor, map, key).map(TvmRef::encoded_abi_word)
        }
        MAP_CLEAR => {
            let [map] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .map_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            heap.map_clear(descriptor, reference_word(*map)?.cast())
                .map(TvmRef::encoded_abi_word)
        }
        MAP_TAKE => {
            let [map, key] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .map_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let map = reference_word(*map)?.cast();
            let key = field_value(*key, descriptor.key_type())?;
            let (value, remainder) = map_take(heap, descriptor, map, key)?;
            let option = allocate_option_ref(heap, layouts, semantics[1], value)?;
            let result = super::unique_layout(layouts, semantics[2], 2)?;
            heap.allocate_aggregate_ref(
                result,
                &[
                    ManagedFieldValue::Reference(option.erase()),
                    ManagedFieldValue::Reference(remainder.erase()),
                ],
            )
            .map(|value| value.erase().encoded_abi_word())
        }
        MAP_ITERATOR => {
            let [map] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .map_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let entries = heap.map_entries(descriptor, reference_word(*map)?.cast())?;
            let pair = super::unique_layout(layouts, semantics[2], 2)?;
            let list = layouts
                .collection(semantics[1])
                .and_then(|collection| collection.list_descriptor())
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let mut values = Vec::with_capacity(entries.len());
            for (key, value) in entries {
                let pair = heap.allocate_aggregate_ref(pair, &[key, value])?;
                values.push(ManagedFieldValue::Reference(pair.erase()));
            }
            heap.list_from_elements(list, &values)
                .map(TvmRef::encoded_abi_word)
        }
        MAP_FROM_ENTRY_LIST => {
            let [entries] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .map_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let list = layouts
                .collection(semantics[1])
                .and_then(|collection| collection.list_descriptor())
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let pair = super::unique_layout(layouts, semantics[2], 2)?;
            let entries = heap.list_elements(list, reference_word(*entries)?.cast())?;
            let entries = entries
                .into_iter()
                .map(|entry| {
                    let ManagedFieldValue::Reference(entry) = entry else {
                        return Err(ManagedMemoryError::InvalidAggregateField);
                    };
                    let fields = super::aggregate_fields(heap, pair, entry)?;
                    let [key, value] = fields.as_slice() else {
                        return Err(ManagedMemoryError::InvalidAggregateArity);
                    };
                    Ok((*key, *value))
                })
                .collect::<Result<Vec<_>, _>>()?;
            map_from_entries(heap, descriptor, &entries).map(TvmRef::encoded_abi_word)
        }
        ITERATOR_NEXT => {
            let [iterator] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .list_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let iterator = reference_word(*iterator)?.cast();
            let first = heap.list_first(descriptor, iterator)?;
            let value = if let Some(first) = first {
                let rest = heap
                    .list_rest(descriptor, iterator)?
                    .ok_or(ManagedMemoryError::CorruptedCollection)?;
                let step = super::unique_layout(layouts, semantics[2], 2)?;
                Some(ManagedFieldValue::Reference(
                    heap.allocate_aggregate_ref(
                        step,
                        &[first, ManagedFieldValue::Reference(rest.erase())],
                    )?
                    .erase(),
                ))
            } else {
                None
            };
            allocate_option(heap, layouts, semantics[1], value)
        }
        SET_FROM_LIST => {
            let [elements] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .set_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let list = layouts
                .collection(semantics[1])
                .and_then(|collection| collection.list_descriptor())
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let elements = heap.list_elements(list, reference_word(*elements)?.cast())?;
            set_from_elements(heap, descriptor, &elements).map(TvmRef::encoded_abi_word)
        }
        SET_CONTAINS => {
            let [set, element] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .set_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let set = reference_word(*set)?.cast::<ManagedSet>();
            let element = field_value(*element, descriptor.element_type())?;
            set_contains(heap, descriptor, set, element).map(u64::from)
        }
        SET_EMPTY => {
            let [] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .set_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            heap.set_empty(descriptor).map(TvmRef::encoded_abi_word)
        }
        SET_IS_EMPTY | SET_LENGTH => {
            let [set] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .set_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let set = reference_word(*set)?.cast::<ManagedSet>();
            if operation == SET_IS_EMPTY {
                heap.set_is_empty(descriptor, set).map(u64::from)
            } else {
                let length = heap.set_length(descriptor, set)?;
                i64::try_from(length)
                    .map(|length| u64::from_ne_bytes(length.to_ne_bytes()))
                    .map_err(|_| ManagedMemoryError::InvalidSequenceLength)
            }
        }
        SET_ADD | SET_REMOVE => {
            let [set, element] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .set_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let set = reference_word(*set)?.cast::<ManagedSet>();
            let element = field_value(*element, descriptor.element_type())?;
            let updated = if operation == SET_ADD {
                set_add(heap, descriptor, set, element)?
            } else {
                set_remove(heap, descriptor, set, element)?
            };
            Ok(updated.encoded_abi_word())
        }
        SET_CLEAR => {
            let [set] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .set_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            heap.set_clear(descriptor, reference_word(*set)?.cast())
                .map(TvmRef::encoded_abi_word)
        }
        SET_ITERATOR => {
            let [set] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = collection
                .set_descriptor()
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let elements = heap.set_elements(descriptor, reference_word(*set)?.cast())?;
            let list = layouts
                .collection(semantics[1])
                .and_then(|collection| collection.list_descriptor())
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            heap.list_from_elements(list, &elements)
                .map(TvmRef::encoded_abi_word)
        }
        _ => Err(ManagedMemoryError::InvalidAggregateAbi),
    }
}

fn allocate_option(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    value: Option<ManagedFieldValue>,
) -> Result<u64, ManagedMemoryError> {
    allocate_option_ref(heap, layouts, semantic, value)
        .map(|value| value.erase().encoded_abi_word())
}

fn allocate_option_ref(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    value: Option<ManagedFieldValue>,
) -> Result<TvmRef<super::super::ManagedAggregate>, ManagedMemoryError> {
    let (variant, fields) = match value {
        Some(value) => ("Some", vec![value]),
        None => ("None", Vec::new()),
    };
    let layout = super::option_layout(layouts, semantic, variant, fields.len())?;
    heap.allocate_aggregate_ref(layout, &fields)
}

fn map_get(
    heap: &ActorHeap,
    descriptor: &super::super::ManagedMapDescriptor,
    map: TvmRef<ManagedMap>,
    key: ManagedFieldValue,
) -> Result<Option<ManagedFieldValue>, ManagedMemoryError> {
    if string_keys(descriptor)? {
        heap.map_get(descriptor, map, key, &mut ManagedStringKeySemantics)
    } else {
        heap.map_get(descriptor, map, key, &mut ManagedScalarKeySemantics)
    }
}

fn map_put(
    heap: &mut ActorHeap,
    descriptor: &super::super::ManagedMapDescriptor,
    map: TvmRef<ManagedMap>,
    key: ManagedFieldValue,
    value: ManagedFieldValue,
) -> Result<TvmRef<ManagedMap>, ManagedMemoryError> {
    if string_keys(descriptor)? {
        heap.map_put(descriptor, map, key, value, &mut ManagedStringKeySemantics)
    } else {
        heap.map_put(descriptor, map, key, value, &mut ManagedScalarKeySemantics)
    }
}

fn map_take(
    heap: &mut ActorHeap,
    descriptor: &super::super::ManagedMapDescriptor,
    map: TvmRef<ManagedMap>,
    key: ManagedFieldValue,
) -> Result<(Option<ManagedFieldValue>, TvmRef<ManagedMap>), ManagedMemoryError> {
    if string_keys(descriptor)? {
        heap.map_take(descriptor, map, key, &mut ManagedStringKeySemantics)
    } else {
        heap.map_take(descriptor, map, key, &mut ManagedScalarKeySemantics)
    }
}

fn map_remove(
    heap: &mut ActorHeap,
    descriptor: &super::super::ManagedMapDescriptor,
    map: TvmRef<ManagedMap>,
    key: ManagedFieldValue,
) -> Result<TvmRef<ManagedMap>, ManagedMemoryError> {
    map_take(heap, descriptor, map, key).map(|(_, remainder)| remainder)
}

fn map_from_entries(
    heap: &mut ActorHeap,
    descriptor: &super::super::ManagedMapDescriptor,
    entries: &[(ManagedFieldValue, ManagedFieldValue)],
) -> Result<TvmRef<ManagedMap>, ManagedMemoryError> {
    if string_keys(descriptor)? {
        heap.map_from_entries(descriptor, entries, &mut ManagedStringKeySemantics)
    } else {
        heap.map_from_entries(descriptor, entries, &mut ManagedScalarKeySemantics)
    }
}

fn string_keys(
    descriptor: &super::super::ManagedMapDescriptor,
) -> Result<bool, ManagedMemoryError> {
    Ok(descriptor.key_type()
        == ManagedFieldType::Reference(SemanticTypeId::from_canonical("std.core.String")?))
}

fn set_from_elements(
    heap: &mut ActorHeap,
    descriptor: &super::super::ManagedSetDescriptor,
    elements: &[ManagedFieldValue],
) -> Result<TvmRef<ManagedSet>, ManagedMemoryError> {
    if string_set(descriptor)? {
        heap.set_from_elements(descriptor, elements, &mut ManagedStringKeySemantics)
    } else {
        heap.set_from_elements(descriptor, elements, &mut ManagedScalarKeySemantics)
    }
}

fn set_contains(
    heap: &ActorHeap,
    descriptor: &super::super::ManagedSetDescriptor,
    set: TvmRef<ManagedSet>,
    element: ManagedFieldValue,
) -> Result<bool, ManagedMemoryError> {
    if string_set(descriptor)? {
        heap.set_contains(descriptor, set, element, &mut ManagedStringKeySemantics)
    } else {
        heap.set_contains(descriptor, set, element, &mut ManagedScalarKeySemantics)
    }
}

fn set_add(
    heap: &mut ActorHeap,
    descriptor: &super::super::ManagedSetDescriptor,
    set: TvmRef<ManagedSet>,
    element: ManagedFieldValue,
) -> Result<TvmRef<ManagedSet>, ManagedMemoryError> {
    if string_set(descriptor)? {
        heap.set_add(descriptor, set, element, &mut ManagedStringKeySemantics)
    } else {
        heap.set_add(descriptor, set, element, &mut ManagedScalarKeySemantics)
    }
}

fn set_remove(
    heap: &mut ActorHeap,
    descriptor: &super::super::ManagedSetDescriptor,
    set: TvmRef<ManagedSet>,
    element: ManagedFieldValue,
) -> Result<TvmRef<ManagedSet>, ManagedMemoryError> {
    if string_set(descriptor)? {
        heap.set_remove(descriptor, set, element, &mut ManagedStringKeySemantics)
    } else {
        heap.set_remove(descriptor, set, element, &mut ManagedScalarKeySemantics)
    }
}

fn string_set(descriptor: &super::super::ManagedSetDescriptor) -> Result<bool, ManagedMemoryError> {
    Ok(descriptor.element_type()
        == ManagedFieldType::Reference(SemanticTypeId::from_canonical("std.core.String")?))
}
