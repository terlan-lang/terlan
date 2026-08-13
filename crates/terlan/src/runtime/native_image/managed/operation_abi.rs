//! Bounded managed-value operations invoked by generated native code.

use std::{num::NonZeroUsize, sync::Arc};

use crate::runtime::vm::http_session::VmHttpSessionService;

use super::{
    ActorHeap, ManagedAggregate, ManagedAggregateDescriptor, ManagedFieldValue,
    ManagedLayoutRegistry, ManagedList, ManagedMap, ManagedMemoryError, ManagedStringKeySemantics,
    SemanticTypeId, TvmRef,
};

#[path = "operation_abi/binary_pattern.rs"]
mod binary_pattern;
#[path = "operation_abi/bitstring.rs"]
mod bitstring;
#[path = "operation_abi/bytes.rs"]
mod bytes;
#[path = "operation_abi/collections.rs"]
mod collections;
#[path = "operation_abi/equality.rs"]
mod equality;
#[path = "operation_abi/field.rs"]
mod field;
#[path = "operation_abi/float.rs"]
mod float;
#[path = "operation_abi/http.rs"]
mod http;
#[path = "operation_abi/integer.rs"]
mod integer;
#[path = "operation_abi/json.rs"]
mod json;
#[path = "operation_abi/memory.rs"]
mod memory;
#[path = "operation_abi/pattern.rs"]
mod pattern;
#[path = "operation_abi/projection.rs"]
mod projection;
#[path = "operation_abi/session.rs"]
mod session;
#[path = "operation_abi/string.rs"]
mod string;
#[path = "operation_abi/template.rs"]
mod template;
pub use binary_pattern::{
    encode_binary_pattern_extract_operation, encode_binary_pattern_matches_operation,
    ManagedBinaryPatternEndian, ManagedBinaryPatternField,
};
pub use bitstring::{encode_bitstring_operation, ManagedBitStringOperation};
pub use bytes::{
    encode_bytes_concat_operation, encode_bytes_contains_operation,
    encode_bytes_first_non_ascii_whitespace_operation, encode_bytes_from_list_operation,
    encode_bytes_length_operation, encode_bytes_read_int_be_operation,
    encode_bytes_read_int_le_operation, encode_bytes_read_uint_be_operation,
    encode_bytes_read_uint_le_operation, encode_bytes_slice_operation,
    encode_bytes_starts_with_operation, encode_bytes_to_list_operation,
};
pub use collections::{
    encode_iterator_next_operation, encode_list_append_operation, encode_list_clear_operation,
    encode_list_concat_operation, encode_list_first_operation, encode_list_first_option_operation,
    encode_list_from_elements_operation, encode_list_get_operation, encode_list_is_empty_operation,
    encode_list_length_operation, encode_list_prepend_operation, encode_list_rest_operation,
    encode_list_rest_option_operation, encode_list_subtract_operation, encode_map_clear_operation,
    encode_map_contains_operation, encode_map_empty_operation, encode_map_from_entries_operation,
    encode_map_from_entry_list_operation, encode_map_get_operation,
    encode_map_get_option_operation, encode_map_is_empty_operation, encode_map_iterator_operation,
    encode_map_length_operation, encode_map_put_operation, encode_map_remove_operation,
    encode_map_take_operation, encode_set_add_operation, encode_set_clear_operation,
    encode_set_contains_operation, encode_set_empty_operation, encode_set_from_list_operation,
    encode_set_is_empty_operation, encode_set_iterator_operation, encode_set_length_operation,
    encode_set_remove_operation,
};
pub use equality::encode_managed_value_equal_operation;
pub(super) use field::field_word;
pub use float::{
    encode_float_from_string_operation, encode_float_log_operation,
    encode_float_to_string_operation,
};
pub use http::{
    encode_cookie_header_operation, encode_response_build_operation,
    encode_response_cookie_jar_operation, encode_response_security_headers_operation,
    ManagedCookieHeaderOperation,
};
pub use integer::{
    encode_int_from_string_base_operation, encode_int_from_string_operation,
    encode_int_to_string_base_operation, encode_int_to_string_operation,
};
pub use json::{encode_json_parse_result_operation, encode_result_is_ok_operation};
pub use memory::{encode_memory_retained_size_operation, encode_memory_shallow_size_operation};
pub use pattern::{encode_managed_type_is_operation, encode_managed_variant_is_operation};
pub(crate) use projection::{decode_aggregate_field_projection, scalar_string_projection_rewrite};
pub use session::{
    encode_session_current_operation, encode_session_expire_operation,
    encode_session_get_operation, encode_session_mutation_operation,
    encode_session_option_is_none_operation, encode_session_rotate_operation,
    encode_session_with_response_operation, ManagedSessionMutation,
};
use string::{
    append_strings, concatenate_strings, join_string_list, prepend_string_literal, strings_equal,
    transform_string,
};
pub use string::{
    encode_string_byte_size_operation, encode_string_characters_operation,
    encode_string_codepoints_operation, encode_string_compare_operation,
    encode_string_contains_operation, encode_string_ends_with_operation,
    encode_string_length_operation, encode_string_lowercase_operation,
    encode_string_replace_operation, encode_string_sha256_operation,
    encode_string_split_once_operation, encode_string_split_operation,
    encode_string_starts_with_operation, encode_string_trim_end_operation,
    encode_string_trim_operation, encode_string_trim_start_operation,
    encode_string_utf8_byte_at_operation, encode_string_utf8_find_any_byte_operation,
    encode_string_utf8_slice_operation,
};
pub use template::{encode_template_render_operation, ManagedTemplateValueKind};

