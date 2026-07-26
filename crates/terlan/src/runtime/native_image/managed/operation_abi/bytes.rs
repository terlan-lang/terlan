//! Generated-code operations for actor-owned immutable byte sequences.

use super::super::{
    ActorHeap, ManagedBytes, ManagedFieldValue, ManagedLayoutRegistry, ManagedList,
    ManagedMemoryError, SemanticTypeId, TvmRef,
};
use super::reference_word;

const MAGIC: &[u8; 4] = b"TVMB";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 8;
const SEMANTIC_BYTES: usize = 16;
const OPERATION_BYTES: usize = HEADER_BYTES + SEMANTIC_BYTES;
const FROM_LIST: u8 = 1;
const TO_LIST: u8 = 2;
const LENGTH: u8 = 3;
const CONCAT: u8 = 4;
const SLICE: u8 = 5;
const READ_UINT_BE: u8 = 6;
const READ_INT_BE: u8 = 7;
const READ_UINT_LE: u8 = 8;
const READ_INT_LE: u8 = 9;

pub(super) fn is_bytes_operation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
}

pub fn encode_bytes_from_list_operation(list_semantic: SemanticTypeId) -> Vec<u8> {
    operation(FROM_LIST, list_semantic, true)
}

pub fn encode_bytes_to_list_operation(list_semantic: SemanticTypeId) -> Vec<u8> {
    operation(TO_LIST, list_semantic, true)
}

pub fn encode_bytes_length_operation(list_semantic: SemanticTypeId) -> Vec<u8> {
    operation(LENGTH, list_semantic, false)
}

pub fn encode_bytes_concat_operation(list_semantic: SemanticTypeId) -> Vec<u8> {
    operation(CONCAT, list_semantic, true)
}

pub fn encode_bytes_slice_operation(list_semantic: SemanticTypeId) -> Vec<u8> {
    operation(SLICE, list_semantic, true)
}

pub fn encode_bytes_read_uint_be_operation(list_semantic: SemanticTypeId) -> Vec<u8> {
    operation(READ_UINT_BE, list_semantic, false)
}

pub fn encode_bytes_read_int_be_operation(list_semantic: SemanticTypeId) -> Vec<u8> {
    operation(READ_INT_BE, list_semantic, false)
}

pub fn encode_bytes_read_uint_le_operation(list_semantic: SemanticTypeId) -> Vec<u8> {
    operation(READ_UINT_LE, list_semantic, false)
}

pub fn encode_bytes_read_int_le_operation(list_semantic: SemanticTypeId) -> Vec<u8> {
    operation(READ_INT_LE, list_semantic, false)
}

pub(super) fn bytes_operation_result_is_reference(encoded: &[u8]) -> bool {
    encoded.get(7) == Some(&1)
}

