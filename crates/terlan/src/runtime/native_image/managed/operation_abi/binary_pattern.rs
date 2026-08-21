//! Bounded binary-layout matching for generated native code.

use std::num::NonZeroUsize;

use super::super::{ActorHeap, ManagedBinary, ManagedBinaryView, ManagedMemoryError, TvmRef};

const MAGIC: &[u8; 4] = b"TVPB";
const VERSION: u16 = 1;
const MATCHES: u8 = 1;
const EXTRACT: u8 = 2;
const HEADER_BYTES: usize = 14;
const FIELD_BYTES: usize = 9;
const MAX_FIELDS: usize = 256;

/// Endianness carried by one binary layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedBinaryPatternEndian {
    Big,
    Little,
}

/// One bounded field in a binary layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedBinaryPatternField {
    UInt(u64),
    Int(u64),
    Bytes(u64),
    Bits(u64),
    Utf8,
    Utf16,
    Utf32,
    Rest,
}

pub(super) fn is_binary_pattern_operation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
}

/// Encodes a whole-binary match against the bounded field layout.
pub fn encode_binary_pattern_matches_operation(
    endian: ManagedBinaryPatternEndian,
    fields: &[ManagedBinaryPatternField],
) -> Result<Vec<u8>, ManagedMemoryError> {
    encode(MATCHES, false, endian, fields, 0)
}

/// Encodes extraction of one selected field after whole-layout validation.
pub fn encode_binary_pattern_extract_operation(
    endian: ManagedBinaryPatternEndian,
    fields: &[ManagedBinaryPatternField],
    selected: usize,
) -> Result<Vec<u8>, ManagedMemoryError> {
    let field = fields
        .get(selected)
        .ok_or(ManagedMemoryError::InvalidManagedOperation)?;
    encode(
        EXTRACT,
        field_is_reference(*field),
        endian,
        fields,
        selected,
    )
}

#[cfg(any(test, not(feature = "serve-runtime-bin"), feature = "native-codegen"))]
pub(super) fn binary_pattern_result_is_reference(encoded: &[u8]) -> bool {
    encoded.get(7) == Some(&1)
}

pub(super) fn execute_binary_pattern_operation(
    heap: &mut ActorHeap,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    let [word] = words else {
        return Err(ManagedMemoryError::InvalidAggregateArity);
    };
    let operation = decode(encoded)?;
    let reference = reference(*word)?;
    let view = heap.read_binary(reference)?;
    let Some(ranges) = matched_ranges(view, operation.endian, &operation.fields) else {
        return if operation.tag == MATCHES {
            Ok(0)
        } else {
            Err(ManagedMemoryError::InvalidManagedOperation)
        };
    };
    if operation.tag == MATCHES {
        return Ok(1);
    }
    let selected = operation
        .fields
        .get(operation.selected)
        .copied()
        .ok_or(ManagedMemoryError::InvalidManagedOperation)?;
    let (start, width) = ranges[operation.selected];
    let extracted = extract(view, operation.endian, selected, start, width)?;
    match extracted {
        Extracted::Scalar(value) => Ok(value as u64),
        Extracted::Bytes(bytes) => heap.allocate_bytes(&bytes).map(TvmRef::encoded_abi_word),
        Extracted::Binary(bytes, bit_length) => {
            let storage = heap.allocate_bytes(&bytes)?;
            heap.allocate_binary(storage, 0, bit_length)
                .map(TvmRef::encoded_abi_word)
        }
    }
}

struct Operation {
    tag: u8,
    endian: ManagedBinaryPatternEndian,
    selected: usize,
    fields: Vec<ManagedBinaryPatternField>,
}

