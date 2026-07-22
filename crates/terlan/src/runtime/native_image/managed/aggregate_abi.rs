//! Bounded aggregate-layout contract between native code and actor heaps.

use std::num::NonZeroUsize;
use std::sync::Arc;

use super::{
    ActorHeap, AtomIndex, ManagedAggregate, ManagedAggregateDescriptor, ManagedAggregateKind,
    ManagedFieldType, ManagedFieldValue, ManagedMemoryError, SemanticTypeId, TvmRef,
};

const MAGIC: &[u8; 4] = b"TVMA";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 8;

/// Maximum encoded aggregate descriptor accepted from one native image call.
pub const MAX_MANAGED_AGGREGATE_ABI_BYTES: usize = 64 * 1024;

/// Native status returned when the actor heap rejects a managed allocation call.
pub const MANAGED_ALLOCATION_FAILED_STATUS: i32 = 22;

/// Encodes one canonical aggregate descriptor for a native allocation call.
pub fn encode_aggregate_layout(
    descriptor: &ManagedAggregateDescriptor,
) -> Result<Vec<u8>, ManagedMemoryError> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&VERSION.to_le_bytes());
    bytes.push(kind_tag(descriptor.kind()));
    bytes.push(0);
    push_text(&mut bytes, descriptor.canonical_type())?;
    if descriptor.kind() == ManagedAggregateKind::Constructor {
        push_text(
            &mut bytes,
            descriptor
                .variant_name()
                .ok_or(ManagedMemoryError::InvalidAggregateAbi)?,
        )?;
        bytes.extend_from_slice(
            &descriptor
                .discriminant()
                .ok_or(ManagedMemoryError::InvalidAggregateAbi)?
                .to_le_bytes(),
        );
        bytes.extend_from_slice(
            &descriptor
                .variant_count()
                .ok_or(ManagedMemoryError::InvalidAggregateAbi)?
                .to_le_bytes(),
        );
    }
    let field_count = u32::try_from(descriptor.fields().len())
        .map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    bytes.extend_from_slice(&field_count.to_le_bytes());
    for field in descriptor.fields() {
        bytes.push(u8::from(field.name().is_some()));
        if let Some(name) = field.name() {
            push_text(&mut bytes, name)?;
        }
        push_field_type(&mut bytes, field.field_type());
    }
    if bytes.len() > MAX_MANAGED_AGGREGATE_ABI_BYTES {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    Ok(bytes)
}

