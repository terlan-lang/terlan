//! Generated-code operations for actor-owned immutable bitstrings.

use super::super::{
    ActorHeap, ManagedBinary, ManagedBinaryView, ManagedBytes, ManagedMemoryError, TvmRef,
};
use super::reference_word;

const MAGIC: &[u8; 4] = b"TVBS";
const VERSION: u16 = 1;
const OPERATION_BYTES: usize = 8;

/// Closed operation inventory shared by the compiler and managed runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedBitStringOperation {
    FromBytes,
    FromAllBytes,
    FromExactBytes,
    RequireExactBits,
    FromUintBe,
    FromIntBe,
    FromUintLe,
    FromIntLe,
    Utf8Scalar,
    ToUtf8Scalar,
    Utf16BeScalar,
    Utf16LeScalar,
    ToUtf16BeScalar,
    ToUtf16LeScalar,
    Utf32BeScalar,
    Utf32LeScalar,
    ToUtf32BeScalar,
    ToUtf32LeScalar,
    BitLength,
    ByteLength,
    IsByteAligned,
    Slice,
    Concat,
    ToBytes,
    ToUintBe,
    ToIntBe,
    ToUintLe,
    ToIntLe,
}

pub(super) fn is_bitstring_operation(encoded: &[u8]) -> bool {
    encoded.starts_with(MAGIC)
}

/// Encodes one bounded bitstring operation for generated native code.
pub fn encode_bitstring_operation(operation: ManagedBitStringOperation) -> Vec<u8> {
    let mut encoded = Vec::with_capacity(OPERATION_BYTES);
    encoded.extend_from_slice(MAGIC);
    encoded.extend_from_slice(&VERSION.to_le_bytes());
    encoded.push(operation.tag());
    encoded.push(u8::from(operation.result_is_reference()));
    encoded
}

pub(super) fn bitstring_result_is_reference(encoded: &[u8]) -> bool {
    encoded.get(7) == Some(&1)
}

pub(super) fn execute_bitstring_operation(
    heap: &mut ActorHeap,
    encoded: &[u8],
    words: &[i64],
) -> Result<u64, ManagedMemoryError> {
    let operation = decode(encoded)?;
    match operation {
        ManagedBitStringOperation::FromBytes => {
            let [bytes, bit_length] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let bytes = heap.read_bytes(reference_word(*bytes)?.cast::<ManagedBytes>())?;
            let bit_length = nonnegative_usize(*bit_length)?;
            let available = bytes
                .len()
                .checked_mul(8)
                .ok_or(ManagedMemoryError::InvalidSequenceLength)?;
            if bit_length > available {
                return Err(ManagedMemoryError::InvalidBitRange);
            }
            let mut packed = bytes[..bit_length.div_ceil(8)].to_vec();
            mask_trailing_bits(&mut packed, bit_length);
            allocate_packed(heap, &packed, bit_length)
        }
        ManagedBitStringOperation::FromAllBytes => {
            let [bytes] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let bytes = reference_word(*bytes)?.cast::<ManagedBytes>();
            let bit_length = heap
                .read_bytes(bytes)?
                .len()
                .checked_mul(8)
                .ok_or(ManagedMemoryError::InvalidSequenceLength)?;
            heap.allocate_binary(bytes, 0, bit_length)
                .map(TvmRef::encoded_abi_word)
        }
        ManagedBitStringOperation::FromExactBytes => {
            let [bytes, byte_length] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let bytes = reference_word(*bytes)?.cast::<ManagedBytes>();
            let byte_length = nonnegative_usize(*byte_length)?;
            if heap.read_bytes(bytes)?.len() != byte_length {
                return Err(ManagedMemoryError::InvalidSequenceLength);
            }
            let bit_length = byte_length
                .checked_mul(8)
                .ok_or(ManagedMemoryError::InvalidSequenceLength)?;
            heap.allocate_binary(bytes, 0, bit_length)
                .map(TvmRef::encoded_abi_word)
        }
        ManagedBitStringOperation::RequireExactBits => {
            let [binary, bit_length] = words else {
                return Err(ManagedMemoryError::InvalidAggregateArity);
            };
            let reference = reference_word(*binary)?.cast::<ManagedBinary>();
            if heap.read_binary(reference)?.bit_length() != nonnegative_usize(*bit_length)? {
                return Err(ManagedMemoryError::InvalidBitRange);
            }
            Ok(reference.encoded_abi_word())
        }
        ManagedBitStringOperation::FromUintBe => encode_integer(heap, words, false, Endian::Big),
        ManagedBitStringOperation::FromIntBe => encode_integer(heap, words, true, Endian::Big),
        ManagedBitStringOperation::FromUintLe => encode_integer(heap, words, false, Endian::Little),
        ManagedBitStringOperation::FromIntLe => encode_integer(heap, words, true, Endian::Little),
        ManagedBitStringOperation::Utf8Scalar => encode_utf8(heap, words),
        ManagedBitStringOperation::Utf16BeScalar => encode_utf16(heap, words, Endian::Big),
        ManagedBitStringOperation::Utf16LeScalar => encode_utf16(heap, words, Endian::Little),
        ManagedBitStringOperation::Utf32BeScalar => encode_utf32(heap, words, Endian::Big),
        ManagedBitStringOperation::Utf32LeScalar => encode_utf32(heap, words, Endian::Little),
        ManagedBitStringOperation::ToUtf8Scalar => decode_utf8(heap, words),
        ManagedBitStringOperation::ToUtf16BeScalar => decode_utf16(heap, words, Endian::Big),
        ManagedBitStringOperation::ToUtf16LeScalar => decode_utf16(heap, words, Endian::Little),
        ManagedBitStringOperation::ToUtf32BeScalar => decode_utf32(heap, words, Endian::Big),
        ManagedBitStringOperation::ToUtf32LeScalar => decode_utf32(heap, words, Endian::Little),
        ManagedBitStringOperation::BitLength => {
            unary_view(heap, words).and_then(|view| scalar_usize(view.bit_length()))
        }
        ManagedBitStringOperation::ByteLength => {
            unary_view(heap, words).and_then(|view| scalar_usize(view.bit_length().div_ceil(8)))
        }
        ManagedBitStringOperation::IsByteAligned => {
            unary_view(heap, words).map(|view| u64::from(view.is_byte_aligned()))
        }
        ManagedBitStringOperation::Slice => slice(heap, words),
        ManagedBitStringOperation::Concat => concatenate(heap, words),
        ManagedBitStringOperation::ToBytes => to_bytes(heap, words),
        ManagedBitStringOperation::ToUintBe => {
            decode_integer_operation(heap, words, false, Endian::Big)
        }
        ManagedBitStringOperation::ToIntBe => {
            decode_integer_operation(heap, words, true, Endian::Big)
        }
        ManagedBitStringOperation::ToUintLe => {
            decode_integer_operation(heap, words, false, Endian::Little)
        }
        ManagedBitStringOperation::ToIntLe => {
            decode_integer_operation(heap, words, true, Endian::Little)
        }
    }
}