const MAGIC: &[u8; 4] = b"TVMO";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 8;
const PROJECT: u8 = 1;
const STRING_MAP_GET_OPTION: u8 = 2;
const LIST_EMPTY: u8 = 3;
const AGGREGATE_REPLACE_FIELD: u8 = 4;
const AGGREGATE_APPEND_PAIR: u8 = 5;
const AGGREGATE_APPEND_VALUE: u8 = 6;
const STRING_EQUAL: u8 = 7;
const STRING_APPEND: u8 = 8;
const PROJECT_SCALAR: u8 = 9;
const STRING_LIST_JOIN: u8 = 10;
const STRING_ESCAPE_HTML_TEXT: u8 = 11;
const STRING_ESCAPE_HTML_ATTRIBUTE: u8 = 12;
const STRING_PREPEND_LITERAL: u8 = 13;
const STRING_PREPEND_PROJECTED_LITERAL: u8 = 14;
const STRING_CONCAT: u8 = 15;
const MEMORY_SHALLOW_SIZE: u8 = 16;
const MEMORY_RETAINED_SIZE: u8 = 17;
const SEMANTIC_BYTES: usize = 16;
const PROJECT_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES + 4;
const MAP_GET_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES * 2;
const LIST_EMPTY_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES;
const REPLACE_FIELD_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES + 4;
const APPEND_PAIR_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES * 3 + 4;
const APPEND_VALUE_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES * 2 + 4;

/// Reports whether bytes identify the managed-operation ABI family.
pub fn is_managed_operation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
        || binary_pattern::is_binary_pattern_operation(encoded)
        || bitstring::is_bitstring_operation(encoded)
        || bytes::is_bytes_operation(encoded)
        || collections::is_collection_operation(encoded)
        || equality::is_equality_operation(encoded)
        || float::is_float_operation(encoded)
        || http::is_http_operation(encoded)
        || integer::is_integer_operation(encoded)
        || json::is_json_operation(encoded)
        || pattern::is_pattern_operation(encoded)
        || session::is_session_operation(encoded)
        || string::is_string_operation(encoded)
        || template::is_template_operation(encoded)
}

