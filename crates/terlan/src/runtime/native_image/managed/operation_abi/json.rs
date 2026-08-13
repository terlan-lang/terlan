//! Bounded JSON decoding over actor-owned managed values.

use crate::terlan_native::json as native_json;

use super::super::{
    ActorHeap, ManagedFieldValue, ManagedLayoutRegistry, ManagedMemoryError, ManagedString,
    SemanticTypeId,
};

const MAGIC: &[u8; 4] = b"TVMJ";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 8;
const SEMANTIC_BYTES: usize = 16;
const PARSE_RESULT: u8 = 1;
const RESULT_IS_OK: u8 = 2;
const PARSE_RESULT_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES * 3;
const RESULT_IS_OK_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES;

/// Encodes parsing of one managed string into `Result[Json, Error]`.
pub fn encode_json_parse_result_operation(
    json_semantic: SemanticTypeId,
    result_semantic: SemanticTypeId,
    error_semantic: SemanticTypeId,
) -> Vec<u8> {
    let mut encoded = header(PARSE_RESULT);
    for semantic in [json_semantic, result_semantic, error_semantic] {
        encoded.extend_from_slice(&semantic.bytes());
    }
    encoded
}

/// Encodes a checked `Ok`-variant predicate for one managed result value.
pub fn encode_result_is_ok_operation(result_semantic: SemanticTypeId) -> Vec<u8> {
    let mut encoded = header(RESULT_IS_OK);
    encoded.extend_from_slice(&result_semantic.bytes());
    encoded
}

/// Reports whether bytes identify the managed JSON operation family.
pub(super) fn is_json_operation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
}

/// Reports whether one managed JSON operation returns a managed reference.
pub(super) fn json_operation_result_is_reference(encoded: &[u8]) -> bool {
    encoded.get(6).copied() != Some(RESULT_IS_OK)
}

/// Executes one exact JSON operation against the current actor heap.
pub(super) fn execute_json_operation(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    validate_header(encoded)?;
    match (encoded[6], encoded.len(), words) {
        (PARSE_RESULT, PARSE_RESULT_BYTES, [text]) if encoded[7] == 0 => {
            parse_result(heap, layouts, encoded, *text)
        }
        (RESULT_IS_OK, RESULT_IS_OK_BYTES, [result]) if encoded[7] == 0 => {
            result_is_ok(heap, layouts, encoded, *result)
        }
        _ => Err(ManagedMemoryError::InvalidManagedOperation),
    }
}

/// Parses actor-owned UTF-8 and allocates the corresponding managed result.
fn parse_result(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    text: i64,
) -> Result<u64, ManagedMemoryError> {
    let json_semantic = semantic_at(encoded, HEADER_BYTES)?;
    let result_semantic = semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES)?;
    let error_semantic = semantic_at(encoded, HEADER_BYTES + SEMANTIC_BYTES * 2)?;
    let text = heap
        .read_string(super::reference_word(text)?.cast::<ManagedString>())?
        .to_owned();
    let (variant, payload) = match native_json::parse(&text) {
        Ok(json) => {
            let canonical = native_json::stringify(&json)
                .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?;
            let canonical = heap.allocate_string(&canonical)?;
            let layout = super::unique_layout(layouts, json_semantic, 1)?;
            let json = heap.allocate_aggregate_ref(
                layout,
                &[ManagedFieldValue::Reference(canonical.erase())],
            )?;
            ("Ok", ManagedFieldValue::Reference(json.erase()))
        }
        Err(error) => {
            let message = heap.allocate_string(error.message())?;
            let code = layouts.atom_index(error.code())?;
            let layout = super::unique_layout(layouts, error_semantic, 2)?;
            let error = heap.allocate_aggregate_ref(
                layout,
                &[
                    ManagedFieldValue::Atom(code),
                    ManagedFieldValue::Reference(message.erase()),
                ],
            )?;
            ("Err", ManagedFieldValue::Reference(error.erase()))
        }
    };
    let layout = super::option_layout(layouts, result_semantic, variant, 1)?;
    heap.allocate_aggregate_ref(layout, &[payload])
        .map(|result| result.erase().encoded_abi_word())
}

/// Checks the active result constructor without opening its payload.
fn result_is_ok(
    heap: &ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    result: i64,
) -> Result<u64, ManagedMemoryError> {
    let semantic = semantic_at(encoded, HEADER_BYTES)?;
    let reference = super::reference_word(result)?;
    let layout = layouts
        .layout_for_reference(heap, semantic, reference)
        .map_err(|_| ManagedMemoryError::ManagedTypeMismatch)?;
    match layout.variant_name() {
        Some("Ok") => Ok(1),
        Some("Err") => Ok(0),
        _ => Err(ManagedMemoryError::ManagedTypeMismatch),
    }
}

/// Builds one fixed operation header.
fn header(operation: u8) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(HEADER_BYTES);
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&VERSION.to_le_bytes());
    encoded.push(operation);
    encoded.push(0);
    encoded
}

/// Validates the common JSON operation header.
fn validate_header(encoded: &[u8]) -> Result<(), ManagedMemoryError> {
    if encoded.len() < HEADER_BYTES
        || encoded.get(..4) != Some(MAGIC)
        || encoded.get(4..6) != Some(&VERSION.to_le_bytes())
    {
        return Err(ManagedMemoryError::InvalidManagedOperation);
    }
    Ok(())
}

/// Reads one semantic identity from an exact operation payload offset.
fn semantic_at(encoded: &[u8], offset: usize) -> Result<SemanticTypeId, ManagedMemoryError> {
    encoded
        .get(offset..offset + SEMANTIC_BYTES)
        .and_then(|bytes| <[u8; SEMANTIC_BYTES]>::try_from(bytes).ok())
        .map(SemanticTypeId::from_bytes)
        .ok_or(ManagedMemoryError::InvalidManagedOperation)
}

#[cfg(test)]
#[path = "json_test.rs"]
#[cfg(test)]
mod json_test;