pub(super) fn execute_bytes_operation(
    heap: &mut ActorHeap,
    layouts: &ManagedLayoutRegistry,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    let (operation, list_semantic) = decode(encoded)?;
    match operation {
        FROM_LIST => {
            let [list] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = layouts
                .collection(list_semantic)
                .and_then(|collection| collection.list_descriptor())
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let values =
                heap.list_elements(descriptor, reference_word(*list)?.cast::<ManagedList>())?;
            let bytes = values
                .into_iter()
                .map(|value| match value {
                    ManagedFieldValue::Int(value) => {
                        u8::try_from(value).map_err(|_| ManagedMemoryError::InvalidAggregateField)
                    }
                    _ => Err(ManagedMemoryError::ManagedTypeMismatch),
                })
                .collect::<Result<Vec<_>, _>>()?;
            heap.allocate_bytes(&bytes).map(TvmRef::encoded_abi_word)
        }
        TO_LIST => {
            let [bytes] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let descriptor = layouts
                .collection(list_semantic)
                .and_then(|collection| collection.list_descriptor())
                .ok_or(ManagedMemoryError::ManagedTypeMismatch)?;
            let elements = heap
                .read_bytes(reference_word(*bytes)?.cast::<ManagedBytes>())?
                .iter()
                .map(|value| ManagedFieldValue::Int(i64::from(*value)))
                .collect::<Vec<_>>();
            heap.list_from_elements(descriptor, &elements)
                .map(TvmRef::encoded_abi_word)
        }
        LENGTH => {
            let [bytes] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let length = heap
                .read_bytes(reference_word(*bytes)?.cast::<ManagedBytes>())?
                .len();
            i64::try_from(length)
                .map(|length| u64::from_ne_bytes(length.to_ne_bytes()))
                .map_err(|_| ManagedMemoryError::InvalidSequenceLength)
        }
        CONCAT => {
            let [left, right] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let left = heap
                .read_bytes(reference_word(*left)?.cast::<ManagedBytes>())?
                .to_vec();
            let right = heap.read_bytes(reference_word(*right)?.cast::<ManagedBytes>())?;
            let mut joined = Vec::with_capacity(
                left.len()
                    .checked_add(right.len())
                    .ok_or(ManagedMemoryError::InvalidSequenceLength)?,
            );
            joined.extend_from_slice(&left);
            joined.extend_from_slice(right);
            heap.allocate_bytes(&joined).map(TvmRef::encoded_abi_word)
        }
        SLICE => {
            let [bytes, start, length] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let start = nonnegative_usize(*start)?;
            let length = nonnegative_usize(*length)?;
            let end = start
                .checked_add(length)
                .ok_or(ManagedMemoryError::InvalidSequenceLength)?;
            let selected = heap
                .read_bytes(reference_word(*bytes)?.cast::<ManagedBytes>())?
                .get(start..end)
                .ok_or(ManagedMemoryError::InvalidBitRange)?
                .to_vec();
            heap.allocate_bytes(&selected).map(TvmRef::encoded_abi_word)
        }
        READ_UINT_BE | READ_INT_BE | READ_UINT_LE | READ_INT_LE => {
            let [bytes, offset, width] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let offset = nonnegative_usize(*offset)?;
            let width = nonnegative_usize(*width)?;
            if !(1..=63).contains(&width) {
                return Err(ManagedMemoryError::InvalidManagedScalar);
            }
            let bytes = heap.read_bytes(reference_word(*bytes)?.cast::<ManagedBytes>())?;
            let end = offset
                .checked_add(width)
                .ok_or(ManagedMemoryError::InvalidBitRange)?;
            if end > bytes.len().saturating_mul(8) {
                return Err(ManagedMemoryError::InvalidBitRange);
            }
            let little = matches!(operation, READ_UINT_LE | READ_INT_LE);
            let signed = matches!(operation, READ_INT_BE | READ_INT_LE);
            let raw = read_integer_bits(bytes, offset, width, little);
            let value = if signed && raw & (1_u64 << (width - 1)) != 0 {
                (raw | (!0_u64 << width)) as i64
            } else {
                raw as i64
            };
            Ok(u64::from_ne_bytes(value.to_ne_bytes()))
        }
        _ => Err(ManagedMemoryError::InvalidAggregateAbi),
    }
}

fn nonnegative_usize(value: i64) -> Result<usize, ManagedMemoryError> {
    usize::try_from(value).map_err(|_| ManagedMemoryError::InvalidSequenceLength)
}

fn read_integer_bits(bytes: &[u8], offset: usize, width: usize, little: bool) -> u64 {
    if !little {
        return (0..width).fold(0_u64, |value, index| {
            (value << 1) | u64::from(read_bit(bytes, offset + index))
        });
    }
    let mut value = 0_u64;
    let mut source = 0usize;
    let mut group_start = 0usize;
    while group_start < width {
        let group_width = (width - group_start).min(8);
        for group_bit in (0..group_width).rev() {
            if read_bit(bytes, offset + source) {
                value |= 1_u64 << (group_start + group_bit);
            }
            source += 1;
        }
        group_start += group_width;
    }
    value
}

fn read_bit(bytes: &[u8], index: usize) -> bool {
    bytes[index / 8] & (1_u8 << (7 - index % 8)) != 0
}

fn operation(operation: u8, list_semantic: SemanticTypeId, reference: bool) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(OPERATION_BYTES);
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&VERSION.to_le_bytes());
    encoded.push(operation);
    encoded.push(u8::from(reference));
    encoded.extend_from_slice(&list_semantic.bytes());
    encoded
}

fn decode(encoded: &[u8]) -> Result<(u8, SemanticTypeId), ManagedMemoryError> {
    if encoded.len() != OPERATION_BYTES
        || encoded.get(..4) != Some(MAGIC)
        || encoded.get(4..6) != Some(&VERSION.to_le_bytes())
        || !matches!(encoded[7], 0 | 1)
    {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    let semantic = encoded
        .get(HEADER_BYTES..OPERATION_BYTES)
        .and_then(|bytes| <[u8; SEMANTIC_BYTES]>::try_from(bytes).ok())
        .map(SemanticTypeId::from_bytes)
        .ok_or(ManagedMemoryError::InvalidAggregateAbi)?;
    Ok((encoded[6], semantic))
}
