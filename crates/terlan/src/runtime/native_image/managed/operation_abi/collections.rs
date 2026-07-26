//! Generated-code operations for concrete persistent collection values.

use super::super::{
    ActorHeap, ManagedFieldType, ManagedFieldValue, ManagedMap, ManagedMemoryError,
    ManagedScalarKeySemantics, ManagedStringKeySemantics, SemanticTypeId, TvmRef,
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
const OPERATION_BYTES: usize = HEADER_BYTES + 16;
const OPTION_OPERATION_BYTES: usize = HEADER_BYTES + 32;
const TRIPLE_OPERATION_BYTES: usize = HEADER_BYTES + 48;

pub(super) fn is_collection_operation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
}

pub fn encode_list_from_elements_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(LIST_FROM_ELEMENTS, semantic)
}

pub fn encode_list_prepend_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(LIST_PREPEND, semantic)
}

pub fn encode_list_append_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(LIST_APPEND, semantic)
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

pub fn encode_list_length_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_result(LIST_LENGTH, semantic, false)
}

pub fn encode_list_get_operation(semantic: SemanticTypeId, reference: bool) -> Vec<u8> {
    operation_with_result(LIST_GET, semantic, reference)
}

pub fn encode_list_first_option_operation(
    list_semantic: SemanticTypeId,
    option_semantic: SemanticTypeId,
) -> Vec<u8> {
    option_operation(LIST_FIRST_OPTION, list_semantic, option_semantic)
}

pub fn encode_list_rest_option_operation(
    list_semantic: SemanticTypeId,
    option_semantic: SemanticTypeId,
) -> Vec<u8> {
    option_operation(LIST_REST_OPTION, list_semantic, option_semantic)
}

pub fn encode_map_contains_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_result(MAP_CONTAINS, semantic, false)
}

pub fn encode_map_get_operation(semantic: SemanticTypeId, reference: bool) -> Vec<u8> {
    operation_with_result(MAP_GET, semantic, reference)
}

pub fn encode_map_empty_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(MAP_EMPTY, semantic)
}

pub fn encode_map_is_empty_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_result(MAP_IS_EMPTY, semantic, false)
}

pub fn encode_map_length_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation_with_result(MAP_LENGTH, semantic, false)
}

pub fn encode_map_get_option_operation(
    map_semantic: SemanticTypeId,
    option_semantic: SemanticTypeId,
) -> Vec<u8> {
    multi_semantic_operation(MAP_GET_OPTION, &[map_semantic, option_semantic])
}

pub fn encode_map_put_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(MAP_PUT, semantic)
}

pub fn encode_map_remove_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(MAP_REMOVE, semantic)
}

pub fn encode_map_clear_operation(semantic: SemanticTypeId) -> Vec<u8> {
    operation(MAP_CLEAR, semantic)
}

pub fn encode_map_take_operation(
    map_semantic: SemanticTypeId,
    option_semantic: SemanticTypeId,
    result_semantic: SemanticTypeId,
) -> Vec<u8> {
    multi_semantic_operation(MAP_TAKE, &[map_semantic, option_semantic, result_semantic])
}

pub fn encode_map_iterator_operation(
    map_semantic: SemanticTypeId,
    list_semantic: SemanticTypeId,
    pair_semantic: SemanticTypeId,
) -> Vec<u8> {
    multi_semantic_operation(MAP_ITERATOR, &[map_semantic, list_semantic, pair_semantic])
}

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

pub(super) fn collection_operation_result_is_reference(encoded: &[u8]) -> bool {
    encoded.get(7) == Some(&1)
        || matches!(
            encoded.get(6).copied(),
            Some(
                LIST_FROM_ELEMENTS
                    | LIST_PREPEND
                    | LIST_APPEND
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
            let mut elements = heap.list_elements(descriptor, tail)?;
            elements.insert(0, field_value(*head, descriptor.element_type())?);
            heap.list_from_elements(descriptor, &elements)
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
            let mut elements = heap.list_elements(descriptor, list)?;
            elements.push(field_value(*value, descriptor.element_type())?);
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

fn option_operation(tag: u8, semantic: SemanticTypeId, option_semantic: SemanticTypeId) -> Vec<u8> {
    multi_semantic_operation(tag, &[semantic, option_semantic])
}

fn multi_semantic_operation(tag: u8, semantics: &[SemanticTypeId]) -> Vec<u8> {
    let mut encoded = operation_with_result(tag, semantics[0], true);
    for semantic in &semantics[1..] {
        encoded.extend_from_slice(&semantic.bytes());
    }
    encoded
}

fn decode(encoded: &[u8]) -> Result<(u8, Vec<SemanticTypeId>, bool), ManagedMemoryError> {
    let semantic_count = match encoded.get(6).copied() {
        Some(LIST_FIRST_OPTION | LIST_REST_OPTION | MAP_GET_OPTION) => 2,
        Some(MAP_TAKE | MAP_ITERATOR | MAP_FROM_ENTRY_LIST | ITERATOR_NEXT) => 3,
        Some(_) => 1,
        None => return Err(ManagedMemoryError::InvalidAggregateAbi),
    };
    let expected_bytes = match semantic_count {
        1 => OPERATION_BYTES,
        2 => OPTION_OPERATION_BYTES,
        3 => TRIPLE_OPERATION_BYTES,
        _ => unreachable!("closed collection semantic count"),
    };
    if encoded.len() != expected_bytes
        || encoded.get(..4) != Some(MAGIC)
        || encoded.get(4..6) != Some(&VERSION.to_le_bytes())
        || encoded[7] > 1
    {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    let semantics = encoded[HEADER_BYTES..]
        .chunks_exact(16)
        .map(|bytes| {
            bytes
                .try_into()
                .map(SemanticTypeId::from_bytes)
                .map_err(|_| ManagedMemoryError::InvalidAggregateAbi)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((encoded[6], semantics, encoded[7] == 1))
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