/// Reports whether one admitted managed ABI payload returns an opaque reference.
pub(crate) fn managed_abi_result_is_reference(encoded: &[u8]) -> bool {
    if super::is_closure_allocation(encoded) {
        return true;
    }
    if !is_managed_operation(encoded) || http::is_http_operation(encoded) {
        return true;
    }
    if json::is_json_operation(encoded) {
        return json::json_operation_result_is_reference(encoded);
    }
    if binary_pattern::is_binary_pattern_operation(encoded) {
        return binary_pattern::binary_pattern_result_is_reference(encoded);
    }
    if bitstring::is_bitstring_operation(encoded) {
        return bitstring::bitstring_result_is_reference(encoded);
    }
    if bytes::is_bytes_operation(encoded) {
        return bytes::bytes_operation_result_is_reference(encoded);
    }
    if session::is_session_operation(encoded) {
        return session::session_operation_result_is_reference(encoded);
    }
    if pattern::is_pattern_operation(encoded) {
        return false;
    }
    if collections::is_collection_operation(encoded) {
        return collections::collection_operation_result_is_reference(encoded);
    }
    if equality::is_equality_operation(encoded) {
        return false;
    }
    if float::is_float_operation(encoded) {
        return float::float_operation_result_is_reference(encoded);
    }
    if integer::is_integer_operation(encoded) {
        return true;
    }
    if string::is_string_operation(encoded) {
        return string::string_operation_result_is_reference(encoded);
    }
    !matches!(
        decode_operation(encoded),
        Ok(ManagedOperation::ProjectScalar { .. }
            | ManagedOperation::StringEqual
            | ManagedOperation::MemoryShallowSize
            | ManagedOperation::MemoryRetainedSize)
    )
}

/// Encodes one checked aggregate field projection operation.
pub fn encode_aggregate_field_operation(
    semantic: SemanticTypeId,
    field: usize,
) -> Result<Vec<u8>, ManagedMemoryError> {
    let field = u32::try_from(field).map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    let mut encoded = header(PROJECT);
    encoded.extend_from_slice(&semantic.bytes());
    encoded.extend_from_slice(&field.to_le_bytes());
    Ok(encoded)
}

/// Encodes one checked aggregate field projection returning a scalar native word.
pub fn encode_aggregate_scalar_field_operation(
    semantic: SemanticTypeId,
    field: usize,
) -> Result<Vec<u8>, ManagedMemoryError> {
    let field = u32::try_from(field).map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    let mut encoded = header(PROJECT_SCALAR);
    encoded.extend_from_slice(&semantic.bytes());
    encoded.extend_from_slice(&field.to_le_bytes());
    Ok(encoded)
}

/// Encodes one managed string-map lookup returning `Option[String]`.
pub fn encode_string_map_get_option_operation(
    map_semantic: SemanticTypeId,
    option_semantic: SemanticTypeId,
) -> Vec<u8> {
    let mut encoded = header(STRING_MAP_GET_OPTION);
    encoded.extend_from_slice(&map_semantic.bytes());
    encoded.extend_from_slice(&option_semantic.bytes());
    encoded
}

/// Encodes allocation of one empty persistent managed list.
pub fn encode_list_empty_operation(list_semantic: SemanticTypeId) -> Vec<u8> {
    let mut encoded = header(LIST_EMPTY);
    encoded.extend_from_slice(&list_semantic.bytes());
    encoded
}

/// Encodes immutable replacement of one aggregate field.
pub fn encode_aggregate_replace_field_operation(
    aggregate_semantic: SemanticTypeId,
    field: usize,
) -> Result<Vec<u8>, ManagedMemoryError> {
    let field = u32::try_from(field).map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    let mut encoded = header(AGGREGATE_REPLACE_FIELD);
    encoded.extend_from_slice(&aggregate_semantic.bytes());
    encoded.extend_from_slice(&field.to_le_bytes());
    Ok(encoded)
}

/// Encodes persistent append of a two-field aggregate into an aggregate list field.
pub fn encode_aggregate_append_pair_operation(
    aggregate_semantic: SemanticTypeId,
    list_semantic: SemanticTypeId,
    pair_semantic: SemanticTypeId,
    field: usize,
) -> Result<Vec<u8>, ManagedMemoryError> {
    let field = u32::try_from(field).map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    let mut encoded = header(AGGREGATE_APPEND_PAIR);
    encoded.extend_from_slice(&aggregate_semantic.bytes());
    encoded.extend_from_slice(&list_semantic.bytes());
    encoded.extend_from_slice(&pair_semantic.bytes());
    encoded.extend_from_slice(&field.to_le_bytes());
    Ok(encoded)
}