impl ManagedBitStringOperation {
    fn tag(self) -> u8 {
        match self {
            Self::FromBytes => 1,
            Self::FromAllBytes => 2,
            Self::FromExactBytes => 3,
            Self::RequireExactBits => 4,
            Self::FromUintBe => 5,
            Self::FromIntBe => 6,
            Self::FromUintLe => 7,
            Self::FromIntLe => 8,
            Self::Utf8Scalar => 9,
            Self::ToUtf8Scalar => 10,
            Self::Utf16BeScalar => 11,
            Self::Utf16LeScalar => 12,
            Self::ToUtf16BeScalar => 13,
            Self::ToUtf16LeScalar => 14,
            Self::Utf32BeScalar => 15,
            Self::Utf32LeScalar => 16,
            Self::ToUtf32BeScalar => 17,
            Self::ToUtf32LeScalar => 18,
            Self::BitLength => 19,
            Self::ByteLength => 20,
            Self::IsByteAligned => 21,
            Self::Slice => 22,
            Self::Concat => 23,
            Self::ToBytes => 24,
            Self::ToUintBe => 25,
            Self::ToIntBe => 26,
            Self::ToUintLe => 27,
            Self::ToIntLe => 28,
        }
    }

    fn from_tag(tag: u8) -> Option<Self> {
        Some(match tag {
            1 => Self::FromBytes,
            2 => Self::FromAllBytes,
            3 => Self::FromExactBytes,
            4 => Self::RequireExactBits,
            5 => Self::FromUintBe,
            6 => Self::FromIntBe,
            7 => Self::FromUintLe,
            8 => Self::FromIntLe,
            9 => Self::Utf8Scalar,
            10 => Self::ToUtf8Scalar,
            11 => Self::Utf16BeScalar,
            12 => Self::Utf16LeScalar,
            13 => Self::ToUtf16BeScalar,
            14 => Self::ToUtf16LeScalar,
            15 => Self::Utf32BeScalar,
            16 => Self::Utf32LeScalar,
            17 => Self::ToUtf32BeScalar,
            18 => Self::ToUtf32LeScalar,
            19 => Self::BitLength,
            20 => Self::ByteLength,
            21 => Self::IsByteAligned,
            22 => Self::Slice,
            23 => Self::Concat,
            24 => Self::ToBytes,
            25 => Self::ToUintBe,
            26 => Self::ToIntBe,
            27 => Self::ToUintLe,
            28 => Self::ToIntLe,
            _ => return None,
        })
    }

