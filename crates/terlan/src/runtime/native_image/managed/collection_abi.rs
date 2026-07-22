//! Canonical image metadata for actor-heap List, Map, and Set profiles.

use super::{
    ManagedFieldType, ManagedListDescriptor, ManagedMapDescriptor, ManagedMemoryError,
    ManagedSetDescriptor, SemanticTypeId,
};

const MAGIC: &[u8; 4] = b"TVCL";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 8;

/// Maximum encoded collection schema accepted from one native image.
pub const MAX_MANAGED_COLLECTION_ABI_BYTES: usize = 64 * 1024;

/// Closed portable collection family selected by a checked type.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedCollectionKind {
    /// Persistent adaptive RRB list.
    List,
    /// Persistent insertion-ordered adaptive A-CHAMP map.
    Map,
    /// Persistent insertion-ordered set backed by the map profile.
    Set,
}

/// Runtime collection profile reconstructed from canonical image metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
enum ManagedCollectionStorage {
    /// Checked list descriptor.
    List(ManagedListDescriptor),
    /// Checked map descriptor.
    Map(ManagedMapDescriptor),
    /// Checked set descriptor.
    Set(ManagedSetDescriptor),
}

/// Canonical checked descriptor for one materialized collection type.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedCollectionDescriptor {
    /// Canonical checked CoreIR type identity.
    canonical_type: Box<str>,
    /// Existing managed storage descriptor selected for this family.
    storage: ManagedCollectionStorage,
}

impl ManagedCollectionDescriptor {
    /// Builds a checked `List[T]` collection descriptor.
    pub fn list(
        canonical_type: &str,
        element_type: ManagedFieldType,
    ) -> Result<Self, ManagedMemoryError> {
        Ok(Self {
            canonical_type: checked_canonical(canonical_type)?,
            storage: ManagedCollectionStorage::List(ManagedListDescriptor::new(
                canonical_type,
                element_type,
            )?),
        })
    }

    /// Builds a checked `Map[K, V]` collection descriptor.
    pub fn map(
        canonical_type: &str,
        key_type: ManagedFieldType,
        value_type: ManagedFieldType,
    ) -> Result<Self, ManagedMemoryError> {
        Ok(Self {
            canonical_type: checked_canonical(canonical_type)?,
            storage: ManagedCollectionStorage::Map(ManagedMapDescriptor::new(
                canonical_type,
                key_type,
                value_type,
            )?),
        })
    }

    /// Builds a checked `Set[T]` collection descriptor.
    pub fn set(
        canonical_type: &str,
        element_type: ManagedFieldType,
    ) -> Result<Self, ManagedMemoryError> {
        Ok(Self {
            canonical_type: checked_canonical(canonical_type)?,
            storage: ManagedCollectionStorage::Set(ManagedSetDescriptor::new(
                canonical_type,
                element_type,
            )?),
        })
    }

    /// Returns the package-qualified canonical type representation.
    pub fn canonical_type(&self) -> &str {
        &self.canonical_type
    }

    /// Returns the closed collection family.
    pub fn kind(&self) -> ManagedCollectionKind {
        match self.storage {
            ManagedCollectionStorage::List(_) => ManagedCollectionKind::List,
            ManagedCollectionStorage::Map(_) => ManagedCollectionKind::Map,
            ManagedCollectionStorage::Set(_) => ManagedCollectionKind::Set,
        }
    }

    /// Returns the root semantic identity shared by all physical profiles.
    pub fn semantic_id(&self) -> SemanticTypeId {
        match &self.storage {
            ManagedCollectionStorage::List(descriptor) => descriptor.semantic_id(),
            ManagedCollectionStorage::Map(descriptor) => descriptor.semantic_id(),
            ManagedCollectionStorage::Set(descriptor) => descriptor.semantic_id(),
        }
    }

    /// Returns this descriptor as a list profile when its family matches.
    pub fn list_descriptor(&self) -> Option<&ManagedListDescriptor> {
        match &self.storage {
            ManagedCollectionStorage::List(descriptor) => Some(descriptor),
            _ => None,
        }
    }

    /// Returns this descriptor as a map profile when its family matches.
    pub fn map_descriptor(&self) -> Option<&ManagedMapDescriptor> {
        match &self.storage {
            ManagedCollectionStorage::Map(descriptor) => Some(descriptor),
            _ => None,
        }
    }

    /// Returns this descriptor as a set profile when its family matches.
    pub fn set_descriptor(&self) -> Option<&ManagedSetDescriptor> {
        match &self.storage {
            ManagedCollectionStorage::Set(descriptor) => Some(descriptor),
            _ => None,
        }
    }

    /// Returns the canonical ordered field categories encoded by this profile.
    fn field_types(&self) -> Vec<ManagedFieldType> {
        match &self.storage {
            ManagedCollectionStorage::List(descriptor) => vec![descriptor.element_type()],
            ManagedCollectionStorage::Map(descriptor) => {
                vec![descriptor.key_type(), descriptor.value_type()]
            }
            ManagedCollectionStorage::Set(descriptor) => vec![descriptor.element_type()],
        }
    }
}

/// Encodes one collection descriptor into deterministic bounded image bytes.
pub fn encode_collection_layout(
    descriptor: &ManagedCollectionDescriptor,
) -> Result<Vec<u8>, ManagedMemoryError> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.push(match descriptor.kind() {
        ManagedCollectionKind::List => 1,
        ManagedCollectionKind::Map => 2,
        ManagedCollectionKind::Set => 3,
    });
    bytes.push(0);
    push_text(&mut bytes, descriptor.canonical_type())?;
    let fields = descriptor.field_types();
    bytes.push(fields.len() as u8);
    for field in fields {
        field.encode(&mut bytes);
    }
    if bytes.len() > MAX_MANAGED_COLLECTION_ABI_BYTES {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    Ok(bytes)
}