fn encode(
    tag: u8,
    reference: bool,
    endian: ManagedBinaryPatternEndian,
    fields: &[ManagedBinaryPatternField],
    selected: usize,
) -> Result<Vec<u8>, ManagedMemoryError> {
    validate_fields(fields)?;
    let count =
        u16::try_from(fields.len()).map_err(|_| ManagedMemoryError::InvalidManagedOperation)?;
    let selected =
        u16::try_from(selected).map_err(|_| ManagedMemoryError::InvalidManagedOperation)?;
    let mut encoded = Vec::with_capacity(HEADER_BYTES + fields.len() * FIELD_BYTES);
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&VERSION.to_le_bytes());
    encoded.push(tag);
    encoded.push(u8::from(reference));
    encoded.push(match endian {
        ManagedBinaryPatternEndian::Big => 0,
        ManagedBinaryPatternEndian::Little => 1,
    });
    encoded.push(0);
    encoded.extend_from_slice(&count.to_le_bytes());
    encoded.extend_from_slice(&selected.to_le_bytes());
    for field in fields {
        let (tag, width) = field_encoding(*field);
        encoded.push(tag);
        encoded.extend_from_slice(&width.to_le_bytes());
    }
    Ok(encoded)
}

fn decode(encoded: &[u8]) -> Result<Operation, ManagedMemoryError> {
    if encoded.len() < HEADER_BYTES
        || encoded.get(..4) != Some(MAGIC)
        || encoded.get(4..6) != Some(&VERSION.to_le_bytes())
        || !matches!(encoded[6], MATCHES | EXTRACT)
        || encoded[7] > 1
        || encoded[9] != 0
    {
        return Err(ManagedMemoryError::InvalidManagedOperation);
    }
    let endian = match encoded[8] {
        0 => ManagedBinaryPatternEndian::Big,
        1 => ManagedBinaryPatternEndian::Little,
        _ => return Err(ManagedMemoryError::InvalidManagedOperation),
    };
    let count = usize::from(u16::from_le_bytes([encoded[10], encoded[11]]));
    let selected = usize::from(u16::from_le_bytes([encoded[12], encoded[13]]));
    if count > MAX_FIELDS || encoded.len() != HEADER_BYTES + count * FIELD_BYTES {
        return Err(ManagedMemoryError::InvalidManagedOperation);
    }
    let mut fields = Vec::with_capacity(count);
    for payload in encoded[HEADER_BYTES..].chunks_exact(FIELD_BYTES) {
        let width = u64::from_le_bytes(
            payload[1..]
                .try_into()
                .map_err(|_| ManagedMemoryError::InvalidManagedOperation)?,
        );
        fields.push(field_decoding(payload[0], width)?);
    }
    validate_fields(&fields)?;
    if encoded[6] == MATCHES && (selected != 0 || encoded[7] != 0)
        || encoded[6] == EXTRACT
            && (selected >= fields.len()
                || encoded[7] != u8::from(field_is_reference(fields[selected])))
    {
        return Err(ManagedMemoryError::InvalidManagedOperation);
    }
    Ok(Operation {
        tag: encoded[6],
        endian,
        selected,
        fields,
    })
}

fn validate_fields(fields: &[ManagedBinaryPatternField]) -> Result<(), ManagedMemoryError> {
    if fields.len() > MAX_FIELDS {
        return Err(ManagedMemoryError::InvalidManagedOperation);
    }
    for (index, field) in fields.iter().enumerate() {
        match field {
            ManagedBinaryPatternField::UInt(width) | ManagedBinaryPatternField::Int(width)
                if !(1..=63).contains(width) =>
            {
                return Err(ManagedMemoryError::InvalidManagedOperation)
            }
            ManagedBinaryPatternField::Bytes(width) | ManagedBinaryPatternField::Bits(width)
                if *width == 0 =>
            {
                return Err(ManagedMemoryError::InvalidManagedOperation)
            }
            ManagedBinaryPatternField::Rest if index + 1 != fields.len() => {
                return Err(ManagedMemoryError::InvalidManagedOperation)
            }
            _ => {}
        }
    }
    Ok(())
}