/// Encodes persistent append of one value into an aggregate-owned list field.
pub fn encode_aggregate_append_value_operation(
    aggregate_semantic: SemanticTypeId,
    list_semantic: SemanticTypeId,
    field: usize,
) -> Result<Vec<u8>, ManagedMemoryError> {
    let field = u32::try_from(field).map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    let mut encoded = header(AGGREGATE_APPEND_VALUE);
    encoded.extend_from_slice(&aggregate_semantic.bytes());
    encoded.extend_from_slice(&list_semantic.bytes());
    encoded.extend_from_slice(&field.to_le_bytes());
    Ok(encoded)
}

/// Encodes checked value equality between two managed UTF-8 strings.
pub fn encode_string_equal_operation() -> Vec<u8> {
    header(STRING_EQUAL)
}

/// Encodes checked concatenation of two managed UTF-8 strings.
pub fn encode_string_append_operation() -> Vec<u8> {
    header(STRING_APPEND)
}

/// Encodes checked concatenation of two or more managed UTF-8 strings.
pub fn encode_string_concat_operation() -> Vec<u8> {
    header(STRING_CONCAT)
}

/// Encodes concatenation of one image-owned UTF-8 prefix and one managed string.
pub fn encode_string_prepend_literal_operation(
    literal: &str,
) -> Result<Vec<u8>, ManagedMemoryError> {
    let length =
        u32::try_from(literal.len()).map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    let mut encoded = header(STRING_PREPEND_LITERAL);
    encoded.extend_from_slice(&length.to_le_bytes());
    encoded.extend_from_slice(literal.as_bytes());
    Ok(encoded)
}

/// Encodes projection and literal-prefix concatenation as one managed call.
pub fn encode_string_prepend_projected_literal_operation(
    semantic: SemanticTypeId,
    field: usize,
    literal: &str,
) -> Result<Vec<u8>, ManagedMemoryError> {
    let field = u32::try_from(field).map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    let length =
        u32::try_from(literal.len()).map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    let mut encoded = header(STRING_PREPEND_PROJECTED_LITERAL);
    encoded.extend_from_slice(&semantic.bytes());
    encoded.extend_from_slice(&field.to_le_bytes());
    encoded.extend_from_slice(&length.to_le_bytes());
    encoded.extend_from_slice(literal.as_bytes());
    Ok(encoded)
}

/// Encodes checked concatenation of one managed list of UTF-8 strings.
pub fn encode_string_list_join_operation() -> Vec<u8> {
    header(STRING_LIST_JOIN)
}

/// Encodes checked HTML text escaping of one managed UTF-8 string.
pub fn encode_string_escape_html_text_operation() -> Vec<u8> {
    header(STRING_ESCAPE_HTML_TEXT)
}

/// Encodes checked HTML attribute escaping of one managed UTF-8 string.
pub fn encode_string_escape_html_attribute_operation() -> Vec<u8> {
    header(STRING_ESCAPE_HTML_ATTRIBUTE)
}

