//! Bounded managed-value operations invoked by generated native code.

use std::num::NonZeroUsize;
use std::sync::Arc;

use crate::runtime::vm::http_session::VmHttpSessionService;

use super::{
    ActorHeap, ManagedAggregate, ManagedAggregateDescriptor, ManagedFieldValue,
    ManagedLayoutRegistry, ManagedList, ManagedMap, ManagedMemoryError, ManagedString,
    ManagedStringKeySemantics, SemanticTypeId, TvmRef,
};

#[path = "operation_abi/binary_pattern.rs"]
mod binary_pattern;
#[path = "operation_abi/collections.rs"]
mod collections;
#[path = "operation_abi/equality.rs"]
mod equality;
#[path = "operation_abi/http.rs"]
mod http;
#[path = "operation_abi/json.rs"]
mod json;
#[path = "operation_abi/pattern.rs"]
mod pattern;
#[path = "operation_abi/session.rs"]
mod session;
#[path = "operation_abi/template.rs"]
mod template;
pub use binary_pattern::{
    encode_binary_pattern_extract_operation, encode_binary_pattern_matches_operation,
    ManagedBinaryPatternEndian, ManagedBinaryPatternField,
};
pub use collections::{
    encode_list_first_operation, encode_list_from_elements_operation,
    encode_list_is_empty_operation, encode_list_prepend_operation, encode_list_rest_operation,
    encode_map_contains_operation, encode_map_from_entries_operation, encode_map_get_operation,
};
pub use equality::encode_managed_value_equal_operation;
pub use http::{
    encode_cookie_header_operation, encode_response_cookie_jar_operation,
    encode_response_security_headers_operation, ManagedCookieHeaderOperation,
};
pub use json::{encode_json_parse_result_operation, encode_result_is_ok_operation};
pub use pattern::{encode_managed_type_is_operation, encode_managed_variant_is_operation};
pub use session::{
    encode_session_current_operation, encode_session_expire_operation,
    encode_session_get_operation, encode_session_mutation_operation,
    encode_session_option_is_none_operation, encode_session_rotate_operation,
    encode_session_with_response_operation, ManagedSessionMutation,
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
        || collections::is_collection_operation(encoded)
        || equality::is_equality_operation(encoded)
        || http::is_http_operation(encoded)
        || json::is_json_operation(encoded)
        || pattern::is_pattern_operation(encoded)
        || session::is_session_operation(encoded)
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
    !matches!(
        decode_operation(encoded),
        Ok(ManagedOperation::ProjectScalar { .. } | ManagedOperation::StringEqual)
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

/// Executes one decoded operation against the current actor heap.
#[cfg(test)]
pub(crate) fn execute_managed_operation(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    execute_managed_operation_with_context(heap, layouts, None, encoded, words)
}

/// Executes one operation with optional VM-owned request services.
pub(crate) fn execute_managed_operation_with_context(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    http_sessions: Option<&VmHttpSessionService>,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    if http::is_http_operation(encoded) {
        return http::execute_http_operation(heap, layouts, encoded, words);
    }
    if binary_pattern::is_binary_pattern_operation(encoded) {
        return binary_pattern::execute_binary_pattern_operation(heap, encoded, words);
    }
    if collections::is_collection_operation(encoded) {
        return collections::execute_collection_operation(heap, layouts, encoded, words);
    }
    if equality::is_equality_operation(encoded) {
        return equality::execute_equality_operation(heap, layouts, encoded, words);
    }
    if json::is_json_operation(encoded) {
        return json::execute_json_operation(heap, layouts, encoded, words);
    }
    if pattern::is_pattern_operation(encoded) {
        return pattern::execute_pattern_operation(heap, layouts, encoded, words);
    }
    if session::is_session_operation(encoded) {
        return session::execute_session_operation(heap, layouts, http_sessions, encoded, words);
    }
    if template::is_template_operation(encoded) {
        return template::execute_template_operation(heap, layouts, encoded, words);
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
                aggregate_semantic,
                list_semantic,
                pair_semantic,
                field,
                *aggregate,
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
    };
    Ok(reference.encoded_abi_word())
}

/// Closed operation payload decoded from immutable image data.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ManagedOperation {
    /// Projects one physical field from an aggregate with the expected identity.
    Project {
        semantic: SemanticTypeId,
        field: usize,
    },
    /// Projects one scalar physical field from an aggregate with the expected identity.
    ProjectScalar {
        semantic: SemanticTypeId,
        field: usize,
    },
    /// Looks up one managed string key and allocates `None` or `Some(value)`.
    StringMapGetOption {
        map_semantic: SemanticTypeId,
        option_semantic: SemanticTypeId,
    },
    /// Allocates an empty persistent list with the admitted collection schema.
    ListEmpty { list_semantic: SemanticTypeId },
    /// Rebuilds an immutable aggregate with one physical field replaced.
    AggregateReplaceField {
        semantic: SemanticTypeId,
        field: usize,
    },
    /// Appends a two-field value to one persistent list field and rebuilds its owner.
    AggregateAppendPair {
        aggregate_semantic: SemanticTypeId,
        list_semantic: SemanticTypeId,
        pair_semantic: SemanticTypeId,
        field: usize,
    },
    /// Appends one checked value to an aggregate-owned persistent list.
    AggregateAppendValue {
        aggregate_semantic: SemanticTypeId,
        list_semantic: SemanticTypeId,
        field: usize,
    },
    /// Compares two validated managed strings by UTF-8 value.
    StringEqual,
    /// Allocates the UTF-8 concatenation of two validated managed strings.
    StringAppend,
    /// Allocates the UTF-8 concatenation of a checked managed string list.
    StringListJoin,
    /// Escapes one checked string for HTML text context.
    StringEscapeHtmlText,
    /// Escapes one checked string for a quoted HTML attribute context.
    StringEscapeHtmlAttribute,
}

/// Builds the common immutable operation header.
fn header(operation: u8) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(MAP_GET_BYTES);
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&VERSION.to_le_bytes());
    encoded.push(operation);
    encoded.push(0);
    encoded
}

