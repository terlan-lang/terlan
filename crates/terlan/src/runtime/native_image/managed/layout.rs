//! Deterministic managed type and physical-layout identity.

use sha2::{Digest, Sha256};

use super::ManagedMemoryError;

/// Maximum admitted payload size for one ordinary managed object.
pub const MAX_MANAGED_OBJECT_BYTES: usize = 16 * 1024 * 1024;

/// Stable 128-bit identity of one canonical Terlan type shape.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SemanticTypeId([u8; 16]);

impl SemanticTypeId {
    /// Derives a semantic identity from a package-qualified canonical type shape.
    pub fn from_canonical(canonical: &str) -> Result<Self, ManagedMemoryError> {
        if canonical.is_empty() {
            return Err(ManagedMemoryError::EmptySemanticIdentity);
        }
        let digest = Sha256::digest(canonical.as_bytes());
        let mut identity = [0_u8; 16];
        identity.copy_from_slice(&digest[..16]);
        Ok(Self(identity))
    }

    /// Returns the canonical identity bytes used in descriptors and cache keys.
    pub fn bytes(self) -> [u8; 16] {
        self.0
    }

    /// Reconstructs an identity carried by a validated internal ABI descriptor.
    pub(crate) fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }
}

/// Target-specific 256-bit fingerprint of one physical managed layout.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LayoutFingerprint([u8; 32]);

impl LayoutFingerprint {
    /// Returns the fingerprint bytes used by image admission and native calls.
    pub fn bytes(self) -> [u8; 32] {
        self.0
    }
}

/// Runtime allocation policy selected for one physical object layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AllocationClass {
    Young,
    Large,
}

impl AllocationClass {
    /// Returns the stable byte included in the native layout fingerprint.
    fn tag(self) -> u8 {
        match self {
            Self::Young => 1,
            Self::Large => 2,
        }
    }
}

/// Canonical read-only descriptor for one managed object representation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedTypeDescriptor {
    semantic_id: SemanticTypeId,
    fingerprint: LayoutFingerprint,
    size: usize,
    alignment: usize,
    reference_offsets: Box<[usize]>,
    allocation_class: AllocationClass,
}

impl ManagedTypeDescriptor {
    /// Creates and validates a deterministic descriptor and reference map.
    pub fn new(
        semantic_id: SemanticTypeId,
        size: usize,
        alignment: usize,
        reference_offsets: Vec<usize>,
        allocation_class: AllocationClass,
    ) -> Result<Self, ManagedMemoryError> {
        Self::new_specialized(
            semantic_id,
            size,
            alignment,
            reference_offsets,
            allocation_class,
            &[],
        )
    }

    /// Creates a descriptor whose fingerprint includes canonical representation data.
    pub fn new_specialized(
        semantic_id: SemanticTypeId,
        size: usize,
        alignment: usize,
        reference_offsets: Vec<usize>,
        allocation_class: AllocationClass,
        representation: &[u8],
    ) -> Result<Self, ManagedMemoryError> {
        validate_layout(size, alignment, &reference_offsets)?;
        let fingerprint = fingerprint_layout(
            semantic_id,
            size,
            alignment,
            &reference_offsets,
            allocation_class,
            representation,
        );
        Ok(Self {
            semantic_id,
            fingerprint,
            size,
            alignment,
            reference_offsets: reference_offsets.into_boxed_slice(),
            allocation_class,
        })
    }

    /// Returns the stable semantic type identity.
    pub fn semantic_id(&self) -> SemanticTypeId {
        self.semantic_id
    }

    /// Returns the target-specific physical layout fingerprint.
    pub fn fingerprint(&self) -> LayoutFingerprint {
        self.fingerprint
    }

    /// Returns the object payload size in bytes.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Returns the required object alignment in bytes.
    pub fn alignment(&self) -> usize {
        self.alignment
    }

    /// Returns sorted payload offsets containing actor-local managed references.
    pub fn reference_offsets(&self) -> &[usize] {
        &self.reference_offsets
    }

    /// Returns the selected runtime allocation class.
    pub fn allocation_class(&self) -> AllocationClass {
        self.allocation_class
    }
}

/// Validates one object size, alignment, and precise reference map.
fn validate_layout(
    size: usize,
    alignment: usize,
    reference_offsets: &[usize],
) -> Result<(), ManagedMemoryError> {
    if size == 0 || size > MAX_MANAGED_OBJECT_BYTES {
        return Err(ManagedMemoryError::InvalidLayoutSize);
    }
    if alignment == 0 || !alignment.is_power_of_two() || alignment > 64 {
        return Err(ManagedMemoryError::InvalidLayoutAlignment);
    }
    let reference_size = std::mem::size_of::<usize>();
    let mut previous = None;
    for &offset in reference_offsets {
        if offset % reference_size != 0
            || offset
                .checked_add(reference_size)
                .is_none_or(|end| end > size)
            || previous.is_some_and(|prior| prior >= offset)
        {
            return Err(ManagedMemoryError::InvalidReferenceMap);
        }
        previous = Some(offset);
    }
    Ok(())
}

/// Computes the canonical target-layout fingerprint.
fn fingerprint_layout(
    semantic_id: SemanticTypeId,
    size: usize,
    alignment: usize,
    reference_offsets: &[usize],
    allocation_class: AllocationClass,
    representation: &[u8],
) -> LayoutFingerprint {
    let mut hasher = Sha256::new();
    hasher.update(b"terlan-managed-layout-v1\0");
    hasher.update(semantic_id.bytes());
    hasher.update((size as u64).to_le_bytes());
    hasher.update((alignment as u64).to_le_bytes());
    hasher.update([allocation_class.tag()]);
    hasher.update((reference_offsets.len() as u64).to_le_bytes());
    for offset in reference_offsets {
        hasher.update((*offset as u64).to_le_bytes());
    }
    hasher.update((representation.len() as u64).to_le_bytes());
    hasher.update(representation);
    LayoutFingerprint(hasher.finalize().into())
}