/// Executes one operation with optional VM-owned request services.
pub(crate) fn execute_managed_operation_with_context(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    http_sessions: Option<&VmHttpSessionService>,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    // The generic family is the most common generated scalar/aggregate ABI.
    // Select it from its authenticated magic once instead of probing every
    // specialized family before decoding the operation.
    if !encoded.starts_with(MAGIC) {
        if http::is_http_operation(encoded) {
            return http::execute_http_operation(heap, layouts, encoded, words);
        }
        if binary_pattern::is_binary_pattern_operation(encoded) {
            return binary_pattern::execute_binary_pattern_operation(heap, encoded, words);
        }
        if bitstring::is_bitstring_operation(encoded) {
            return bitstring::execute_bitstring_operation(heap, encoded, words);
        }
        if bytes::is_bytes_operation(encoded) {
            return bytes::execute_bytes_operation(heap, layouts, encoded, words);
        }
        if collections::is_collection_operation(encoded) {
            return collections::execute_collection_operation(heap, layouts, encoded, words);
        }
        if equality::is_equality_operation(encoded) {
            return equality::execute_equality_operation(heap, layouts, encoded, words);
        }
        if float::is_float_operation(encoded) {
            return float::execute_float_operation(heap, layouts, encoded, words);
        }
        if integer::is_integer_operation(encoded) {
            return integer::execute_integer_operation(heap, layouts, encoded, words);
        }
        if json::is_json_operation(encoded) {
            return json::execute_json_operation(heap, layouts, encoded, words);
        }
        if pattern::is_pattern_operation(encoded) {
            return pattern::execute_pattern_operation(heap, layouts, encoded, words);
        }
        if session::is_session_operation(encoded) {
            return session::execute_session_operation(
                heap,
                layouts,
                http_sessions,
                encoded,
                words,
            );
        }
        if string::is_string_operation(encoded) {
            return string::execute_string_operation(heap, layouts, encoded, words);
        }
        if template::is_template_operation(encoded) {
            return template::execute_template_operation(heap, layouts, encoded, words);
        }
    }
    let operation = decode_operation(encoded)?;
    let reference = match operation {
        ManagedOperation::Project { semantic, field } => {
            let [aggregate] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            return project_field(heap, layouts, semantic, field, *aggregate);
        }
        ManagedOperation::ProjectScalar { semantic, field } => {
            let [aggregate] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            return project_field(heap, layouts, semantic, field, *aggregate);
        }
        ManagedOperation::StringMapGetOption {
            map_semantic,
            option_semantic,
        } => {
            let [map, key] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            lookup_string_map(heap, layouts, map_semantic, option_semantic, *map, *key)?.erase()
        }
        ManagedOperation::ListEmpty { list_semantic } => {
            let [] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            allocate_empty_list(heap, layouts, list_semantic)?
        }
        ManagedOperation::AggregateReplaceField { semantic, field } => {
            let [aggregate, replacement] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            replace_aggregate_field(heap, layouts, semantic, field, *aggregate, *replacement)?
        }
        ManagedOperation::AggregateAppendPair {
            aggregate_semantic,
            list_semantic,
            pair_semantic,
            field,
        } => {
            let [aggregate, first, second] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            append_pair_to_aggregate_list(
                heap,
                layouts,
                AggregateListTarget {
                    aggregate_semantic,
                    list_semantic,
                    pair_semantic,
                    field,
                    aggregate: *aggregate,
                },
                *first,
                *second,
            )?
        }
        ManagedOperation::AggregateAppendValue {
            aggregate_semantic,
            list_semantic,
            field,
        } => {
            let [aggregate, value] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            append_value_to_aggregate_list(
                heap,
                layouts,
                aggregate_semantic,
                list_semantic,
                field,
                *aggregate,
                *value,
            )?
        }
        ManagedOperation::StringEqual => {
            let [left, right] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            return strings_equal(heap, *left, *right).map(u64::from);
        }
        ManagedOperation::StringAppend => {
            let [left, right] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            append_strings(heap, *left, *right)?.erase()
        }
        ManagedOperation::StringConcat => {
            if words.len() < 2 {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            }
            concatenate_strings(heap, words)?.erase()
        }
        ManagedOperation::StringPrependLiteral(literal) => {
            let [right] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            prepend_string_literal(heap, literal, *right)?.erase()
        }
        ManagedOperation::StringPrependProjectedLiteral {
            semantic,
            field,
            literal,
        } => {
            let [aggregate] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let right = project_field(heap, layouts, semantic, field, *aggregate)?;
            prepend_string_literal(heap, literal, i64::from_ne_bytes(right.to_ne_bytes()))?.erase()
        }
        ManagedOperation::StringListJoin => {
            let [list] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            join_string_list(heap, layouts, *list)?.erase()
        }
        ManagedOperation::StringEscapeHtmlText => {
            let [value] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            transform_string(heap, *value, crate::terlan_html::escape_html_text)?.erase()
        }
        ManagedOperation::StringEscapeHtmlAttribute => {
            let [value] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            transform_string(heap, *value, crate::terlan_html::escape_html_attr)?.erase()
        }
        ManagedOperation::MemoryShallowSize => {
            let [value] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let reference = reference_word(*value)?;
            let size = heap.shallow_size(reference)?;
            return u64::try_from(size).map_err(|_| ManagedMemoryError::AllocationLimitExceeded);
        }
        ManagedOperation::MemoryRetainedSize => {
            let [value] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let reference = reference_word(*value)?;
            let size = heap.retained_size(reference)?;
            return u64::try_from(size).map_err(|_| ManagedMemoryError::AllocationLimitExceeded);
        }
    };
    Ok(reference.encoded_abi_word())
}

