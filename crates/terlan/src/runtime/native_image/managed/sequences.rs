//! Actor-local immutable string, byte, and bitstring values.

use std::sync::{Arc, OnceLock};

use super::{
    managed_binary_semantic_id, managed_bytes_semantic_id, managed_string_semantic_id, ActorHeap,
    AllocationClass, ManagedMemoryError, ManagedTypeDescriptor, SemanticTypeId, TvmRef,
};

/// Bytes reserved for the canonical sequence byte-length prefix.
pub const MANAGED_SEQUENCE_HEADER_BYTES: usize = std::mem::size_of::<u64>();
const BINARY_STORAGE_OFFSET: usize = 0;
const BINARY_BIT_OFFSET_OFFSET: usize = std::mem::size_of::<usize>();
const BINARY_BIT_LENGTH_OFFSET: usize = BINARY_BIT_OFFSET_OFFSET + std::mem::size_of::<u64>();
const BINARY_PAYLOAD_BYTES: usize = std::mem::size_of::<usize>() + 2 * std::mem::size_of::<u64>();

/// Compile-time marker for one immutable, valid UTF-8 managed string.
#[derive(Debug)]
pub struct ManagedString;

/// Compile-time marker for one immutable managed byte sequence.
#[derive(Debug)]
pub struct ManagedBytes;

/// Compile-time marker for one checked managed bitstring slice.
#[derive(Debug)]
pub struct ManagedBinary;

/// Borrowed semantic view of a managed bitstring slice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ManagedBinaryView<'a> {
    storage: &'a [u8],
    bit_offset: usize,
    bit_length: usize,
}

impl<'a> ManagedBinaryView<'a> {
    /// Returns the complete immutable backing byte sequence.
    pub fn storage(self) -> &'a [u8] {
        self.storage
    }

    /// Returns the checked starting bit offset in the backing sequence.
    pub fn bit_offset(self) -> usize {
        self.bit_offset
    }

    /// Returns the logical number of bits in this slice.
    pub fn bit_length(self) -> usize {
        self.bit_length
    }

    /// Reports whether both slice boundaries are byte aligned.
    pub fn is_byte_aligned(self) -> bool {
        self.bit_offset.is_multiple_of(8) && self.bit_length.is_multiple_of(8)
    }

    /// Returns a zero-copy byte slice when both boundaries are byte aligned.
    pub fn aligned_bytes(self) -> Option<&'a [u8]> {
        if !self.is_byte_aligned() {
            return None;
        }
        let start = self.bit_offset / 8;
        let end = start.checked_add(self.bit_length / 8)?;
        self.storage.get(start..end)
    }

    /// Reads one logical bit in most-significant-bit-first binary order.
    pub fn bit(self, index: usize) -> Option<bool> {
        if index >= self.bit_length {
            return None;
        }
        let absolute = self.bit_offset.checked_add(index)?;
        let byte = *self.storage.get(absolute / 8)?;
        Some(byte & (1 << (7 - absolute % 8)) != 0)
    }
}

impl ActorHeap {
    /// Allocates one immutable UTF-8 string in this actor's managed heap.
    pub fn allocate_string(
        &mut self,
        value: &str,
    ) -> Result<TvmRef<ManagedString>, ManagedMemoryError> {
        self.allocate_string_bytes(value.as_bytes())
    }

    /// Validates UTF-8 bytes and allocates the resulting immutable string.
    pub fn allocate_string_bytes(
        &mut self,
        value: &[u8],
    ) -> Result<TvmRef<ManagedString>, ManagedMemoryError> {
        std::str::from_utf8(value).map_err(|_| ManagedMemoryError::InvalidUtf8)?;
        allocate_sequence(self, managed_string_semantic_id(), value)
    }

    /// Reads one managed string after owner, generation, and semantic-type checks.
    pub fn read_string(&self, value: TvmRef<ManagedString>) -> Result<&str, ManagedMemoryError> {
        require_semantic_type(self, value, managed_string_semantic_id())?;
        std::str::from_utf8(sequence_bytes(self.read(value)?)?)
            .map_err(|_| ManagedMemoryError::InvalidUtf8)
    }

