//! Deterministic image-generation-local atom identities.

use std::fmt;

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
    identities: Box<[Box<str>]>,
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
        Ok(Self {
            identities: identities.into_iter().map(String::into_boxed_str).collect(),
        })
    }

    /// Returns the number of compiler-known atom identities.
    pub fn len(&self) -> usize {
        self.identities.len()
    }

    /// Reports whether this image generation contains no atoms.
    pub fn is_empty(&self) -> bool {
        self.identities.is_empty()
    }

    /// Resolves canonical atom text into its generation-local index.
    pub fn index(&self, identity: &str) -> Result<AtomIndex, ManagedMemoryError> {
        validate_atom_identity(identity)?;
        self.identities
            .binary_search_by(|candidate| candidate.as_ref().cmp(identity))
            .map(|index| AtomIndex(index as u32))
            .map_err(|_| ManagedMemoryError::UnknownAtom)
    }

    /// Resolves one generation-local index into canonical UTF-8 identity.
    pub fn identity(&self, index: AtomIndex) -> Result<&str, ManagedMemoryError> {
        self.identities
            .get(index.0 as usize)
            .map(Box::as_ref)
            .ok_or(ManagedMemoryError::UnknownAtom)
    }

    /// Returns canonical atom identities in image order.
    pub fn identities(&self) -> impl ExactSizeIterator<Item = &str> {
        self.identities.iter().map(Box::as_ref)
    }
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
mod managed_atom_test;
