#[cfg(test)]
use super::atomic::{VmAtomicArray, VmAtomicError, VmAtomicKind, VmAtomicValue};

/// Publication mode requested for a VM counter array.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum VmCounterMode {
    Atomic,
    WriteConcurrent,
}

/// Stable validation failures for counter operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) enum VmCounterError {
    EmptyArray,
    IndexOutOfBounds { index: usize, len: usize },
    ValueOutOfRange,
    DeltaOutOfRange,
    LogicalSizeOverflow,
}

/// Deterministic counter metadata independent of host scheduler layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg(test)]
pub(crate) struct VmCounterInfo {
    pub(crate) len: usize,
    pub(crate) logical_bytes: usize,
    pub(crate) mode: VmCounterMode,
}

/// Fixed-size VM-owned signed counter array.
///
/// Both modes use sequentially consistent 64-bit cells. `WriteConcurrent`
/// declares intent for heavily contended callers without exposing ERTS
/// scheduler shards or host allocation geometry.
#[derive(Debug)]
#[cfg(test)]
pub(crate) struct VmCounterArray {
    mode: VmCounterMode,
    cells: VmAtomicArray,
}

#[cfg(test)]
impl VmCounterArray {
    pub(crate) fn new(len: usize, mode: VmCounterMode) -> Result<Self, VmCounterError> {
        len.checked_mul(std::mem::size_of::<u64>())
            .ok_or(VmCounterError::LogicalSizeOverflow)?;
        let cells = VmAtomicArray::new(len, VmAtomicKind::Signed).map_err(map_atomic_error)?;
        Ok(Self { mode, cells })
    }

    pub(crate) fn info(&self) -> VmCounterInfo {
        VmCounterInfo {
            len: self.cells.len(),
            logical_bytes: self.cells.len() * std::mem::size_of::<u64>(),
            mode: self.mode,
        }
    }

    pub(crate) fn get(&self, index: usize) -> Result<i64, VmCounterError> {
        match self.cells.get(index).map_err(map_atomic_error)? {
            VmAtomicValue::Signed(value) => Ok(value),
            VmAtomicValue::Unsigned(_) => unreachable!("counter storage is always signed"),
        }
    }

    pub(crate) fn put(&self, index: usize, value: i128) -> Result<(), VmCounterError> {
        let value = i64::try_from(value).map_err(|_| VmCounterError::ValueOutOfRange)?;
        self.cells
            .put(index, VmAtomicValue::Signed(value))
            .map_err(map_atomic_error)
    }

    pub(crate) fn add(&self, index: usize, delta: i128) -> Result<(), VmCounterError> {
        self.cells.add(index, delta).map_err(map_atomic_error)
    }

    pub(crate) fn sub(&self, index: usize, delta: i128) -> Result<(), VmCounterError> {
        self.cells.sub(index, delta).map_err(map_atomic_error)
    }
}

#[cfg(test)]
fn map_atomic_error(error: VmAtomicError) -> VmCounterError {
    match error {
        VmAtomicError::EmptyArray => VmCounterError::EmptyArray,
        VmAtomicError::IndexOutOfBounds { index, len } => {
            VmCounterError::IndexOutOfBounds { index, len }
        }
        VmAtomicError::DeltaOutOfRange => VmCounterError::DeltaOutOfRange,
        VmAtomicError::ValueKindMismatch => {
            unreachable!("counter storage only accepts signed values")
        }
    }
}

#[cfg(test)]
#[path = "counter_beam_suite_parity_test.rs"]
#[cfg(test)]
mod counter_beam_suite_parity_test;