    /// Concatenates two validated actor-owned strings directly into the
    /// managed heap without allocating an intermediate host `String`.
    pub(super) fn concatenate_strings(
        &mut self,
        left: TvmRef<ManagedString>,
        right: TvmRef<ManagedString>,
    ) -> Result<TvmRef<ManagedString>, ManagedMemoryError> {
        let left_length = self.read_string(left)?.len();
        let right_length = self.read_string(right)?.len();
        let value_length = left_length
            .checked_add(right_length)
            .ok_or(ManagedMemoryError::InvalidSequenceLength)?;
        let mut left_range = self.payload_range(left)?;
        let mut right_range = self.payload_range(right)?;
        left_range.start = left_range
            .start
            .checked_add(MANAGED_SEQUENCE_HEADER_BYTES)
            .ok_or(ManagedMemoryError::InvalidSequenceLength)?;
        right_range.start = right_range
            .start
            .checked_add(MANAGED_SEQUENCE_HEADER_BYTES)
            .ok_or(ManagedMemoryError::InvalidSequenceLength)?;
        if left_range.len() != left_length || right_range.len() != right_length {
            return Err(ManagedMemoryError::InvalidSequenceLength);
        }
        allocate_sequence_ranges(
            self,
            managed_string_semantic_id(),
            value_length,
            &[left_range, right_range],
        )
    }

    /// Allocates one immutable byte sequence in this actor's managed heap.
    pub fn allocate_bytes(
        &mut self,
        value: &[u8],
    ) -> Result<TvmRef<ManagedBytes>, ManagedMemoryError> {
        allocate_sequence(self, managed_bytes_semantic_id(), value)
    }

    /// Reads one managed byte sequence after owner, generation, and type checks.
    pub fn read_bytes(&self, value: TvmRef<ManagedBytes>) -> Result<&[u8], ManagedMemoryError> {
        require_semantic_type(self, value, managed_bytes_semantic_id())?;
        sequence_bytes(self.read(value)?)
    }

    /// Allocates a checked bitstring slice over actor-owned immutable bytes.
    pub fn allocate_binary(
        &mut self,
        storage: TvmRef<ManagedBytes>,
        bit_offset: usize,
        bit_length: usize,
    ) -> Result<TvmRef<ManagedBinary>, ManagedMemoryError> {
        require_semantic_type(self, storage, managed_bytes_semantic_id())?;
        validate_bit_range(self.read_bytes(storage)?.len(), bit_offset, bit_length)?;
        let mut payload = [0_u8; BINARY_PAYLOAD_BYTES];
        payload[BINARY_BIT_OFFSET_OFFSET..BINARY_BIT_LENGTH_OFFSET]
            .copy_from_slice(&encode_usize(bit_offset)?);
        payload[BINARY_BIT_LENGTH_OFFSET..].copy_from_slice(&encode_usize(bit_length)?);
        self.allocate(
            binary_descriptor(),
            &payload,
            &[(BINARY_STORAGE_OFFSET, storage.erase())],
        )
    }

    /// Reads a checked semantic view over one managed bitstring slice.
    pub fn read_binary(
        &self,
        value: TvmRef<ManagedBinary>,
    ) -> Result<ManagedBinaryView<'_>, ManagedMemoryError> {
        require_semantic_type(self, value, managed_binary_semantic_id())?;
        let payload = self.read(value)?;
        let bit_offset = decode_usize(payload, BINARY_BIT_OFFSET_OFFSET)?;
        let bit_length = decode_usize(payload, BINARY_BIT_LENGTH_OFFSET)?;
        let storage = self.reference_field(value, BINARY_STORAGE_OFFSET)?;
        require_semantic_type(self, storage, managed_bytes_semantic_id())?;
        let storage = self.read_bytes(storage.cast())?;
        validate_bit_range(storage.len(), bit_offset, bit_length)?;
        Ok(ManagedBinaryView {
            storage,
            bit_offset,
            bit_length,
        })
    }
}

/// Builds a deterministic actor-local sequence descriptor and payload.
fn allocate_sequence<T>(
    heap: &mut ActorHeap,
    semantic: SemanticTypeId,
    value: &[u8],
) -> Result<TvmRef<T>, ManagedMemoryError> {
    let length =
        u64::try_from(value.len()).map_err(|_| ManagedMemoryError::InvalidSequenceLength)?;
    let size = MANAGED_SEQUENCE_HEADER_BYTES
        .checked_add(value.len())
        .ok_or(ManagedMemoryError::InvalidSequenceLength)?;
    let allocation_class = if size > 64 * 1024 {
        AllocationClass::Large
    } else {
        AllocationClass::Young
    };
    let descriptor = heap.sequence_descriptor(semantic, size, allocation_class)?;
    let header = length.to_le_bytes();
    heap.allocate_reference_free_parts(descriptor, &[&header, value])
}

