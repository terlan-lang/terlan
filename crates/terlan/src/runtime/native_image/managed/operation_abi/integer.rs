//! Bounded integer text conversion for generated native code.

use super::super::{
    ActorHeap, ManagedFieldValue, ManagedLayoutRegistry, ManagedMemoryError, ManagedString,
    SemanticTypeId,
};

const MAGIC: &[u8; 4] = b"TVMI";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 8;
const SEMANTIC_BYTES: usize = 16;
const TO_STRING: u8 = 1;
const TO_STRING_BASE: u8 = 2;
const FROM_STRING: u8 = 3;
const FROM_STRING_BASE: u8 = 4;
const OPTION_OPERATION_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES;
const MAX_INTEGER_PARSE_BYTES: usize = 128;

/// Encodes decimal integer formatting.
pub fn encode_int_to_string_operation() -> Vec<u8> {
    header(TO_STRING)
}

/// Encodes checked integer formatting in a caller-supplied radix.
pub fn encode_int_to_string_base_operation(option_semantic: SemanticTypeId) -> Vec<u8> {
    option_operation(TO_STRING_BASE, option_semantic)
}

/// Encodes checked decimal integer parsing into the identified option layout.
pub fn encode_int_from_string_operation(option_semantic: SemanticTypeId) -> Vec<u8> {
    option_operation(FROM_STRING, option_semantic)
}

/// Encodes checked integer parsing in a caller-supplied radix.
pub fn encode_int_from_string_base_operation(option_semantic: SemanticTypeId) -> Vec<u8> {
    option_operation(FROM_STRING_BASE, option_semantic)
}

pub(super) fn is_integer_operation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
}

pub(super) fn execute_integer_operation(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    validate_header(encoded)?;
    match (encoded[6], encoded.len(), words) {
        (TO_STRING, HEADER_BYTES, [value]) if encoded[7] == 0 => heap
            .allocate_string(&value.to_string())
            .map(|value| value.erase().encoded_abi_word()),
        (TO_STRING_BASE, OPTION_OPERATION_BYTES, [value, base]) if encoded[7] == 0 => {
            let rendered = u32::try_from(*base)
                .ok()
                .filter(|base| (2..=36).contains(base))
                .map(|base| format_radix(*value, base));
            allocate_option_string(heap, layouts, semantic_at(encoded)?, rendered)
        }
        (FROM_STRING, OPTION_OPERATION_BYTES, [text]) if encoded[7] == 0 => {
            let parsed = parse_integer(heap, *text, 10)?;
            allocate_option_int(heap, layouts, semantic_at(encoded)?, parsed)
        }
        (FROM_STRING_BASE, OPTION_OPERATION_BYTES, [text, base]) if encoded[7] == 0 => {
            let parsed = u32::try_from(*base)
                .ok()
                .filter(|base| (2..=36).contains(base))
                .map(|base| parse_integer(heap, *text, base))
                .transpose()?
                .flatten();
            allocate_option_int(heap, layouts, semantic_at(encoded)?, parsed)
        }
        _ => Err(ManagedMemoryError::InvalidManagedOperation),
    }
}

fn parse_integer(
    heap: &ActorHeap,
    text: i64,
    base: u32,
) -> Result<Option<i64>, ManagedMemoryError> {
    let text = heap.read_string(super::reference_word(text)?.cast::<ManagedString>())?;
    if text.is_empty() || text.len() > MAX_INTEGER_PARSE_BYTES {
        return Ok(None);
    }
    Ok(i64::from_str_radix(text, base).ok())
}

fn format_radix(value: i64, base: u32) -> String {
    let negative = value < 0;
    let mut magnitude = value.unsigned_abs();
    let mut digits = [0_u8; 65];
    let mut cursor = digits.len();
    loop {
        cursor -= 1;
        let digit = (magnitude % u64::from(base)) as u8;
        digits[cursor] = if digit < 10 {
            b'0' + digit
        } else {
            b'A' + digit - 10
        };
        magnitude /= u64::from(base);
        if magnitude == 0 {
            break;
        }
    }
    if negative {
        cursor -= 1;
        digits[cursor] = b'-';
    }
    String::from_utf8(digits[cursor..].to_vec()).expect("radix digits are ASCII")
}

fn allocate_option_int(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    value: Option<i64>,
) -> Result<u64, ManagedMemoryError> {
    let (variant, fields) = match value {
        Some(value) => ("Some", vec![ManagedFieldValue::Int(value)]),
        None => ("None", Vec::new()),
    };
    allocate_option(heap, layouts, semantic, variant, fields)
}

fn allocate_option_string(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    value: Option<String>,
) -> Result<u64, ManagedMemoryError> {
    let value = value
        .map(|value| heap.allocate_string(&value))
        .transpose()?
        .map(|value| ManagedFieldValue::Reference(value.erase()));
    let (variant, fields) = match value {
        Some(value) => ("Some", vec![value]),
        None => ("None", Vec::new()),
    };
    allocate_option(heap, layouts, semantic, variant, fields)
}

fn allocate_option(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    semantic: SemanticTypeId,
    variant: &str,
    fields: Vec<ManagedFieldValue>,
) -> Result<u64, ManagedMemoryError> {
    let layout = super::option_layout(layouts, semantic, variant, fields.len())?;
    heap.allocate_aggregate_ref(layout, &fields)
        .map(|value| value.erase().encoded_abi_word())
}

fn option_operation(operation: u8, semantic: SemanticTypeId) -> Vec<u8> {
    let mut encoded = header(operation);
    encoded.extend_from_slice(&semantic.bytes());
    encoded
}

fn header(operation: u8) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(OPTION_OPERATION_BYTES);
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

fn semantic_at(encoded: &[u8]) -> Result<SemanticTypeId, ManagedMemoryError> {
    encoded
        .get(HEADER_BYTES..OPTION_OPERATION_BYTES)
        .and_then(|bytes| <[u8; SEMANTIC_BYTES]>::try_from(bytes).ok())
        .map(SemanticTypeId::from_bytes)
        .ok_or(ManagedMemoryError::InvalidManagedOperation)
}

#[cfg(test)]
#[path = "integer_test.rs"]
mod test;