#[path = "operation_abi/codec.rs"]
mod codec;
use codec::{decode_operation, header, ManagedOperation};

fn field_at(encoded: &[u8], offset: usize) -> Result<usize, ManagedMemoryError> {
    encoded
        .get(offset..offset + 4)
        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
        .map(u32::from_le_bytes)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(ManagedMemoryError::InvalidAggregateAbi)
}

/// Reads one exact trailing UTF-8 literal prefixed by its little-endian length.
fn literal_at(encoded: &[u8], offset: usize) -> Result<&str, ManagedMemoryError> {
    let length = encoded
        .get(offset..offset + 4)
        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
        .map(u32::from_le_bytes)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(ManagedMemoryError::InvalidAggregateAbi)?;
    let literal = encoded
        .get(offset + 4..)
        .filter(|literal| literal.len() == length)
        .ok_or(ManagedMemoryError::InvalidAggregateAbi)?;
    std::str::from_utf8(literal).map_err(|_| ManagedMemoryError::InvalidUtf8)
}

/// Reads one fixed-width semantic identity from encoded operation bytes.
fn semantic_at(encoded: &[u8], offset: usize) -> Result<SemanticTypeId, ManagedMemoryError> {
    encoded
        .get(offset..offset + SEMANTIC_BYTES)
        .and_then(|bytes| <[u8; SEMANTIC_BYTES]>::try_from(bytes).ok())
        .map(SemanticTypeId::from_bytes)
        .ok_or(ManagedMemoryError::InvalidAggregateAbi)
}

/// Projects one checked field into its fixed native word representation.
fn project_field(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    field: usize,
    aggregate: i64,
) -> Result<u64, ManagedMemoryError> {
    let reference = reference_word(aggregate)?;
    let descriptor = layouts
        .layout_for_reference(heap, semantic, reference)
        .map_err(|_| ManagedMemoryError::ManagedTypeMismatch)?;
    let value = heap
        .read_aggregate(reference.cast::<ManagedAggregate>(), descriptor)?
        .field(field)?;
    Ok(field_word(value))
}

/// Looks up one string map and wraps the result in its managed option union.
fn lookup_string_map(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    map_semantic: SemanticTypeId,
    option_semantic: SemanticTypeId,
    map: i64,
    key: i64,
) -> Result<TvmRef<ManagedAggregate>, ManagedMemoryError> {
    let map = reference_word(map)?;
    let key = reference_word(key)?;
    let collection = layouts
        .collection(map_semantic)
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
    let descriptor = collection
        .map_descriptor()
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
    let value = heap.map_get(
        descriptor,
        map.cast::<ManagedMap>(),
        ManagedFieldValue::Reference(key),
        &mut ManagedStringKeySemantics,
    )?;
    let (variant, fields) = match value {
        Some(ManagedFieldValue::Reference(value)) => {
            ("Some", vec![ManagedFieldValue::Reference(value)])
        }
        Some(_) => return Err(ManagedMemoryError::ManagedTypeMismatch),
        None => ("None", Vec::new()),
    };
    let option = option_layout(layouts, option_semantic, variant, fields.len())?;
    heap.allocate_aggregate_ref(option, &fields)
}