/// Decodes and reconstructs one canonical checked collection descriptor.
pub fn decode_collection_layout(
    bytes: &[u8],
) -> Result<ManagedCollectionDescriptor, ManagedMemoryError> {
    if bytes.len() > MAX_MANAGED_COLLECTION_ABI_BYTES {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    let mut input = CollectionAbiInput::new(bytes)?;
    let kind = input.header()?;
    let canonical = input.text()?;
    let count = input.u8()? as usize;
    let fields = (0..count)
        .map(|_| input.field_type())
        .collect::<Result<Vec<_>, _>>()?;
    input.finish()?;
    match (kind, fields.as_slice()) {
        (ManagedCollectionKind::List, [element]) => {
            ManagedCollectionDescriptor::list(canonical, *element)
        }
        (ManagedCollectionKind::Map, [key, value]) => {
            ManagedCollectionDescriptor::map(canonical, *key, *value)
        }
        (ManagedCollectionKind::Set, [element]) => {
            ManagedCollectionDescriptor::set(canonical, *element)
        }
        _ => Err(ManagedMemoryError::InvalidAggregateAbi),
    }
}

/// Rejects empty canonical collection identities before descriptor construction.
fn checked_canonical(canonical: &str) -> Result<Box<str>, ManagedMemoryError> {
    if canonical.is_empty() {
        Err(ManagedMemoryError::InvalidAggregateShape)
    } else {
        Ok(canonical.to_owned().into_boxed_str())
    }
}

/// Appends one bounded canonical UTF-8 identity.
fn push_text(bytes: &mut Vec<u8>, value: &str) -> Result<(), ManagedMemoryError> {
    let length = u32::try_from(value.len()).map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    bytes.extend_from_slice(&length.to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

/// Checked cursor over one untrusted collection schema.
struct CollectionAbiInput<'a> {
    /// Complete bounded schema bytes.
    bytes: &'a [u8],
    /// Current checked decode offset.
    offset: usize,
}

impl<'a> CollectionAbiInput<'a> {
    /// Creates a decoder after enforcing the fixed header bound.
    fn new(bytes: &'a [u8]) -> Result<Self, ManagedMemoryError> {
        (bytes.len() >= HEADER_BYTES)
            .then_some(Self { bytes, offset: 0 })
            .ok_or(ManagedMemoryError::InvalidAggregateAbi)
    }

    /// Reads magic, version, kind, and reserved bits.
    fn header(&mut self) -> Result<ManagedCollectionKind, ManagedMemoryError> {
        if self.take(4)? != MAGIC || self.u16()? != VERSION {
            return Err(ManagedMemoryError::InvalidAggregateAbi);
        }
        let kind = match self.u8()? {
            1 => ManagedCollectionKind::List,
            2 => ManagedCollectionKind::Map,
            3 => ManagedCollectionKind::Set,
            _ => return Err(ManagedMemoryError::InvalidAggregateAbi),
        };
        if self.u8()? != 0 {
            return Err(ManagedMemoryError::InvalidAggregateAbi);
        }
        Ok(kind)
    }

    /// Reads one checked managed field category.
    fn field_type(&mut self) -> Result<ManagedFieldType, ManagedMemoryError> {
        match self.u8()? {
            0 => Ok(ManagedFieldType::Unit),
            1 => Ok(ManagedFieldType::Bool),
            2 => Ok(ManagedFieldType::Int),
            3 => Ok(ManagedFieldType::Float),
            4 => Ok(ManagedFieldType::Atom),
            5 => Ok(ManagedFieldType::Reference(SemanticTypeId::from_bytes(
                self.array()?,
            ))),
            _ => Err(ManagedMemoryError::InvalidAggregateAbi),
        }
    }

    /// Reads one bounded UTF-8 canonical identity.
    fn text(&mut self) -> Result<&'a str, ManagedMemoryError> {
        let length = self.u32()? as usize;
        let bytes = self.take(length)?;
        let text =
            std::str::from_utf8(bytes).map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
        (!text.is_empty())
            .then_some(text)
            .ok_or(ManagedMemoryError::InvalidAggregateAbi)
    }

    /// Reads one byte.
    fn u8(&mut self) -> Result<u8, ManagedMemoryError> {
        Ok(self.take(1)?[0])
    }

    /// Reads one little-endian `u16`.
    fn u16(&mut self) -> Result<u16, ManagedMemoryError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    /// Reads one little-endian `u32`.
    fn u32(&mut self) -> Result<u32, ManagedMemoryError> {
        Ok(u32::from_le_bytes(self.array()?))
    }

    /// Reads one fixed-width byte array.
    fn array<const N: usize>(&mut self) -> Result<[u8; N], ManagedMemoryError> {
        self.take(N)?
            .try_into()
            .map_err(|_| ManagedMemoryError::InvalidAggregateAbi)
    }

    /// Reads one checked byte range.
    fn take(&mut self, length: usize) -> Result<&'a [u8], ManagedMemoryError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or(ManagedMemoryError::InvalidAggregateAbi)?;
        let value = self
            .bytes
            .get(self.offset..end)
            .ok_or(ManagedMemoryError::InvalidAggregateAbi)?;
        self.offset = end;
        Ok(value)
    }

    /// Rejects trailing bytes outside the canonical schema.
    fn finish(self) -> Result<(), ManagedMemoryError> {
        (self.offset == self.bytes.len())
            .then_some(())
            .ok_or(ManagedMemoryError::InvalidAggregateAbi)
    }
}

#[cfg(test)]
#[path = "collection_abi_test.rs"]
mod collection_abi_test;