fn matched_ranges(
    view: ManagedBinaryView<'_>,
    endian: ManagedBinaryPatternEndian,
    fields: &[ManagedBinaryPatternField],
) -> Option<Vec<(usize, usize)>> {
    let mut offset = 0usize;
    let mut ranges = Vec::with_capacity(fields.len());
    for field in fields {
        let width = match field {
            ManagedBinaryPatternField::UInt(width)
            | ManagedBinaryPatternField::Int(width)
            | ManagedBinaryPatternField::Bits(width) => usize::try_from(*width).ok()?,
            ManagedBinaryPatternField::Bytes(width) => {
                usize::try_from(*width).ok()?.checked_mul(8)?
            }
            ManagedBinaryPatternField::Utf8 => utf8_width(view, offset)?,
            ManagedBinaryPatternField::Utf16 => utf16_width(view, offset, endian)?,
            ManagedBinaryPatternField::Utf32 => 32,
            ManagedBinaryPatternField::Rest => {
                let width = view.bit_length().checked_sub(offset)?;
                if !width.is_multiple_of(8) {
                    return None;
                }
                width
            }
        };
        let end = offset.checked_add(width)?;
        if end > view.bit_length() {
            return None;
        }
        if matches!(field, ManagedBinaryPatternField::Utf8)
            && decode_utf(view, offset, width, endian, *field).is_none()
            || matches!(
                field,
                ManagedBinaryPatternField::Utf16 | ManagedBinaryPatternField::Utf32
            ) && decode_utf(view, offset, width, endian, *field).is_none()
        {
            return None;
        }
        ranges.push((offset, width));
        offset = end;
    }
    (offset == view.bit_length()).then_some(ranges)
}

enum Extracted {
    Scalar(i64),
    Bytes(Vec<u8>),
    Binary(Vec<u8>, usize),
}

fn extract(
    view: ManagedBinaryView<'_>,
    endian: ManagedBinaryPatternEndian,
    field: ManagedBinaryPatternField,
    start: usize,
    width: usize,
) -> Result<Extracted, ManagedMemoryError> {
    match field {
        ManagedBinaryPatternField::UInt(_) => {
            decode_integer(view, start, width, false, endian).map(Extracted::Scalar)
        }
        ManagedBinaryPatternField::Int(_) => {
            decode_integer(view, start, width, true, endian).map(Extracted::Scalar)
        }
        ManagedBinaryPatternField::Utf8
        | ManagedBinaryPatternField::Utf16
        | ManagedBinaryPatternField::Utf32 => decode_utf(view, start, width, endian, field)
            .map(Extracted::Scalar)
            .ok_or(ManagedMemoryError::InvalidManagedOperation),
        ManagedBinaryPatternField::Bytes(_) | ManagedBinaryPatternField::Rest => {
            let bytes = copy_bits(view, start, width)?;
            Ok(Extracted::Bytes(bytes))
        }
        ManagedBinaryPatternField::Bits(_) => {
            let bytes = copy_bits(view, start, width)?;
            Ok(Extracted::Binary(bytes, width))
        }
    }
}

fn decode_integer(
    view: ManagedBinaryView<'_>,
    start: usize,
    width: usize,
    signed: bool,
    endian: ManagedBinaryPatternEndian,
) -> Result<i64, ManagedMemoryError> {
    let mut raw = 0u64;
    match endian {
        ManagedBinaryPatternEndian::Big => {
            for index in 0..width {
                raw = (raw << 1) | u64::from(bit(view, start + index)?);
            }
        }
        ManagedBinaryPatternEndian::Little => {
            let mut source = 0usize;
            while source < width {
                let group = (width - source).min(8);
                for index in 0..group {
                    raw |= u64::from(bit(view, start + source + index)?)
                        << (source + group - index - 1);
                }
                source += group;
            }
        }
    }
    if signed && raw & (1 << (width - 1)) != 0 {
        Ok((raw | (!0u64 << width)) as i64)
    } else {
        Ok(raw as i64)
    }
}

fn utf8_width(view: ManagedBinaryView<'_>, start: usize) -> Option<usize> {
    let first = decode_integer(view, start, 8, false, ManagedBinaryPatternEndian::Big).ok()? as u8;
    Some(match first {
        0x00..=0x7f => 8,
        0xc2..=0xdf => 16,
        0xe0..=0xef => 24,
        0xf0..=0xf4 => 32,
        _ => return None,
    })
}

fn utf16_width(
    view: ManagedBinaryView<'_>,
    start: usize,
    endian: ManagedBinaryPatternEndian,
) -> Option<usize> {
    let first = decode_integer(view, start, 16, false, endian).ok()? as u16;
    Some(if (0xd800..=0xdbff).contains(&first) {
        32
    } else {
        16
    })
}