/// Allocates one empty list from its admitted collection descriptor.
fn allocate_empty_list(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
) -> Result<TvmRef<()>, ManagedMemoryError> {
    let descriptor = layouts
        .collection(semantic)
        .and_then(|collection| collection.list_descriptor())
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
    heap.list_from_elements(descriptor, &[]).map(TvmRef::erase)
}

/// Rebuilds one aggregate after replacing a field with a checked native word.
fn replace_aggregate_field(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    field: usize,
    aggregate: i64,
    replacement: i64,
) -> Result<TvmRef<()>, ManagedMemoryError> {
    let reference = reference_word(aggregate)?;
    let descriptor = layouts
        .layout_for_reference(heap, semantic, reference)
        .map_err(|_| ManagedMemoryError::ManagedTypeMismatch)?;
    let mut values = aggregate_fields(heap, descriptor, reference)?;
    let field_type = descriptor
        .fields()
        .get(field)
        .ok_or(ManagedMemoryError::InvalidAggregateField)?
        .field_type();
    values[field] = field_value(replacement, field_type)?;
    heap.allocate_aggregate_ref(descriptor, &values)
        .map(TvmRef::erase)
}

/// Persistently appends one pair into an aggregate-owned list and rebuilds the owner.
pub(super) struct AggregateListTarget {
    pub(super) aggregate_semantic: SemanticTypeId,
    pub(super) list_semantic: SemanticTypeId,
    pub(super) pair_semantic: SemanticTypeId,
    pub(super) field: usize,
    pub(super) aggregate: i64,
}

fn append_pair_to_aggregate_list(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    target: AggregateListTarget,
    first: i64,
    second: i64,
) -> Result<TvmRef<()>, ManagedMemoryError> {
    let AggregateListTarget {
        aggregate_semantic,
        list_semantic,
        pair_semantic,
        field,
        aggregate,
    } = target;
    let aggregate = reference_word(aggregate)?;
    let aggregate_layout = layouts
        .layout_for_reference(heap, aggregate_semantic, aggregate)
        .map_err(|_| ManagedMemoryError::ManagedTypeMismatch)?;
    let mut aggregate_values = aggregate_fields(heap, aggregate_layout, aggregate)?;
    let list = match aggregate_values.get(field).copied() {
        Some(ManagedFieldValue::Reference(reference)) => reference.cast::<ManagedList>(),
        _ => return Err(ManagedMemoryError::InvalidAggregateField),
    };
    let list_descriptor = layouts
        .collection(list_semantic)
        .and_then(|collection| collection.list_descriptor())
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
    let pair_layout = unique_layout(layouts, pair_semantic, 2)?;
    let pair_values = pair_layout
        .fields()
        .iter()
        .zip([first, second])
        .map(|(descriptor, word)| field_value(word, descriptor.field_type()))
        .collect::<Result<Vec<_>, _>>()?;
    let pair = heap.allocate_aggregate_ref(pair_layout, &pair_values)?;
    let list = heap.list_append(
        list_descriptor,
        list,
        ManagedFieldValue::Reference(pair.erase()),
    )?;
    aggregate_values[field] = ManagedFieldValue::Reference(list.erase());
    heap.allocate_aggregate_ref(aggregate_layout, &aggregate_values)
        .map(TvmRef::erase)
}

/// Persistently appends one checked value into an aggregate-owned list.
fn append_value_to_aggregate_list(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    aggregate_semantic: SemanticTypeId,
    list_semantic: SemanticTypeId,
    field: usize,
    aggregate: i64,
    value: i64,
) -> Result<TvmRef<()>, ManagedMemoryError> {
    let aggregate = reference_word(aggregate)?;
    let aggregate_layout = layouts
        .layout_for_reference(heap, aggregate_semantic, aggregate)
        .map_err(|_| ManagedMemoryError::ManagedTypeMismatch)?;
    let mut aggregate_values = aggregate_fields(heap, aggregate_layout, aggregate)?;
    let list = match aggregate_values.get(field).copied() {
        Some(ManagedFieldValue::Reference(reference)) => reference.cast::<ManagedList>(),
        _ => return Err(ManagedMemoryError::InvalidAggregateField),
    };
    let list_descriptor = layouts
        .collection(list_semantic)
        .and_then(|collection| collection.list_descriptor())
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
    let value = field_value(value, list_descriptor.element_type())?;
    let list = heap.list_append(list_descriptor, list, value)?;
    aggregate_values[field] = ManagedFieldValue::Reference(list.erase());
    heap.allocate_aggregate_ref(aggregate_layout, &aggregate_values)
        .map(TvmRef::erase)
}