    fn result_is_reference(self) -> bool {
        matches!(
            self,
            Self::FromBytes
                | Self::FromAllBytes
                | Self::FromExactBytes
                | Self::RequireExactBits
                | Self::FromUintBe
                | Self::FromIntBe
                | Self::FromUintLe
                | Self::FromIntLe
                | Self::Utf8Scalar
                | Self::Utf16BeScalar
                | Self::Utf16LeScalar
                | Self::Utf32BeScalar
                | Self::Utf32LeScalar
                | Self::Slice
                | Self::Concat
                | Self::ToBytes
        )
    }
}

#[derive(Clone, Copy)]
enum Endian {
    Big,
    Little,
}

fn decode(encoded: &[u8]) -> Result<ManagedBitStringOperation, ManagedMemoryError> {
    let operation = encoded
        .get(6)
        .copied()
        .and_then(ManagedBitStringOperation::from_tag)
        .ok_or(ManagedMemoryError::InvalidManagedOperation)?;
    if encoded.len() != OPERATION_BYTES
        || encoded.get(..4) != Some(MAGIC)
        || encoded.get(4..6) != Some(&VERSION.to_le_bytes())
        || encoded[7] != u8::from(operation.result_is_reference())
    {
        return Err(ManagedMemoryError::InvalidManagedOperation);
    }
    Ok(operation)
}

fn unary_view<'a>(
    heap: &'a ActorHeap,
    words: &[i64],
) -> Result<ManagedBinaryView<'a>, ManagedMemoryError> {
    let [binary] = words else {
        return Err(ManagedMemoryError::InvalidAggregateArity);
    };
    heap.read_binary(reference_word(*binary)?.cast::<ManagedBinary>())
}

fn encode_integer(
    heap: &mut ActorHeap,
    words: &[i64],
    signed: bool,
    endian: Endian,
) -> Result<u64, ManagedMemoryError> {
    let [value, width] = words else {
        return Err(ManagedMemoryError::InvalidAggregateArity);
    };
    let width = integer_width(*width)?;
    let fits = if signed {
        let bound = 1_i64 << (width - 1);
        (-bound..bound).contains(value)
    } else {
        *value >= 0 && (*value as u64) < (1_u64 << width)
    };
    if !fits {
        return Err(ManagedMemoryError::InvalidManagedScalar);
    }
    let mask = (1_u64 << width) - 1;
    let raw = (*value as u64) & mask;
    let mut packed = vec![0_u8; width.div_ceil(8)];
    match endian {
        Endian::Big => {
            for output_bit in 0..width {
                if raw & (1_u64 << (width - output_bit - 1)) != 0 {
                    set_bit(&mut packed, output_bit);
                }
            }
        }
        Endian::Little => {
            let mut output_bit = 0;
            let mut group_start = 0;
            while group_start < width {
                let group_width = (width - group_start).min(8);
                for group_bit in (0..group_width).rev() {
                    if raw & (1_u64 << (group_start + group_bit)) != 0 {
                        set_bit(&mut packed, output_bit);
                    }
                    output_bit += 1;
                }
                group_start += group_width;
            }
        }
    }
    allocate_packed(heap, &packed, width)
}

fn decode_integer_operation(
    heap: &ActorHeap,
    words: &[i64],
    signed: bool,
    endian: Endian,
) -> Result<u64, ManagedMemoryError> {
    let view = unary_view(heap, words)?;
    let width = view.bit_length();
    if !(1..=63).contains(&width) {
        return Err(ManagedMemoryError::InvalidManagedScalar);
    }
    let raw = match endian {
        Endian::Big => {
            let mut value = 0_u64;
            for index in 0..width {
                value = (value << 1) | u64::from(view.bit(index).unwrap_or(false));
            }
            value
        }
        Endian::Little => {
            let mut value = 0_u64;
            let mut source_bit = 0;
            let mut group_start = 0;
            while group_start < width {
                let group_width = (width - group_start).min(8);
                for group_bit in (0..group_width).rev() {
                    if view.bit(source_bit).unwrap_or(false) {
                        value |= 1_u64 << (group_start + group_bit);
                    }
                    source_bit += 1;
                }
                group_start += group_width;
            }
            value
        }
    };
    let value = if signed && raw & (1_u64 << (width - 1)) != 0 {
        (raw | (!0_u64 << width)) as i64
    } else {
        raw as i64
    };
    Ok(u64::from_ne_bytes(value.to_ne_bytes()))
}