fn decode_utf(
    view: ManagedBinaryView<'_>,
    start: usize,
    width: usize,
    endian: ManagedBinaryPatternEndian,
    field: ManagedBinaryPatternField,
) -> Option<i64> {
    let bytes = copy_bits(view, start, width).ok()?;
    let scalar = match field {
        ManagedBinaryPatternField::Utf8 => {
            let text = std::str::from_utf8(&bytes).ok()?;
            let mut chars = text.chars();
            let scalar = chars.next()?;
            if chars.next().is_some() {
                return None;
            }
            scalar
        }
        ManagedBinaryPatternField::Utf16 => {
            let units = bytes.chunks_exact(2).map(|pair| match endian {
                ManagedBinaryPatternEndian::Big => u16::from_be_bytes([pair[0], pair[1]]),
                ManagedBinaryPatternEndian::Little => u16::from_le_bytes([pair[0], pair[1]]),
            });
            let mut decoded = char::decode_utf16(units);
            let scalar = decoded.next()?.ok()?;
            if decoded.next().is_some() {
                return None;
            }
            scalar
        }
        ManagedBinaryPatternField::Utf32 => {
            let bytes: [u8; 4] = bytes.try_into().ok()?;
            char::from_u32(match endian {
                ManagedBinaryPatternEndian::Big => u32::from_be_bytes(bytes),
                ManagedBinaryPatternEndian::Little => u32::from_le_bytes(bytes),
            })?
        }
        _ => return None,
    };
    Some(i64::from(u32::from(scalar)))
}

fn copy_bits(
    view: ManagedBinaryView<'_>,
    start: usize,
    width: usize,
) -> Result<Vec<u8>, ManagedMemoryError> {
    let mut output = vec![0u8; width.div_ceil(8)];
    for index in 0..width {
        if bit(view, start + index)? {
            output[index / 8] |= 1 << (7 - index % 8);
        }
    }
    Ok(output)
}

fn bit(view: ManagedBinaryView<'_>, index: usize) -> Result<bool, ManagedMemoryError> {
    view.bit(index).ok_or(ManagedMemoryError::InvalidBitRange)
}

fn field_is_reference(field: ManagedBinaryPatternField) -> bool {
    matches!(
        field,
        ManagedBinaryPatternField::Bytes(_)
            | ManagedBinaryPatternField::Bits(_)
            | ManagedBinaryPatternField::Rest
    )
}

fn field_encoding(field: ManagedBinaryPatternField) -> (u8, u64) {
    match field {
        ManagedBinaryPatternField::UInt(width) => (1, width),
        ManagedBinaryPatternField::Int(width) => (2, width),
        ManagedBinaryPatternField::Bytes(width) => (3, width),
        ManagedBinaryPatternField::Bits(width) => (4, width),
        ManagedBinaryPatternField::Utf8 => (5, 0),
        ManagedBinaryPatternField::Utf16 => (6, 0),
        ManagedBinaryPatternField::Utf32 => (7, 0),
        ManagedBinaryPatternField::Rest => (8, 0),
    }
}

fn field_decoding(tag: u8, width: u64) -> Result<ManagedBinaryPatternField, ManagedMemoryError> {
    match (tag, width) {
        (1, width) => Ok(ManagedBinaryPatternField::UInt(width)),
        (2, width) => Ok(ManagedBinaryPatternField::Int(width)),
        (3, width) => Ok(ManagedBinaryPatternField::Bytes(width)),
        (4, width) => Ok(ManagedBinaryPatternField::Bits(width)),
        (5, 0) => Ok(ManagedBinaryPatternField::Utf8),
        (6, 0) => Ok(ManagedBinaryPatternField::Utf16),
        (7, 0) => Ok(ManagedBinaryPatternField::Utf32),
        (8, 0) => Ok(ManagedBinaryPatternField::Rest),
        _ => Err(ManagedMemoryError::InvalidManagedOperation),
    }
}

fn reference(word: i64) -> Result<TvmRef<ManagedBinary>, ManagedMemoryError> {
    usize::try_from(u64::from_ne_bytes(word.to_ne_bytes()))
        .ok()
        .and_then(NonZeroUsize::new)
        .map(TvmRef::from_encoded)
        .ok_or(ManagedMemoryError::InvalidAggregateField)
}