/// Reads every field before a persistent aggregate rebuild begins allocating.
pub(super) fn aggregate_fields(
    heap: &ActorHeap,
    descriptor: &ManagedAggregateDescriptor,
    reference: TvmRef<()>,
) -> Result<Vec<ManagedFieldValue>, ManagedMemoryError> {
    let view = heap.read_aggregate(reference.cast::<ManagedAggregate>(), descriptor)?;
    (0..descriptor.fields().len())
        .map(|index| view.field(index))
        .collect()
}

/// Selects the sole admitted fixed-arity layout for one helper aggregate.
pub(super) fn unique_layout(
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    arity: usize,
) -> Result<&ManagedAggregateDescriptor, ManagedMemoryError> {
    let mut matching = layouts
        .layouts(semantic)
        .iter()
        .filter(|layout| layout.fields().len() == arity);
    let layout = matching
        .next()
        .map(Arc::as_ref)
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
    if matching.next().is_some() {
        return Err(ManagedMemoryError::ManagedTypeMismatch);
    }
    Ok(layout)
}

/// Converts one native word into the exact managed field category requested.
fn field_value(
    word: i64,
    field_type: super::ManagedFieldType,
) -> Result<ManagedFieldValue, ManagedMemoryError> {
    match field_type {
        super::ManagedFieldType::Unit if word == 0 => Ok(ManagedFieldValue::Unit),
        super::ManagedFieldType::Int => Ok(ManagedFieldValue::Int(word)),
        super::ManagedFieldType::Bool => match word {
            0 => Ok(ManagedFieldValue::Bool(false)),
            1 => Ok(ManagedFieldValue::Bool(true)),
            _ => Err(ManagedMemoryError::InvalidManagedScalar),
        },
        super::ManagedFieldType::Reference(_) => {
            reference_word(word).map(ManagedFieldValue::Reference)
        }
        super::ManagedFieldType::Float => {
            let value = f64::from_bits(u64::from_ne_bytes(word.to_ne_bytes()));
            value
                .is_finite()
                .then_some(ManagedFieldValue::Float(value))
                .ok_or(ManagedMemoryError::InvalidManagedScalar)
        }
        super::ManagedFieldType::Atom => u32::try_from(word)
            .map(super::AtomIndex::from_runtime)
            .map(ManagedFieldValue::Atom)
            .map_err(|_| ManagedMemoryError::InvalidManagedScalar),
        _ => Err(ManagedMemoryError::InvalidAggregateField),
    }
}

/// Selects the exact admitted option variant used by one lookup result.
fn option_layout<'a>(
    layouts: &'a ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    variant: &str,
    arity: usize,
) -> Result<&'a ManagedAggregateDescriptor, ManagedMemoryError> {
    layouts
        .layouts(semantic)
        .iter()
        .find(|layout| layout.variant_name() == Some(variant) && layout.fields().len() == arity)
        .map(Arc::as_ref)
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)
}

/// Decodes one nonzero native reference word.
fn reference_word(word: i64) -> Result<TvmRef<()>, ManagedMemoryError> {
    usize::try_from(u64::from_ne_bytes(word.to_ne_bytes()))
        .ok()
        .and_then(NonZeroUsize::new)
        .map(TvmRef::from_encoded)
        .ok_or(ManagedMemoryError::InvalidAggregateField)
}

#[cfg(test)]
#[path = "operation_abi_test.rs"]
#[cfg(test)]
mod operation_abi_test;
#[cfg(test)]
pub(crate) use operation_abi_test::execute_managed_operation;
