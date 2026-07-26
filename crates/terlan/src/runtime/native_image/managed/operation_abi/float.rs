//! Bounded Float operations over actor-owned managed values.

use super::super::{
    ActorHeap, ManagedFieldValue, ManagedLayoutRegistry, ManagedMemoryError, ManagedString,
    SemanticTypeId,
};

const MAGIC: &[u8; 4] = b"TVMF";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 8;
const SEMANTIC_BYTES: usize = 16;
const TO_STRING: u8 = 1;
const FROM_STRING: u8 = 2;
const LOG: u8 = 3;
const FROM_STRING_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES;

/// Encodes canonical finite-binary64 formatting into a managed UTF-8 string.
pub fn encode_float_to_string_operation() -> Vec<u8> {
    header(TO_STRING)
}

/// Encodes finite-binary64 parsing into the admitted `Option[Float]` layout.
pub fn encode_float_from_string_operation(option_semantic: SemanticTypeId) -> Vec<u8> {
    let mut encoded = header(FROM_STRING);
    encoded.extend_from_slice(&option_semantic.bytes());
    encoded
}

/// Encodes natural logarithm over one finite-binary64 scalar word.
pub fn encode_float_log_operation() -> Vec<u8> {
    header(LOG)
}

pub(super) fn is_float_operation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
}

pub(super) fn float_operation_result_is_reference(encoded: &[u8]) -> bool {
    encoded.get(6).copied() != Some(LOG)
}

pub(super) fn execute_float_operation(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    validate_header(encoded)?;
    match (encoded[6], encoded.len(), words) {
        (TO_STRING, HEADER_BYTES, [word]) if encoded[7] == 0 => {
            let value = finite_float(*word)?;
            heap.allocate_string(&value.to_string())
                .map(|value| value.erase().encoded_abi_word())
        }
        (FROM_STRING, FROM_STRING_BYTES, [text]) if encoded[7] == 0 => {
            parse_float(heap, layouts, encoded, *text)
        }
        (LOG, HEADER_BYTES, [word]) if encoded[7] == 0 => {
            let result = finite_float(*word)?.ln();
            result
                .is_finite()
                .then_some(result.to_bits())
                .ok_or(ManagedMemoryError::InvalidManagedScalar)
        }
        _ => Err(ManagedMemoryError::InvalidManagedOperation),
    }
}

fn parse_float(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    text: i64,
) -> Result<u64, ManagedMemoryError> {
    let semantic = semantic_at(encoded, HEADER_BYTES)?;
    let text = heap
        .read_string(super::reference_word(text)?.cast::<ManagedString>())?
        .to_owned();
    let parsed = text.parse::<f64>().ok().filter(|value| value.is_finite());
    let (variant, fields) = match parsed {
        Some(value) => ("Some", vec![ManagedFieldValue::Float(value)]),
        None => ("None", Vec::new()),
    };
    let layout = super::option_layout(layouts, semantic, variant, fields.len())?;
    heap.allocate_aggregate_ref(layout, &fields)
        .map(|value| value.erase().encoded_abi_word())
}

fn finite_float(word: i64) -> Result<f64, ManagedMemoryError> {
    let value = f64::from_bits(u64::from_ne_bytes(word.to_ne_bytes()));
    value
        .is_finite()
        .then_some(value)
        .ok_or(ManagedMemoryError::InvalidManagedScalar)
}

fn header(operation: u8) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(FROM_STRING_BYTES);
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&VERSION.to_le_bytes());
    encoded.push(operation);
    encoded.push(0);
    encoded
}

fn validate_header(encoded: &[u8]) -> Result<(), ManagedMemoryError> {
    if encoded.len() < HEADER_BYTES
        || encoded.get(..4) != Some(MAGIC)
        || encoded.get(4..6) != Some(&VERSION.to_le_bytes())
    {
        return Err(ManagedMemoryError::InvalidManagedOperation);
    }
    Ok(())
}

fn semantic_at(encoded: &[u8], offset: usize) -> Result<SemanticTypeId, ManagedMemoryError> {
    encoded
        .get(offset..offset + SEMANTIC_BYTES)
        .and_then(|bytes| <[u8; SEMANTIC_BYTES]>::try_from(bytes).ok())
        .map(SemanticTypeId::from_bytes)
        .ok_or(ManagedMemoryError::InvalidManagedOperation)
}

#[cfg(test)]
#[path = "float_test.rs"]
mod test;