fn encode_utf8(heap: &mut ActorHeap, words: &[i64]) -> Result<u64, ManagedMemoryError> {
    let scalar = unary_scalar(words)?;
    let scalar = scalar_char(scalar)?;
    let mut buffer = [0_u8; 4];
    let encoded = scalar.encode_utf8(&mut buffer);
    allocate_packed(heap, encoded.as_bytes(), encoded.len() * 8)
}

fn encode_utf16(
    heap: &mut ActorHeap,
    words: &[i64],
    endian: Endian,
) -> Result<u64, ManagedMemoryError> {
    let scalar = scalar_char(unary_scalar(words)?)?;
    let mut units = [0_u16; 2];
    let mut packed = Vec::with_capacity(4);
    for unit in scalar.encode_utf16(&mut units) {
        let bytes = match endian {
            Endian::Big => unit.to_be_bytes(),
            Endian::Little => unit.to_le_bytes(),
        };
        packed.extend_from_slice(&bytes);
    }
    allocate_packed(heap, &packed, packed.len() * 8)
}

fn encode_utf32(
    heap: &mut ActorHeap,
    words: &[i64],
    endian: Endian,
) -> Result<u64, ManagedMemoryError> {
    let value = u32::from(scalar_char(unary_scalar(words)?)?);
    let packed = match endian {
        Endian::Big => value.to_be_bytes(),
        Endian::Little => value.to_le_bytes(),
    };
    allocate_packed(heap, &packed, 32)
}

fn decode_utf8(heap: &ActorHeap, words: &[i64]) -> Result<u64, ManagedMemoryError> {
    let packed = aligned_packed(unary_view(heap, words)?)?;
    let text = std::str::from_utf8(&packed).map_err(|_| ManagedMemoryError::InvalidUtf8)?;
    let mut chars = text.chars();
    let scalar = chars.next().ok_or(ManagedMemoryError::InvalidUtf8)?;
    if chars.next().is_some() {
        return Err(ManagedMemoryError::InvalidUtf8);
    }
    Ok(u64::from(u32::from(scalar)))
}

fn decode_utf16(
    heap: &ActorHeap,
    words: &[i64],
    endian: Endian,
) -> Result<u64, ManagedMemoryError> {
    let packed = aligned_packed(unary_view(heap, words)?)?;
    if !matches!(packed.len(), 2 | 4) {
        return Err(ManagedMemoryError::InvalidManagedScalar);
    }
    let units = packed.chunks_exact(2).map(|bytes| match endian {
        Endian::Big => u16::from_be_bytes([bytes[0], bytes[1]]),
        Endian::Little => u16::from_le_bytes([bytes[0], bytes[1]]),
    });
    let mut chars = char::decode_utf16(units);
    let scalar = chars
        .next()
        .transpose()
        .map_err(|_| ManagedMemoryError::InvalidManagedScalar)?
        .ok_or(ManagedMemoryError::InvalidManagedScalar)?;
    if chars.next().is_some() {
        return Err(ManagedMemoryError::InvalidManagedScalar);
    }
    Ok(u64::from(u32::from(scalar)))
}

fn decode_utf32(
    heap: &ActorHeap,
    words: &[i64],
    endian: Endian,
) -> Result<u64, ManagedMemoryError> {
    let packed: [u8; 4] = aligned_packed(unary_view(heap, words)?)?
        .try_into()
        .map_err(|_| ManagedMemoryError::InvalidManagedScalar)?;
    let value = match endian {
        Endian::Big => u32::from_be_bytes(packed),
        Endian::Little => u32::from_le_bytes(packed),
    };
    char::from_u32(value)
        .map(u32::from)
        .map(u64::from)
        .ok_or(ManagedMemoryError::InvalidManagedScalar)
}

fn slice(heap: &mut ActorHeap, words: &[i64]) -> Result<u64, ManagedMemoryError> {
    let [binary, start, bit_length] = words else {
        return Err(ManagedMemoryError::InvalidAggregateArity);
    };
    let view = heap.read_binary(reference_word(*binary)?.cast::<ManagedBinary>())?;
    let start = nonnegative_usize(*start)?;
    let bit_length = nonnegative_usize(*bit_length)?;
    let end = start
        .checked_add(bit_length)
        .ok_or(ManagedMemoryError::InvalidBitRange)?;
    if end > view.bit_length() {
        return Err(ManagedMemoryError::InvalidBitRange);
    }
    let packed = pack_range(view, start, bit_length)?;
    allocate_packed(heap, &packed, bit_length)
}