/// Decodes one exact operation payload and rejects extensions or truncation.
fn decode_operation(encoded: &[u8]) -> Result<ManagedOperation, ManagedMemoryError> {
    if encoded.len() < HEADER_BYTES
        || encoded.get(..4) != Some(MAGIC)
        || encoded.get(4..6) != Some(&VERSION.to_le_bytes())
        || encoded[7] != 0
    {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    match encoded[6] {
        PROJECT if encoded.len() == PROJECT_BYTES => {
            let semantic = semantic_at(encoded, HEADER_BYTES)?;
            let field = encoded
                .get(HEADER_BYTES + SEMANTIC_BYTES..PROJECT_BYTES)
                .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
                .map(u32::from_le_bytes)
                .and_then(|value| usize::try_from(value).ok())
                .ok_or(ManagedMemoryError::InvalidAggregateAbi)?;
            Ok(ManagedOperation::Project { semantic, field })
        }
        PROJECT_SCALAR if encoded.len() == PROJECT_BYTES => {
            let semantic = semantic_at(encoded, HEADER_BYTES)?;
            let field = field_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?;
            Ok(ManagedOperation::ProjectScalar { semantic, field })
        }
        STRING_MAP_GET_OPTION if encoded.len() == MAP_GET_BYTES => {
            Ok(ManagedOperation::StringMapGetOption {
                map_semantic: semantic_at(encoded, HEADER_BYTES)?,
                option_semantic: semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?,
            })
        }
        LIST_EMPTY if encoded.len() == LIST_EMPTY_BYTES => Ok(ManagedOperation::ListEmpty {
            list_semantic: semantic_at(encoded, HEADER_BYTES)?,
        }),
        AGGREGATE_REPLACE_FIELD if encoded.len() == REPLACE_FIELD_BYTES => {
            Ok(ManagedOperation::AggregateReplaceField {
                semantic: semantic_at(encoded, HEADER_BYTES)?,
                field: field_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?,
            })
        }
        AGGREGATE_APPEND_PAIR if encoded.len() == APPEND_PAIR_BYTES => {
            Ok(ManagedOperation::AggregateAppendPair {
                aggregate_semantic: semantic_at(encoded, HEADER_BYTES)?,
                list_semantic: semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?,
                pair_semantic: semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 2)?,
                field: field_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 3)?,
            })
        }
        AGGREGATE_APPEND_VALUE if encoded.len() == APPEND_VALUE_BYTES => {
            Ok(ManagedOperation::AggregateAppendValue {
                aggregate_semantic: semantic_at(encoded, HEADER_BYTES)?,
                list_semantic: semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?,
                field: field_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 2)?,
            })
        }
        STRING_EQUAL if encoded.len() == HEADER_BYTES => Ok(ManagedOperation::StringEqual),
        STRING_APPEND if encoded.len() == HEADER_BYTES => Ok(ManagedOperation::StringAppend),
        STRING_LIST_JOIN if encoded.len() == HEADER_BYTES => Ok(ManagedOperation::StringListJoin),
        STRING_ESCAPE_HTML_TEXT if encoded.len() == HEADER_BYTES => {
            Ok(ManagedOperation::StringEscapeHtmlText)
        }
        STRING_ESCAPE_HTML_ATTRIBUTE if encoded.len() == HEADER_BYTES => {
            Ok(ManagedOperation::StringEscapeHtmlAttribute)
        }
        _ => Err(ManagedMemoryError::InvalidAggregateAbi),
    }
}

