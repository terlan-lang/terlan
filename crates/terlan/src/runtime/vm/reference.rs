#![allow(dead_code)]

use std::fmt;
use std::sync::Arc;

#[cfg(test)]
#[path = "reference_test.rs"]
mod reference_test;

/// Distribution-safe identity allocated by one Terlan VM instance.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct VmReferenceId {
    node_id: Arc<str>,
    epoch: u64,
    local_id: u64,
}

impl VmReferenceId {
    /// Returns the VM node that allocated this reference.
    pub(crate) fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Returns the allocating VM boot epoch.
    pub(crate) const fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Returns the monotonic identity within the node epoch.
    pub(crate) const fn as_u64(&self) -> u64 {
        self.local_id
    }
}

/// Typed failure produced by VM reference and unique-integer allocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum VmReferenceAllocationError {
    ReferenceSequenceExhausted,
    UniqueIntegerSequenceExhausted,
}

impl fmt::Display for VmReferenceAllocationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ReferenceSequenceExhausted => {
                formatter.write_str("VM reference sequence exhausted")
            }
            Self::UniqueIntegerSequenceExhausted => {
                formatter.write_str("VM unique-integer sequence exhausted")
            }
        }
    }
}

/// VM-owned allocator for opaque references and monotonic unique integers.
///
/// Reference identities include node and boot-epoch metadata so independently
/// running VMs cannot accidentally produce equal references. Ordinary and
/// monitor references share `next_reference_id`; unique integers intentionally
/// use their own monotonic sequence because they are values, not capabilities.
#[derive(Debug)]
pub(crate) struct VmReferenceAllocator {
    node_id: Arc<str>,
    epoch: u64,
    next_reference_id: u64,
    next_unique_integer: i64,
    reference_limit: u64,
    unique_integer_limit: i64,
}

impl VmReferenceAllocator {
    /// Creates an allocator for one explicit VM node and boot epoch.
    pub(crate) fn new(node_id: impl Into<String>, epoch: u64) -> Result<Self, String> {
        Self::with_limits(node_id, epoch, u64::MAX, i64::MAX)
    }

    /// Creates an allocator with explicit per-epoch identity ceilings.
    pub(crate) fn with_limits(
        node_id: impl Into<String>,
        epoch: u64,
        reference_limit: u64,
        unique_integer_limit: i64,
    ) -> Result<Self, String> {
        let node_id = node_id.into();
        if node_id.trim().is_empty() {
            return Err("VM reference node id must not be empty".to_string());
        }
        if epoch == 0 {
            return Err("VM reference epoch must be non-zero".to_string());
        }
        if unique_integer_limit < 0 {
            return Err("VM unique-integer limit must not be negative".to_string());
        }
        Ok(Self {
            node_id: Arc::from(node_id),
            epoch,
            next_reference_id: 0,
            next_unique_integer: 0,
            reference_limit,
            unique_integer_limit,
        })
    }

    /// Allocates the next opaque reference in this VM epoch.
    pub(crate) fn allocate_reference(
        &mut self,
    ) -> Result<VmReferenceId, VmReferenceAllocationError> {
        let local_id = self
            .next_reference_id
            .checked_add(1)
            .ok_or(VmReferenceAllocationError::ReferenceSequenceExhausted)?;
        if local_id > self.reference_limit {
            return Err(VmReferenceAllocationError::ReferenceSequenceExhausted);
        }
        self.next_reference_id = local_id;
        Ok(VmReferenceId {
            node_id: Arc::clone(&self.node_id),
            epoch: self.epoch,
            local_id,
        })
    }

    /// Allocates a positive monotonic integer unique within this VM epoch.
    pub(crate) fn allocate_unique_integer(&mut self) -> Result<i64, VmReferenceAllocationError> {
        let value = self
            .next_unique_integer
            .checked_add(1)
            .ok_or(VmReferenceAllocationError::UniqueIntegerSequenceExhausted)?;
        if value > self.unique_integer_limit {
            return Err(VmReferenceAllocationError::UniqueIntegerSequenceExhausted);
        }
        self.next_unique_integer = value;
        Ok(value)
    }
}
