#![allow(dead_code)]

use std::sync::atomic::{AtomicU64, Ordering};

/// Integer interpretation used by a VM-owned atomic array.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmAtomicKind {
    Signed,
    Unsigned,
}

/// Typed value returned by a VM-owned atomic array.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmAtomicValue {
    Signed(i64),
    Unsigned(u64),
}

/// Stable validation failures for VM-owned atomic operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmAtomicError {
    EmptyArray,
    IndexOutOfBounds { index: usize, len: usize },
    ValueKindMismatch,
    DeltaOutOfRange,
}

/// Result of a compare-and-exchange operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmCompareExchange {
    Exchanged,
    Mismatch(VmAtomicValue),
}

/// VM-owned fixed-size array of 64-bit atomic integers.
///
/// Values are stored as raw two's-complement bits. The declared kind controls
/// input validation and result interpretation, while arithmetic wraps at the
/// 64-bit boundary. Sequentially consistent operations give VM processes one
/// deterministic global order without exposing host pointer or ERTS details.
#[derive(Debug)]
pub(crate) struct VmAtomicArray {
    kind: VmAtomicKind,
    cells: Box<[AtomicU64]>,
}

impl VmAtomicArray {
    pub(crate) fn new(len: usize, kind: VmAtomicKind) -> Result<Self, VmAtomicError> {
        if len == 0 {
            return Err(VmAtomicError::EmptyArray);
        }
        let cells = (0..len)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Ok(Self { kind, cells })
    }

    pub(crate) fn len(&self) -> usize {
        self.cells.len()
    }

    pub(crate) fn kind(&self) -> VmAtomicKind {
        self.kind
    }

    pub(crate) fn limits(&self) -> (VmAtomicValue, VmAtomicValue) {
        match self.kind {
            VmAtomicKind::Signed => (
                VmAtomicValue::Signed(i64::MIN),
                VmAtomicValue::Signed(i64::MAX),
            ),
            VmAtomicKind::Unsigned => (
                VmAtomicValue::Unsigned(0),
                VmAtomicValue::Unsigned(u64::MAX),
            ),
        }
    }

    pub(crate) fn get(&self, index: usize) -> Result<VmAtomicValue, VmAtomicError> {
        Ok(self.value(self.cell(index)?.load(Ordering::SeqCst)))
    }

    pub(crate) fn put(&self, index: usize, value: VmAtomicValue) -> Result<(), VmAtomicError> {
        let bits = self.value_bits(value)?;
        self.cell(index)?.store(bits, Ordering::SeqCst);
        Ok(())
    }

    pub(crate) fn add(&self, index: usize, delta: i128) -> Result<(), VmAtomicError> {
        self.add_get_bits(index, delta).map(|_| ())
    }

    pub(crate) fn add_get(
        &self,
        index: usize,
        delta: i128,
    ) -> Result<VmAtomicValue, VmAtomicError> {
        Ok(self.value(self.add_get_bits(index, delta)?))
    }

    pub(crate) fn sub(&self, index: usize, delta: i128) -> Result<(), VmAtomicError> {
        self.sub_get_bits(index, delta).map(|_| ())
    }

    pub(crate) fn sub_get(
        &self,
        index: usize,
        delta: i128,
    ) -> Result<VmAtomicValue, VmAtomicError> {
        Ok(self.value(self.sub_get_bits(index, delta)?))
    }

    pub(crate) fn exchange(
        &self,
        index: usize,
        value: VmAtomicValue,
    ) -> Result<VmAtomicValue, VmAtomicError> {
        let bits = self.value_bits(value)?;
        Ok(self.value(self.cell(index)?.swap(bits, Ordering::SeqCst)))
    }

    pub(crate) fn compare_exchange(
        &self,
        index: usize,
        expected: VmAtomicValue,
        replacement: VmAtomicValue,
    ) -> Result<VmCompareExchange, VmAtomicError> {
        let expected = self.value_bits(expected)?;
        let replacement = self.value_bits(replacement)?;
        match self.cell(index)?.compare_exchange(
            expected,
            replacement,
            Ordering::SeqCst,
            Ordering::SeqCst,
        ) {
            Ok(_) => Ok(VmCompareExchange::Exchanged),
            Err(actual) => Ok(VmCompareExchange::Mismatch(self.value(actual))),
        }
    }

    fn add_get_bits(&self, index: usize, delta: i128) -> Result<u64, VmAtomicError> {
        let delta = delta_bits(delta)?;
        let previous = self.cell(index)?.fetch_add(delta, Ordering::SeqCst);
        Ok(previous.wrapping_add(delta))
    }

    fn sub_get_bits(&self, index: usize, delta: i128) -> Result<u64, VmAtomicError> {
        let delta = delta_bits(delta)?;
        let previous = self.cell(index)?.fetch_sub(delta, Ordering::SeqCst);
        Ok(previous.wrapping_sub(delta))
    }

    fn cell(&self, index: usize) -> Result<&AtomicU64, VmAtomicError> {
        self.cells
            .get(index)
            .ok_or(VmAtomicError::IndexOutOfBounds {
                index,
                len: self.cells.len(),
            })
    }

    fn value_bits(&self, value: VmAtomicValue) -> Result<u64, VmAtomicError> {
        match (self.kind, value) {
            (VmAtomicKind::Signed, VmAtomicValue::Signed(value)) => Ok(value as u64),
            (VmAtomicKind::Unsigned, VmAtomicValue::Unsigned(value)) => Ok(value),
            _ => Err(VmAtomicError::ValueKindMismatch),
        }
    }

    fn value(&self, bits: u64) -> VmAtomicValue {
        match self.kind {
            VmAtomicKind::Signed => VmAtomicValue::Signed(bits as i64),
            VmAtomicKind::Unsigned => VmAtomicValue::Unsigned(bits),
        }
    }
}

fn delta_bits(delta: i128) -> Result<u64, VmAtomicError> {
    if delta < i64::MIN as i128 || delta > u64::MAX as i128 {
        return Err(VmAtomicError::DeltaOutOfRange);
    }
    Ok(delta as u64)
}

#[cfg(test)]
#[path = "atomic_beam_suite_parity_test.rs"]
mod atomic_beam_suite_parity_test;