fn concatenate(heap: &mut ActorHeap, words: &[i64]) -> Result<u64, ManagedMemoryError> {
    let [left, right] = words else {
        return Err(ManagedMemoryError::InvalidAggregateArity);
    };
    let left = heap.read_binary(reference_word(*left)?.cast::<ManagedBinary>())?;
    let right = heap.read_binary(reference_word(*right)?.cast::<ManagedBinary>())?;
    let bit_length = left
        .bit_length()
        .checked_add(right.bit_length())
        .ok_or(ManagedMemoryError::InvalidSequenceLength)?;
    let mut packed = vec![0_u8; bit_length.div_ceil(8)];
    copy_bits(left, &mut packed, 0)?;
    copy_bits(right, &mut packed, left.bit_length())?;
    allocate_packed(heap, &packed, bit_length)
}

fn to_bytes(heap: &mut ActorHeap, words: &[i64]) -> Result<u64, ManagedMemoryError> {
    let packed = aligned_packed(unary_view(heap, words)?)?;
    heap.allocate_bytes(&packed).map(TvmRef::encoded_abi_word)
}

fn aligned_packed(view: ManagedBinaryView<'_>) -> Result<Vec<u8>, ManagedMemoryError> {
    if !view.is_byte_aligned() {
        return Err(ManagedMemoryError::InvalidBitRange);
    }
    pack_range(view, 0, view.bit_length())
}

fn pack_range(
    source: ManagedBinaryView<'_>,
    start: usize,
    bit_length: usize,
) -> Result<Vec<u8>, ManagedMemoryError> {
    let mut output = vec![0_u8; bit_length.div_ceil(8)];
    for index in 0..bit_length {
        if source
            .bit(start + index)
            .ok_or(ManagedMemoryError::InvalidBitRange)?
        {
            set_bit(&mut output, index);
        }
    }
    Ok(output)
}

fn copy_bits(
    source: ManagedBinaryView<'_>,
    output: &mut [u8],
    offset: usize,
) -> Result<(), ManagedMemoryError> {
    for index in 0..source.bit_length() {
        if source
            .bit(index)
            .ok_or(ManagedMemoryError::InvalidBitRange)?
        {
            set_bit(output, offset + index);
        }
    }
    Ok(())
}

fn allocate_packed(
    heap: &mut ActorHeap,
    packed: &[u8],
    bit_length: usize,
) -> Result<u64, ManagedMemoryError> {
    let storage = heap.allocate_bytes(packed)?;
    heap.allocate_binary(storage, 0, bit_length)
        .map(TvmRef::encoded_abi_word)
}

fn unary_scalar(words: &[i64]) -> Result<i64, ManagedMemoryError> {
    let [value] = words else {
        return Err(ManagedMemoryError::InvalidAggregateArity);
    };
    Ok(*value)
}

fn scalar_char(value: i64) -> Result<char, ManagedMemoryError> {
    u32::try_from(value)
        .ok()
        .and_then(char::from_u32)
        .ok_or(ManagedMemoryError::InvalidManagedScalar)
}

fn integer_width(value: i64) -> Result<usize, ManagedMemoryError> {
    nonnegative_usize(value)
        .ok()
        .filter(|width| (1..=63).contains(width))
        .ok_or(ManagedMemoryError::InvalidManagedScalar)
}

fn nonnegative_usize(value: i64) -> Result<usize, ManagedMemoryError> {
    usize::try_from(value).map_err(|_| ManagedMemoryError::InvalidManagedScalar)
}

fn scalar_usize(value: usize) -> Result<u64, ManagedMemoryError> {
    i64::try_from(value)
        .map(|value| u64::from_ne_bytes(value.to_ne_bytes()))
        .map_err(|_| ManagedMemoryError::InvalidSequenceLength)
}

fn set_bit(bytes: &mut [u8], index: usize) {
    bytes[index / 8] |= 1 << (7 - index % 8);
}

fn mask_trailing_bits(bytes: &mut [u8], bit_length: usize) {
    let used = bit_length % 8;
    if used != 0 {
        if let Some(last) = bytes.last_mut() {
            *last &= u8::MAX << (8 - used);
        }
    }
}
