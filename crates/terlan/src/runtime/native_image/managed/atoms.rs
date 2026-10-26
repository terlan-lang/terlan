//! Deterministic image-generation-local atom identities.

use std::fmt;
use std::ops::Range;

use super::ManagedMemoryError;

/// Zero-based immutable atom index used by one admitted native image generation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AtomIndex(u32);

impl AtomIndex {
    /// Returns the compact index carried by compiled code.
    pub fn get(self) -> u32 {
        self.0
    }

    /// Reconstructs an index already validated against its owning image atom table.
    pub(super) fn from_runtime(value: u32) -> Self {
        Self(value)
    }
}

/// Canonically ordered finite atom table owned by one native image generation.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AtomTable {
    text: Box<str>,
    ranges: Box<[Range<u32>]>,
}

impl AtomTable {
    /// Builds a deterministic table from compiler-normalized UTF-8 identities.
    pub fn new<I, S>(identities: I) -> Result<Self, ManagedMemoryError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut identities = identities
            .into_iter()
            .map(Into::into)
            .collect::<Vec<String>>();
        for identity in &identities {
            validate_atom_identity(identity)?;
        }
        identities.sort_unstable();
        identities.dedup();
        if identities.len() > u32::MAX as usize {
            return Err(ManagedMemoryError::TooManyAtoms);
        }
        let byte_count = identities.iter().try_fold(0usize, |length, identity| {
            packed_span(length, identity.len()).map(|range| range.end as usize)
        })?;
        let mut text = String::with_capacity(byte_count);
        let mut ranges = Vec::with_capacity(identities.len());
        for identity in identities {
            ranges.push(packed_span(text.len(), identity.len())?);
            text.push_str(&identity);
        }
        Ok(Self {
            text: text.into_boxed_str(),
            ranges: ranges.into_boxed_slice(),
        })
    }

    /// Returns the number of compiler-known atom identities.
    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    /// Reports whether this image generation contains no atoms.
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// Resolves canonical atom text into its generation-local index.
    pub fn index(&self, identity: &str) -> Result<AtomIndex, ManagedMemoryError> {
        validate_atom_identity(identity)?;
        self.ranges
            .binary_search_by(|range| self.text_at(range).cmp(identity))
            .map(|index| AtomIndex(index as u32))
            .map_err(|_| ManagedMemoryError::UnknownAtom)
    }

    /// Resolves one generation-local index into canonical UTF-8 identity.
    pub fn identity(&self, index: AtomIndex) -> Result<&str, ManagedMemoryError> {
        self.ranges
            .get(index.0 as usize)
            .map(|range| self.text_at(range))
            .ok_or(ManagedMemoryError::UnknownAtom)
    }

    /// Returns canonical atom identities in image order.
    pub fn identities(&self) -> impl ExactSizeIterator<Item = &str> {
        self.ranges.iter().map(|range| self.text_at(range))
    }

    /// Private ranges are constructed at UTF-8 boundaries and cannot outlive this table.
    fn text_at(&self, range: &Range<u32>) -> &str {
        &self.text[range.start as usize..range.end as usize]
    }
}

/// Keeps offset arithmetic bounded before reserving the contiguous text buffer.
fn packed_span(start: usize, length: usize) -> Result<Range<u32>, ManagedMemoryError> {
    let end = start
        .checked_add(length)
        .ok_or(ManagedMemoryError::TooManyAtoms)?;
    let start = u32::try_from(start).map_err(|_| ManagedMemoryError::TooManyAtoms)?;
    let end = u32::try_from(end).map_err(|_| ManagedMemoryError::TooManyAtoms)?;
    Ok(start..end)
}

impl fmt::Display for AtomIndex {
    /// Formats the generation-local compact index without implying global identity.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Rejects identities that cannot participate in canonical image metadata.
fn validate_atom_identity(identity: &str) -> Result<(), ManagedMemoryError> {
    if identity.is_empty() {
        return Err(ManagedMemoryError::EmptyAtomIdentity);
    }
    if identity.contains('\0') || identity.chars().any(char::is_control) {
        return Err(ManagedMemoryError::InvalidAtomIdentity);
    }
    Ok(())
}

#[cfg(test)]
#[path = "managed_atom_test.rs"]
#[cfg(test)]
mod managed_atom_test;

#[cfg(test)]
#[path = "managed_atom_packed_test.rs"]
mod packed_test;
#[cfg(test)]
#[path = "managed_atom_profile_test.rs"]
mod profile_test;
