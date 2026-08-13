//! Encoders for actor-local managed-memory accounting.

use super::{header, MEMORY_RETAINED_SIZE, MEMORY_SHALLOW_SIZE};

/// Encodes managed shallow-size inspection for one validated object reference.
pub fn encode_memory_shallow_size_operation() -> Vec<u8> {
    header(MEMORY_SHALLOW_SIZE)
}

/// Encodes managed retained-size inspection for one validated object graph.
pub fn encode_memory_retained_size_operation() -> Vec<u8> {
    header(MEMORY_RETAINED_SIZE)
}
