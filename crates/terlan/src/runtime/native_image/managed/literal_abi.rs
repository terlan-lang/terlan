//! Bounded immutable literal contract between native code and actor heaps.

use super::{ActorHeap, ManagedMemoryError};

const STRING_MAGIC: &[u8; 4] = b"TVMS";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 10;

/// Maximum encoded managed literal accepted from one native image call.
pub const MAX_MANAGED_LITERAL_ABI_BYTES: usize = 64 * 1024;

/// Encodes one UTF-8 string literal for immutable native object data.
pub fn encode_string_literal(value: &str) -> Result<Vec<u8>, ManagedMemoryError> {
    let length = u32::try_from(value.len()).map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    let total = HEADER_BYTES
        .checked_add(value.len())
        .ok_or(ManagedMemoryError::InvalidAggregateAbi)?;
    if total > MAX_MANAGED_LITERAL_ABI_BYTES {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    let mut bytes = Vec::with_capacity(total);
    bytes.extend_from_slice(STRING_MAGIC);
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.extend_from_slice(&length.to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
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
        let value = decode_string_literal(encoded)?;
        let reference = self.allocate_string(value)?;
        u64::try_from(reference.encoded().get())
            .map_err(|_| ManagedMemoryError::UnsupportedPointerWidth)
    }
}

/// Decodes and validates one immutable UTF-8 string allocation payload.
fn decode_string_literal(bytes: &[u8]) -> Result<&str, ManagedMemoryError> {
    if bytes.len() < HEADER_BYTES || bytes.len() > MAX_MANAGED_LITERAL_ABI_BYTES {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    if bytes.get(..4) != Some(STRING_MAGIC) || bytes.get(4..6) != Some(&VERSION.to_le_bytes()) {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    let length = bytes
        .get(6..10)
        .and_then(|value| <[u8; 4]>::try_from(value).ok())
        .map(u32::from_le_bytes)
        .ok_or(ManagedMemoryError::InvalidAggregateAbi)? as usize;
    let payload = bytes
        .get(HEADER_BYTES..)
        .filter(|payload| payload.len() == length)
        .ok_or(ManagedMemoryError::InvalidAggregateAbi)?;
    std::str::from_utf8(payload).map_err(|_| ManagedMemoryError::InvalidAggregateAbi)
}

#[cfg(test)]
#[path = "literal_abi_test.rs"]
mod literal_abi_test;
