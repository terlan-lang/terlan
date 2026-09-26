//! Checked TETF output allocation, including temporary canonical sort keys.

use crate::runtime::vm::VmRuntimeResult;

/// A growable output whose logical length and reserved capacity have a hard bound.
pub(super) struct EncodingBuffer {
    bytes: Vec<u8>,
    maximum: usize,
}

#[cfg(test)]
#[path = "buffer_test.rs"]
mod tests;

impl EncodingBuffer {
    pub(super) const fn new(maximum: usize) -> Self {
        Self {
            bytes: Vec::new(),
            maximum,
        }
    }

    pub(super) fn remaining(&self) -> usize {
        self.maximum - self.bytes.len()
    }

    pub(super) fn require(&self, additional: usize) -> VmRuntimeResult<()> {
        if additional > self.remaining() {
            return Err("error[tetf_size]: encoded term exceeds byte limit".into());
        }
        Ok(())
    }

    pub(super) fn push(&mut self, byte: u8) -> VmRuntimeResult<()> {
        self.extend_from_slice(&[byte])
    }

    pub(super) fn extend_from_slice(&mut self, bytes: &[u8]) -> VmRuntimeResult<()> {
        self.require(bytes.len())?;
        let required = self.bytes.len() + bytes.len();
        if required > self.bytes.capacity() {
            let capacity = required
                .max(self.bytes.capacity().saturating_mul(2))
                .min(self.maximum);
            self.bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .map_err(|_| "error[tetf_allocation]: unable to reserve bounded encoding buffer")?;
        }
        self.bytes.extend_from_slice(bytes);
        Ok(())
    }

    pub(super) fn into_vec(self) -> Vec<u8> {
        self.bytes
    }
}