/// Decodes and fully revalidates one native aggregate allocation descriptor.
pub fn decode_aggregate_layout(
    bytes: &[u8],
) -> Result<ManagedAggregateDescriptor, ManagedMemoryError> {
    if bytes.len() > MAX_MANAGED_AGGREGATE_ABI_BYTES {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    let mut input = AggregateAbiInput::new(bytes)?;
    let kind = input.header()?;
    let canonical = input.text()?;
    let variant = if kind == ManagedAggregateKind::Constructor {
        Some((input.text()?, input.u32()?, input.u32()?))
    } else {
        None
    };
    let count = input.u32()? as usize;
    if count > MAX_MANAGED_AGGREGATE_ABI_BYTES / 2 {
        return Err(ManagedMemoryError::InvalidAggregateAbi);
    }
    let mut fields = Vec::with_capacity(count);
    for _ in 0..count {
        let name = match input.u8()? {
            0 => None,
            1 => Some(input.text()?.to_owned()),
            _ => return Err(ManagedMemoryError::InvalidAggregateAbi),
        };
        fields.push((name, input.field_type()?));
    }
    input.finish()?;
    build_descriptor(kind, canonical, variant, fields)
}

impl ActorHeap {
    /// Decodes a bounded native descriptor and allocates only in this actor's heap.
    pub fn allocate_aggregate_abi(
        &mut self,
        encoded_layout: &[u8],
        values: &[ManagedFieldValue],
    ) -> Result<(TvmRef<ManagedAggregate>, Arc<ManagedAggregateDescriptor>), ManagedMemoryError>
    {
        let descriptor = Arc::new(decode_aggregate_layout(encoded_layout)?);
        let value = self.allocate_aggregate(descriptor.clone(), values)?;
        Ok((value, descriptor))
    }

    /// Decodes native field words and returns one opaque actor-local reference word.
    pub(crate) fn allocate_aggregate_words_abi(
        &mut self,
        encoded_layout: &[u8],
        words: &[i64],
    ) -> Result<(u64, Arc<ManagedAggregateDescriptor>), ManagedMemoryError> {
        let descriptor = Arc::new(decode_aggregate_layout(encoded_layout)?);
        if words.len() != descriptor.fields().len() {
            return Err(ManagedMemoryError::InvalidAggregateArity);
        }
        let values = descriptor
            .fields()
            .iter()
            .zip(words)
            .map(|(field, word)| decode_field_word(field.field_type(), *word))
            .collect::<Result<Vec<_>, _>>()?;
        let reference = self.allocate_aggregate(descriptor.clone(), &values)?;
        let encoded = u64::try_from(reference.encoded().get())
            .map_err(|_| ManagedMemoryError::UnsupportedPointerWidth)?;
        Ok((encoded, descriptor))
    }
}

/// Decodes one pointer-width field word according to its closed descriptor kind.
fn decode_field_word(
    field_type: ManagedFieldType,
    word: i64,
) -> Result<ManagedFieldValue, ManagedMemoryError> {
    match field_type {
        ManagedFieldType::Unit if word == 0 => Ok(ManagedFieldValue::Unit),
        ManagedFieldType::Unit => Err(ManagedMemoryError::InvalidManagedScalar),
        ManagedFieldType::Bool => match word {
            0 => Ok(ManagedFieldValue::Bool(false)),
            1 => Ok(ManagedFieldValue::Bool(true)),
            _ => Err(ManagedMemoryError::InvalidManagedScalar),
        },
        ManagedFieldType::Int => Ok(ManagedFieldValue::Int(word)),
        ManagedFieldType::Float => {
            let value = f64::from_bits(word as u64);
            value
                .is_finite()
                .then_some(ManagedFieldValue::Float(value))
                .ok_or(ManagedMemoryError::InvalidManagedScalar)
        }
        ManagedFieldType::Atom => u32::try_from(word)
            .map(AtomIndex::from_runtime)
            .map(ManagedFieldValue::Atom)
            .map_err(|_| ManagedMemoryError::InvalidManagedScalar),
        ManagedFieldType::Reference(_) => {
            let encoded = usize::try_from(word as u64)
                .ok()
                .and_then(NonZeroUsize::new)
                .ok_or(ManagedMemoryError::InvalidManagedScalar)?;
            Ok(ManagedFieldValue::Reference(TvmRef::from_encoded(encoded)))
        }
    }
}

/// Reconstructs the canonical descriptor through its existing checked builders.
fn build_descriptor(
    kind: ManagedAggregateKind,
    canonical: &str,
    variant: Option<(&str, u32, u32)>,
    fields: Vec<(Option<String>, ManagedFieldType)>,
) -> Result<ManagedAggregateDescriptor, ManagedMemoryError> {
    let result = match kind {
        ManagedAggregateKind::Tuple if fields.iter().all(|(name, _)| name.is_none()) => {
            ManagedAggregateDescriptor::tuple(
                canonical,
                fields.into_iter().map(|(_, field)| field).collect(),
            )
        }
        ManagedAggregateKind::FixedArray
            if !fields.is_empty()
                && fields
                    .iter()
                    .all(|(name, field)| name.is_none() && *field == fields[0].1) =>
        {
            ManagedAggregateDescriptor::fixed_array(canonical, fields[0].1, fields.len())
        }
        ManagedAggregateKind::Record if fields.iter().all(|(name, _)| name.is_some()) => {
            let mut named_fields = Vec::with_capacity(fields.len());
            for (name, field) in fields {
                let Some(name) = name else {
                    return Err(ManagedMemoryError::InvalidAggregateAbi);
                };
                named_fields.push((name, field));
            }
            ManagedAggregateDescriptor::record(canonical, named_fields)
        }
        ManagedAggregateKind::Constructor => {
            let Some((name, discriminant, variant_count)) = variant else {
                return Err(ManagedMemoryError::InvalidAggregateAbi);
            };
            ManagedAggregateDescriptor::constructor(
                canonical,
                name,
                discriminant,
                variant_count,
                fields,
            )
        }
        _ => return Err(ManagedMemoryError::InvalidAggregateAbi),
    };
    result.map_err(|_| ManagedMemoryError::InvalidAggregateAbi)
}

/// Returns the stable ABI tag for one aggregate family.
fn kind_tag(kind: ManagedAggregateKind) -> u8 {
    match kind {
        ManagedAggregateKind::Tuple => 1,
        ManagedAggregateKind::FixedArray => 2,
        ManagedAggregateKind::Record => 3,
        ManagedAggregateKind::Constructor => 4,
    }
}

/// Appends one bounded UTF-8 string.
fn push_text(bytes: &mut Vec<u8>, value: &str) -> Result<(), ManagedMemoryError> {
    let length = u32::try_from(value.len()).map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
    bytes.extend_from_slice(&length.to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

/// Appends one closed field-kind descriptor.
fn push_field_type(bytes: &mut Vec<u8>, field: ManagedFieldType) {
    bytes.push(match field {
        ManagedFieldType::Unit => 0,
        ManagedFieldType::Bool => 1,
        ManagedFieldType::Int => 2,
        ManagedFieldType::Float => 3,
        ManagedFieldType::Atom => 4,
        ManagedFieldType::Reference(_) => 5,
    });
    if let ManagedFieldType::Reference(semantic) = field {
        bytes.extend_from_slice(&semantic.bytes());
    }
}

/// Checked cursor over one untrusted aggregate ABI descriptor.
struct AggregateAbiInput<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> AggregateAbiInput<'a> {
    /// Creates a cursor after enforcing the global descriptor bound.
    fn new(bytes: &'a [u8]) -> Result<Self, ManagedMemoryError> {
        (bytes.len() >= HEADER_BYTES)
            .then_some(Self { bytes, offset: 0 })
            .ok_or(ManagedMemoryError::InvalidAggregateAbi)
    }

    /// Parses the fixed magic, version, aggregate kind, and reserved byte.
    fn header(&mut self) -> Result<ManagedAggregateKind, ManagedMemoryError> {
        if self.take(4)? != MAGIC || self.u16()? != VERSION {
            return Err(ManagedMemoryError::InvalidAggregateAbi);
        }
        let kind = match self.u8()? {
            1 => ManagedAggregateKind::Tuple,
            2 => ManagedAggregateKind::FixedArray,
            3 => ManagedAggregateKind::Record,
            4 => ManagedAggregateKind::Constructor,
            _ => return Err(ManagedMemoryError::InvalidAggregateAbi),
        };
        if self.u8()? != 0 {
            return Err(ManagedMemoryError::InvalidAggregateAbi);
        }
        Ok(kind)
    }

    /// Reads one bounded UTF-8 string.
    fn text(&mut self) -> Result<&'a str, ManagedMemoryError> {
        let length = self.u32()? as usize;
        std::str::from_utf8(self.take(length)?).map_err(|_| ManagedMemoryError::InvalidAggregateAbi)
    }

    /// Reads one closed field-kind descriptor.
    fn field_type(&mut self) -> Result<ManagedFieldType, ManagedMemoryError> {
        Ok(match self.u8()? {
            0 => ManagedFieldType::Unit,
            1 => ManagedFieldType::Bool,
            2 => ManagedFieldType::Int,
            3 => ManagedFieldType::Float,
            4 => ManagedFieldType::Atom,
            5 => {
                let identity = self
                    .take(16)?
                    .try_into()
                    .map_err(|_| ManagedMemoryError::InvalidAggregateAbi)?;
                ManagedFieldType::Reference(SemanticTypeId::from_bytes(identity))
            }
            _ => return Err(ManagedMemoryError::InvalidAggregateAbi),
        })
    }

    /// Reads one byte.
    fn u8(&mut self) -> Result<u8, ManagedMemoryError> {
        Ok(self.take(1)?[0])
    }

    /// Reads one little-endian `u16`.
    fn u16(&mut self) -> Result<u16, ManagedMemoryError> {
        self.take(2)?
            .try_into()
            .map(u16::from_le_bytes)
            .map_err(|_| ManagedMemoryError::InvalidAggregateAbi)
    }

    /// Reads one little-endian `u32`.
    fn u32(&mut self) -> Result<u32, ManagedMemoryError> {
        self.take(4)?
            .try_into()
            .map(u32::from_le_bytes)
            .map_err(|_| ManagedMemoryError::InvalidAggregateAbi)
    }

    /// Returns one checked byte range and advances the cursor.
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

    /// Rejects trailing bytes that could create ambiguous descriptor identities.
    fn finish(self) -> Result<(), ManagedMemoryError> {
        (self.offset == self.bytes.len())
            .then_some(())
            .ok_or(ManagedMemoryError::InvalidAggregateAbi)
    }
}

#[cfg(test)]
#[path = "aggregate_abi_test.rs"]
mod aggregate_abi_test;