/// Compares two actor-owned managed strings after validating both references.
fn strings_equal(heap: &ActorHeap, left: i64, right: i64) -> Result<bool, ManagedMemoryError> {
    let left = reference_word(left)?.cast::<ManagedString>();
    let right = reference_word(right)?.cast::<ManagedString>();
    Ok(heap.read_string(left)? == heap.read_string(right)?)
}

/// Allocates the concatenation of two actor-owned managed strings.
fn append_strings(
    heap: &mut ActorHeap,
    left: i64,
    right: i64,
) -> Result<TvmRef<ManagedString>, ManagedMemoryError> {
    let left = heap
        .read_string(reference_word(left)?.cast::<ManagedString>())?
        .to_string();
    let right = heap
        .read_string(reference_word(right)?.cast::<ManagedString>())?
        .to_string();
    heap.allocate_string(&format!("{left}{right}"))
}

/// Allocates the ordered concatenation of one actor-owned managed string list.
fn join_string_list(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    list: i64,
) -> Result<TvmRef<ManagedString>, ManagedMemoryError> {
    let list = reference_word(list)?.cast::<ManagedList>();
    let semantic = heap.descriptor(list)?.semantic_id();
    let descriptor = layouts
        .collection(semantic)
        .and_then(|collection| collection.list_descriptor())
        .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
    let expected =
        super::ManagedFieldType::Reference(SemanticTypeId::from_canonical("std.core.String")?);
    if descriptor.element_type() != expected {
        return Err(ManagedMemoryError::ManagedTypeMismatch);
    }
    let elements = heap.list_elements(descriptor, list)?;
    let mut fragments = Vec::with_capacity(elements.len());
    let mut capacity = 0usize;
    for element in elements {
        let ManagedFieldValue::Reference(reference) = element else {
            return Err(ManagedMemoryError::InvalidAggregateField);
        };
        let fragment = heap.read_string(reference.cast::<ManagedString>())?;
        capacity = capacity
            .checked_add(fragment.len())
            .ok_or(ManagedMemoryError::AllocationLimitExceeded)?;
        fragments.push(fragment.to_string());
    }
    let mut joined = String::with_capacity(capacity);
    for fragment in fragments {
        joined.push_str(&fragment);
    }
    heap.allocate_string(&joined)
}