/// Builds a sequence by copying exact source ranges already inside the heap.
fn allocate_sequence_ranges<T>(
    heap: &mut ActorHeap,
    semantic: SemanticTypeId,
    value_length: usize,
    ranges: &[std::ops::Range<usize>],
) -> Result<TvmRef<T>, ManagedMemoryError> {
    let length =
        u64::try_from(value_length).map_err(|_| ManagedMemoryError::InvalidSequenceLength)?;
    let size = MANAGED_SEQUENCE_HEADER_BYTES
        .checked_add(value_length)
        .ok_or(ManagedMemoryError::InvalidSequenceLength)?;
    let allocation_class = if size > 64 * 1024 {
        AllocationClass::Large
    } else {
        AllocationClass::Young
    };
    let descriptor = heap.sequence_descriptor(semantic, size, allocation_class)?;
    heap.allocate_reference_free_ranges(descriptor, &length.to_le_bytes(), ranges)
}

/// Returns the length-delimited bytes from one validated sequence payload.
fn sequence_bytes(payload: &[u8]) -> Result<&[u8], ManagedMemoryError> {
    let length = decode_usize(payload, 0)?;
    let end = MANAGED_SEQUENCE_HEADER_BYTES
        .checked_add(length)
        .ok_or(ManagedMemoryError::InvalidSequenceLength)?;
    if end != payload.len() {
        return Err(ManagedMemoryError::InvalidSequenceLength);
    }
    payload
        .get(MANAGED_SEQUENCE_HEADER_BYTES..end)
        .ok_or(ManagedMemoryError::InvalidSequenceLength)
}

/// Builds the fixed descriptor for a bitstring slice object.
fn binary_descriptor() -> Arc<ManagedTypeDescriptor> {
    static DESCRIPTOR: OnceLock<Arc<ManagedTypeDescriptor>> = OnceLock::new();
    Arc::clone(DESCRIPTOR.get_or_init(|| {
        Arc::new(
            ManagedTypeDescriptor::new(
                managed_binary_semantic_id(),
                BINARY_PAYLOAD_BYTES,
                8,
                vec![BINARY_STORAGE_OFFSET],
                AllocationClass::Young,
            )
            .expect("canonical managed Binary layout is valid"),
        )
    }))
}

/// Ensures a managed reference carries the expected canonical semantic identity.
fn require_semantic_type<T>(
    heap: &ActorHeap,
    value: TvmRef<T>,
    expected: SemanticTypeId,
) -> Result<(), ManagedMemoryError> {
    if heap.descriptor(value)?.semantic_id() != expected {
        return Err(ManagedMemoryError::ManagedTypeMismatch);
    }
    Ok(())
}

/// Validates checked bit arithmetic against the backing byte sequence.
fn validate_bit_range(
    byte_length: usize,
    bit_offset: usize,
    bit_length: usize,
) -> Result<(), ManagedMemoryError> {
    let available = byte_length
        .checked_mul(8)
        .ok_or(ManagedMemoryError::InvalidBitRange)?;
    let end = bit_offset
        .checked_add(bit_length)
        .ok_or(ManagedMemoryError::InvalidBitRange)?;
    if end > available {
        return Err(ManagedMemoryError::InvalidBitRange);
    }
    Ok(())
}

/// Encodes one host length into the fixed little-endian ABI field.
fn encode_usize(value: usize) -> Result<[u8; 8], ManagedMemoryError> {
    u64::try_from(value)
        .map(u64::to_le_bytes)
        .map_err(|_| ManagedMemoryError::InvalidSequenceLength)
}

/// Decodes one fixed little-endian ABI length field.
fn decode_usize(payload: &[u8], offset: usize) -> Result<usize, ManagedMemoryError> {
    let end = offset
        .checked_add(8)
        .ok_or(ManagedMemoryError::InvalidSequenceLength)?;
    let bytes: [u8; 8] = payload
        .get(offset..end)
        .ok_or(ManagedMemoryError::InvalidSequenceLength)?
        .try_into()
        .map_err(|_| ManagedMemoryError::InvalidSequenceLength)?;
    usize::try_from(u64::from_le_bytes(bytes))
        .map_err(|_| ManagedMemoryError::InvalidSequenceLength)
}

#[cfg(test)]
#[path = "managed_sequence_test.rs"]
mod managed_sequence_test;
