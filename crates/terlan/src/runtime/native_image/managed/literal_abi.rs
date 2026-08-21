//! Bounded immutable literal contract between native code and actor heaps.

use super::{ActorHeap, ManagedMemoryError};

const STRING_MAGIC: &[u8; 4] = b"TVMS";
const BINARY_MAGIC: &[u8; 4] = b"TVML";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 10;

/// Maximum encoded managed literal accepted from one native image call.
pub const MAX_MANAGED_LITERAL_ABI_BYTES: usize = 64 * 1024;

/// Encodes one UTF-8 string literal for immutable native object data.
pub fn encode_string_literal(value: &str) -> Result<Vec<u8>, ManagedMemoryError> {
    encode_literal(STRING_MAGIC, value.as_bytes())
}

/// Encodes one byte-aligned Binary literal for immutable native object data.
pub fn encode_binary_literal(value: &[u8]) -> Result<Vec<u8>, ManagedMemoryError> {
    encode_literal(BINARY_MAGIC, value)
}

fn encode_literal(magic: &[u8; 4], value: &[u8]) -> Result<Vec<u8>, ManagedMemoryError> {
    let length = u32::try_from(value.len()).map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    let total = HEADER_BYTES
        .checked_add(value.len())
        .ok_or(ManagedMemoryError::InvalidAggregateAbi)?;
    if total > MAX_MANAGED_LITERAL_ABI_BYTES {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    let mut bytes = Vec::with_capacity(total);
    bytes.extend_from_slice(magic);
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.extend_from_slice(&length.to_le_bytes());
    bytes.extend_from_slice(value);
    Ok(bytes)
}

impl ActorHeap {
    /// Allocates one compiler-owned literal or aggregate from bounded ABI data.
    pub(crate) fn allocate_managed_words_abi(
        &mut self,
        encoded: &[u8],
        words: &[i64],
    ) -> Result<u64, ManagedMemoryError> {
        if encoded.starts_with(b"TVMA") {
            return self
                .allocate_aggregate_words_abi(encoded, words)
                .map(|(reference, _)| reference);
        }
        if !words.is_empty() {
            return Err(ManagedMemoryError::InvalidAggregateArity);
        }
        let reference = if encoded.starts_with(STRING_MAGIC) {
            self.allocate_string(decode_string_literal(encoded)?)?
                .erase()
        } else if encoded.starts_with(BINARY_MAGIC) {
            let value = decode_literal(encoded, BINARY_MAGIC)?;
            self.with_allocation_transaction(|heap| {
                let storage = heap.allocate_bytes(value)?;
                heap.allocate_binary(storage, 0, value.len().saturating_mul(8))
            })?
            .erase()
        } else {
            return Err(ManagedMemoryError::InvalidAggregateAbi);
        };
        u64::try_from(reference.encoded().get())
            .map_err(|_| ManagedMemoryError::UnsupportedPointerWidth)
    }
}

/// Decodes and validates one immutable UTF-8 string allocation payload.
fn decode_string_literal(bytes: &[u8]) -> Result<&str, ManagedMemoryError> {
    std::str::from_utf8(decode_literal(bytes, STRING_MAGIC)?)
        .map_err(|_| ManagedMemoryError::InvalidAggregateAbi)
}

fn decode_literal<'a>(bytes: &'a [u8], magic: &[u8; 4]) -> Result<&'a [u8], ManagedMemoryError> {
    if bytes.len() < HEADER_BYTES || bytes.len() > MAX_MANAGED_LITERAL_ABI_BYTES {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    if bytes.get(..4) != Some(magic) || bytes.get(4..6) != Some(&VERSION.to_le_bytes()) {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    let length = bytes
        .get(6..10)
        .and_then(|value| <[u8; 4]>::try_from(value).ok())
        .map(u32::from_le_bytes)
        .ok_or(ManagedMemoryError::InvalidAggregateAbi)? as usize;
    bytes
        .get(HEADER_BYTES..)
        .filter(|payload| payload.len() == length)
        .ok_or(ManagedMemoryError::InvalidAggregateAbi)
}

#[cfg(test)]
#[path = "literal_abi_test.rs"]
#[cfg(test)]
mod literal_abi_test;