/// Applies one maintained string transform after validating the managed input.
fn transform_string(
    heap: &mut ActorHeap,
    value: i64,
    transform: fn(&str) -> String,
) -> Result<TvmRef<ManagedString>, ManagedMemoryError> {
    let value = heap
        .read_string(reference_word(value)?.cast::<ManagedString>())?
        .to_string();
    heap.allocate_string(&transform(&value))
}

/// Reads one checked physical field index from encoded operation bytes.
fn field_at(encoded: &[u8], offset: usize) -> Result<usize, ManagedMemoryError> {
    encoded
        .get(offset..offset + 4)
        .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
        .map(u32::from_le_bytes)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or(ManagedMemoryError::InvalidAggregateAbi)
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
        .read_aggregate(reference.cast::<ManagedAggregate>(), &descriptor)?
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
    heap.allocate_aggregate(option, &fields)
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
    let mut values = aggregate_fields(heap, &descriptor, reference)?;
    let field_type = descriptor
        .fields()
        .get(field)
        .ok_or(ManagedMemoryError::InvalidAggregateField)?
        .field_type();
    values[field] = field_value(replacement, field_type)?;
    heap.allocate_aggregate(descriptor, &values)
        .map(TvmRef::erase)
}

/// Persistently appends one pair into an aggregate-owned list and rebuilds the owner.
#[allow(clippy::too_many_arguments)]
fn append_pair_to_aggregate_list(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    aggregate_semantic: SemanticTypeId,
    list_semantic: SemanticTypeId,
    pair_semantic: SemanticTypeId,
    field: usize,
    aggregate: i64,
    first: i64,
    second: i64,
) -> Result<TvmRef<()>, ManagedMemoryError> {
    let aggregate = reference_word(aggregate)?;
    let aggregate_layout = layouts
        .layout_for_reference(heap, aggregate_semantic, aggregate)
        .map_err(|_| ManagedMemoryError::ManagedTypeMismatch)?;
    let mut aggregate_values = aggregate_fields(heap, &aggregate_layout, aggregate)?;
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
    let pair = heap.allocate_aggregate(pair_layout, &pair_values)?;
    let list = heap.list_append(
        list_descriptor,
        list,
        ManagedFieldValue::Reference(pair.erase()),
    )?;
    aggregate_values[field] = ManagedFieldValue::Reference(list.erase());
    heap.allocate_aggregate(aggregate_layout, &aggregate_values)
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
    let mut aggregate_values = aggregate_fields(heap, &aggregate_layout, aggregate)?;
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
    heap.allocate_aggregate(aggregate_layout, &aggregate_values)
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
) -> Result<Arc<ManagedAggregateDescriptor>, ManagedMemoryError> {
    let mut matching = layouts
        .layouts(semantic)
        .iter()
        .filter(|layout| layout.fields().len() == arity);
    let layout = matching
        .next()
        .cloned()
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
fn option_layout(
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    variant: &str,
    arity: usize,
) -> Result<Arc<ManagedAggregateDescriptor>, ManagedMemoryError> {
    layouts
        .layouts(semantic)
        .iter()
        .find(|layout| layout.variant_name() == Some(variant) && layout.fields().len() == arity)
        .cloned()
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

/// Converts one checked physical field into its native word representation.
pub(super) fn field_word(value: ManagedFieldValue) -> u64 {
    match value {
        ManagedFieldValue::Unit => 0,
        ManagedFieldValue::Bool(value) => u64::from(value),
        ManagedFieldValue::Int(value) => u64::from_ne_bytes(value.to_ne_bytes()),
        ManagedFieldValue::Float(value) => value.to_bits(),
        ManagedFieldValue::Atom(value) => u64::from(value.get()),
        ManagedFieldValue::Reference(value) => value.encoded_abi_word(),
    }
}

#[cfg(test)]
#[path = "operation_abi_test.rs"]
mod operation_abi_test;
