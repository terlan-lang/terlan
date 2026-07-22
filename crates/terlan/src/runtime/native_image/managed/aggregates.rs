//! Fixed managed products and algebraic constructor values.

use std::sync::Arc;

use super::{
    ActorHeap, AllocationClass, AtomIndex, ManagedMemoryError, ManagedTypeDescriptor,
    SemanticTypeId, TvmRef,
};

const VARIANT_TAG_BYTES: usize = std::mem::size_of::<u32>();

/// Compile-time marker for one fixed managed aggregate object.
#[derive(Debug)]
pub struct ManagedAggregate;

/// Product representation selected for a non-variant aggregate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedProductKind {
    Tuple,
    FixedArray,
    Record,
}

/// Closed aggregate shape carried by the managed allocation ABI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedAggregateKind {
    /// Ordered heterogeneous positional product.
    Tuple,
    /// Ordered homogeneous fixed-length product.
    FixedArray,
    /// Ordered product whose fields retain source identities.
    Record,
    /// One active variant in a finite algebraic union.
    Constructor,
}

/// Physical value category of one materialized aggregate field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedFieldType {
    Unit,
    Bool,
    Int,
    Float,
    Atom,
    Reference(SemanticTypeId),
}

impl ManagedFieldType {
    /// Returns the field's materialized size and target alignment.
    pub(super) fn layout(self) -> (usize, usize) {
        match self {
            Self::Unit => (0, 1),
            Self::Bool => (1, 1),
            Self::Atom => (4, 4),
            Self::Int | Self::Float | Self::Reference(_) => (8, 8),
        }
    }

    /// Appends this field category to canonical representation bytes.
    pub(super) fn encode(self, bytes: &mut Vec<u8>) {
        match self {
            Self::Unit => bytes.push(0),
            Self::Bool => bytes.push(1),
            Self::Int => bytes.push(2),
            Self::Float => bytes.push(3),
            Self::Atom => bytes.push(4),
            Self::Reference(semantic) => {
                bytes.push(5);
                bytes.extend_from_slice(&semantic.bytes());
            }
        }
    }
}

/// Typed field value accepted by managed aggregate allocation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ManagedFieldValue {
    Unit,
    Bool(bool),
    Int(i64),
    Float(f64),
    Atom(AtomIndex),
    Reference(TvmRef<()>),
}

/// One ordered source field and its computed physical location.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedFieldDescriptor {
    name: Option<Box<str>>,
    field_type: ManagedFieldType,
    offset: usize,
}

impl ManagedFieldDescriptor {
    /// Returns the optional source field identity retained by records and constructors.
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the field's closed physical value category.
    pub fn field_type(&self) -> ManagedFieldType {
        self.field_type
    }

    /// Returns the byte offset in the managed object payload.
    pub fn offset(&self) -> usize {
        self.offset
    }
}

/// Canonical descriptor for one product or one active algebraic variant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedAggregateDescriptor {
    canonical_type: Box<str>,
    kind: ManagedAggregateKind,
    variant_name: Option<Box<str>>,
    managed: Arc<ManagedTypeDescriptor>,
    fields: Box<[ManagedFieldDescriptor]>,
    discriminant: Option<u32>,
    variant_count: Option<u32>,
}

impl ManagedAggregateDescriptor {
    /// Builds an ordered tuple descriptor.
    pub fn tuple(
        canonical_type: &str,
        fields: Vec<ManagedFieldType>,
    ) -> Result<Self, ManagedMemoryError> {
        Self::product(canonical_type, ManagedProductKind::Tuple, fields)
    }

    /// Builds a homogeneous fixed-array descriptor.
    pub fn fixed_array(
        canonical_type: &str,
        element: ManagedFieldType,
        length: usize,
    ) -> Result<Self, ManagedMemoryError> {
        if length == 0 {
            return Err(ManagedMemoryError::InvalidAggregateShape);
        }
        Self::product(
            canonical_type,
            ManagedProductKind::FixedArray,
            vec![element; length],
        )
    }

    /// Builds an ordered record descriptor with unique nonempty field names.
    pub fn record(
        canonical_type: &str,
        fields: Vec<(String, ManagedFieldType)>,
    ) -> Result<Self, ManagedMemoryError> {
        validate_names(fields.iter().map(|(name, _)| name.as_str()))?;
        let names = fields
            .iter()
            .map(|(name, _)| Some(name.clone().into_boxed_str()))
            .collect::<Vec<_>>();
        let types = fields.into_iter().map(|(_, ty)| ty).collect::<Vec<_>>();
        Self::build(canonical_type, 3, names, types, None)
    }

    /// Builds one active constructor descriptor for a finite algebraic union.
    pub fn constructor(
        canonical_type: &str,
        variant_name: &str,
        discriminant: u32,
        variant_count: u32,
        fields: Vec<(Option<String>, ManagedFieldType)>,
    ) -> Result<Self, ManagedMemoryError> {
        if variant_name.is_empty() || variant_count == 0 || discriminant >= variant_count {
            return Err(ManagedMemoryError::InvalidVariantDiscriminant);
        }
        let present_names = fields
            .iter()
            .filter_map(|(name, _)| name.as_deref())
            .collect::<Vec<_>>();
        validate_names(present_names.iter().copied())?;
        let names = fields
            .iter()
            .map(|(name, _)| name.clone().map(String::into_boxed_str))
            .collect::<Vec<_>>();
        let types = fields.into_iter().map(|(_, ty)| ty).collect::<Vec<_>>();
        Self::build(
            canonical_type,
            4,
            names,
            types,
            Some((variant_name, discriminant, variant_count)),
        )
    }

    /// Returns the underlying heap descriptor and precise reference map.
    pub fn managed(&self) -> &Arc<ManagedTypeDescriptor> {
        &self.managed
    }

    /// Returns the package-qualified canonical semantic type.
    pub fn canonical_type(&self) -> &str {
        &self.canonical_type
    }

    /// Returns the closed physical aggregate family.
    pub fn kind(&self) -> ManagedAggregateKind {
        self.kind
    }

    /// Returns the active constructor name for a variant descriptor.
    pub fn variant_name(&self) -> Option<&str> {
        self.variant_name.as_deref()
    }

    /// Returns ordered source fields and computed physical offsets.
    pub fn fields(&self) -> &[ManagedFieldDescriptor] {
        &self.fields
    }

    /// Returns the active semantic discriminant for a constructor.
    pub fn discriminant(&self) -> Option<u32> {
        self.discriminant
    }

    /// Returns the finite union cardinality for a constructor.
    pub fn variant_count(&self) -> Option<u32> {
        self.variant_count
    }

    /// Builds a non-variant product descriptor.
    fn product(
        canonical_type: &str,
        kind: ManagedProductKind,
        fields: Vec<ManagedFieldType>,
    ) -> Result<Self, ManagedMemoryError> {
        if fields.is_empty() {
            return Err(ManagedMemoryError::InvalidAggregateShape);
        }
        let tag = match kind {
            ManagedProductKind::Tuple => 1,
            ManagedProductKind::FixedArray => 2,
            ManagedProductKind::Record => 3,
        };
        Self::build(canonical_type, tag, vec![None; fields.len()], fields, None)
    }

    /// Computes target layout and its canonical representation fingerprint input.
    fn build(
        canonical_type: &str,
        kind_tag: u8,
        names: Vec<Option<Box<str>>>,
        fields: Vec<ManagedFieldType>,
        variant: Option<(&str, u32, u32)>,
    ) -> Result<Self, ManagedMemoryError> {
        if names.len() != fields.len() {
            return Err(ManagedMemoryError::InvalidAggregateShape);
        }
        let mut cursor = if variant.is_some() {
            VARIANT_TAG_BYTES
        } else {
            0
        };
        let mut alignment = if variant.is_some() { 4 } else { 1 };
        let mut reference_offsets = Vec::new();
        let mut descriptors = Vec::with_capacity(fields.len());
        let mut representation = vec![kind_tag];
        if let Some((name, discriminant, count)) = variant {
            representation.extend_from_slice(&discriminant.to_le_bytes());
            representation.extend_from_slice(&count.to_le_bytes());
            encode_text(&mut representation, name)?;
        }
        representation.extend_from_slice(&(fields.len() as u64).to_le_bytes());
        for (name, field_type) in names.into_iter().zip(fields) {
            let (size, field_alignment) = field_type.layout();
            cursor = align_up(cursor, field_alignment)?;
            if matches!(field_type, ManagedFieldType::Reference(_)) {
                reference_offsets.push(cursor);
            }
            representation.push(u8::from(name.is_some()));
            if let Some(name) = name.as_deref() {
                encode_text(&mut representation, name)?;
            }
            field_type.encode(&mut representation);
            representation.extend_from_slice(&(cursor as u64).to_le_bytes());
            descriptors.push(ManagedFieldDescriptor {
                name,
                field_type,
                offset: cursor,
            });
            cursor = cursor
                .checked_add(size)
                .ok_or(ManagedMemoryError::InvalidAggregateShape)?;
            alignment = alignment.max(field_alignment);
        }
        let size = align_up(cursor.max(1), alignment)?;
        let semantic = SemanticTypeId::from_canonical(canonical_type)?;
        let managed = ManagedTypeDescriptor::new_specialized(
            semantic,
            size,
            alignment,
            reference_offsets,
            AllocationClass::Young,
            &representation,
        )?;
        let kind = match kind_tag {
            1 => ManagedAggregateKind::Tuple,
            2 => ManagedAggregateKind::FixedArray,
            3 => ManagedAggregateKind::Record,
            4 => ManagedAggregateKind::Constructor,
            _ => return Err(ManagedMemoryError::InvalidAggregateShape),
        };
        Ok(Self {
            canonical_type: canonical_type.to_owned().into_boxed_str(),
            kind,
            variant_name: variant.map(|(name, _, _)| name.to_owned().into_boxed_str()),
            managed: Arc::new(managed),
            fields: descriptors.into_boxed_slice(),
            discriminant: variant.map(|(_, discriminant, _)| discriminant),
            variant_count: variant.map(|(_, _, count)| count),
        })
    }
}

/// Borrowed typed view over one validated aggregate object.
#[derive(Debug)]
pub struct ManagedAggregateView<'a> {
    heap: &'a ActorHeap,
    value: TvmRef<ManagedAggregate>,
    descriptor: &'a ManagedAggregateDescriptor,
    payload: &'a [u8],
}

impl ManagedAggregateView<'_> {
    /// Returns the active constructor discriminant, when this is a variant.
    pub fn discriminant(&self) -> Option<u32> {
        self.descriptor.discriminant
    }

    /// Returns one field after checked decoding against its physical category.
    pub fn field(&self, index: usize) -> Result<ManagedFieldValue, ManagedMemoryError> {
        let field = self
            .descriptor
            .fields
            .get(index)
            .ok_or(ManagedMemoryError::InvalidAggregateField)?;
        decode_typed_slot(
            self.heap,
            self.value.erase(),
            self.payload,
            field.offset,
            field.field_type,
        )
    }
}

impl ActorHeap {
    /// Allocates one immutable fixed aggregate with exact typed field validation.
    pub fn allocate_aggregate(
        &mut self,
        descriptor: Arc<ManagedAggregateDescriptor>,
        values: &[ManagedFieldValue],
    ) -> Result<TvmRef<ManagedAggregate>, ManagedMemoryError> {
        if values.len() != descriptor.fields.len() {
            return Err(ManagedMemoryError::InvalidAggregateArity);
        }
        let mut payload = vec![0_u8; descriptor.managed.size()];
        if let Some(discriminant) = descriptor.discriminant {
            payload[..VARIANT_TAG_BYTES].copy_from_slice(&discriminant.to_le_bytes());
        }
        let mut references = Vec::new();
        for (field, value) in descriptor.fields.iter().zip(values) {
            encode_typed_slot(
                self,
                &mut payload,
                field.offset,
                field.field_type,
                *value,
                &mut references,
            )?;
        }
        self.allocate(descriptor.managed.clone(), &payload, &references)
    }

    /// Opens a typed immutable view after descriptor and discriminant validation.
    pub fn read_aggregate<'a>(
        &'a self,
        value: TvmRef<ManagedAggregate>,
        descriptor: &'a ManagedAggregateDescriptor,
    ) -> Result<ManagedAggregateView<'a>, ManagedMemoryError> {
        if self.descriptor(value)?.fingerprint() != descriptor.managed.fingerprint() {
            return Err(ManagedMemoryError::ManagedTypeMismatch);
        }
        let payload = self.read(value)?;
        if let Some(discriminant) = descriptor.discriminant {
            if read_u32(payload, 0)? != discriminant {
                return Err(ManagedMemoryError::InvalidVariantDiscriminant);
            }
        }
        Ok(ManagedAggregateView {
            heap: self,
            value,
            descriptor,
            payload,
        })
    }
}

/// Encodes and validates one aggregate field.
pub(super) fn encode_typed_slot(
    heap: &ActorHeap,
    payload: &mut [u8],
    offset: usize,
    field_type: ManagedFieldType,
    value: ManagedFieldValue,
    references: &mut Vec<(usize, TvmRef<()>)>,
) -> Result<(), ManagedMemoryError> {
    validate_typed_value(heap, field_type, value)?;
    match (field_type, value) {
        (ManagedFieldType::Unit, ManagedFieldValue::Unit) => Ok(()),
        (ManagedFieldType::Bool, ManagedFieldValue::Bool(value)) => {
            write(payload, offset, &[u8::from(value)])
        }
        (ManagedFieldType::Int, ManagedFieldValue::Int(value)) => {
            write(payload, offset, &value.to_le_bytes())
        }
        (ManagedFieldType::Float, ManagedFieldValue::Float(value)) => {
            write(payload, offset, &value.to_bits().to_le_bytes())
        }
        (ManagedFieldType::Atom, ManagedFieldValue::Atom(value)) => {
            write(payload, offset, &value.get().to_le_bytes())
        }
        (ManagedFieldType::Reference(_), ManagedFieldValue::Reference(value)) => {
            references.push((offset, value));
            Ok(())
        }
        _ => Err(ManagedMemoryError::InvalidAggregateField),
    }
}

/// Validates one typed value without allocating or writing an object slot.
pub(super) fn validate_typed_value(
    heap: &ActorHeap,
    field_type: ManagedFieldType,
    value: ManagedFieldValue,
) -> Result<(), ManagedMemoryError> {
    match (field_type, value) {
        (ManagedFieldType::Unit, ManagedFieldValue::Unit)
        | (ManagedFieldType::Bool, ManagedFieldValue::Bool(_))
        | (ManagedFieldType::Int, ManagedFieldValue::Int(_))
        | (ManagedFieldType::Atom, ManagedFieldValue::Atom(_)) => Ok(()),
        (ManagedFieldType::Float, ManagedFieldValue::Float(value)) if value.is_finite() => Ok(()),
        (ManagedFieldType::Float, ManagedFieldValue::Float(_)) => {
            Err(ManagedMemoryError::InvalidManagedScalar)
        }
        (ManagedFieldType::Reference(expected), ManagedFieldValue::Reference(value)) => {
            if heap.descriptor(value)?.semantic_id() == expected {
                Ok(())
            } else {
                Err(ManagedMemoryError::InvalidAggregateField)
            }
        }
        _ => Err(ManagedMemoryError::InvalidAggregateField),
    }
}

/// Decodes one field from a previously validated aggregate payload.
pub(super) fn decode_typed_slot(
    heap: &ActorHeap,
    object: TvmRef<()>,
    payload: &[u8],
    offset: usize,
    field_type: ManagedFieldType,
) -> Result<ManagedFieldValue, ManagedMemoryError> {
    match field_type {
        ManagedFieldType::Unit => Ok(ManagedFieldValue::Unit),
        ManagedFieldType::Bool => match read(payload, offset, 1)?[0] {
            0 => Ok(ManagedFieldValue::Bool(false)),
            1 => Ok(ManagedFieldValue::Bool(true)),
            _ => Err(ManagedMemoryError::InvalidManagedScalar),
        },
        ManagedFieldType::Int => Ok(ManagedFieldValue::Int(i64::from_le_bytes(read_array(
            payload, offset,
        )?))),
        ManagedFieldType::Float => {
            let value = f64::from_bits(u64::from_le_bytes(read_array(payload, offset)?));
            value
                .is_finite()
                .then_some(ManagedFieldValue::Float(value))
                .ok_or(ManagedMemoryError::InvalidManagedScalar)
        }
        ManagedFieldType::Atom => Ok(ManagedFieldValue::Atom(AtomIndex::from_runtime(read_u32(
            payload, offset,
        )?))),
        ManagedFieldType::Reference(expected) => {
            let value = heap.reference_field(object, offset)?;
            if heap.descriptor(value)?.semantic_id() != expected {
                return Err(ManagedMemoryError::InvalidAggregateField);
            }
            Ok(ManagedFieldValue::Reference(value))
        }
    }
}

/// Validates unique nonempty record-style field identities.
fn validate_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Result<(), ManagedMemoryError> {
    let mut names = names.into_iter().collect::<Vec<_>>();
    if names.iter().any(|name| name.is_empty()) {
        return Err(ManagedMemoryError::InvalidAggregateShape);
    }
    names.sort_unstable();
    if names.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(ManagedMemoryError::InvalidAggregateShape);
    }
    Ok(())
}

/// Encodes length-delimited canonical representation text.
fn encode_text(bytes: &mut Vec<u8>, value: &str) -> Result<(), ManagedMemoryError> {
    let length =
        u64::try_from(value.len()).map_err(|_| ManagedMemoryError::InvalidAggregateShape)?;
    bytes.extend_from_slice(&length.to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
    Ok(())
}

/// Aligns one aggregate cursor without overflow.
fn align_up(value: usize, alignment: usize) -> Result<usize, ManagedMemoryError> {
    value
        .checked_add(alignment - 1)
        .map(|aligned| aligned & !(alignment - 1))
        .ok_or(ManagedMemoryError::InvalidAggregateShape)
}

/// Writes one scalar field into checked payload bounds.
fn write(payload: &mut [u8], offset: usize, bytes: &[u8]) -> Result<(), ManagedMemoryError> {
    payload
        .get_mut(offset..offset + bytes.len())
        .ok_or(ManagedMemoryError::InvalidAggregateShape)?
        .copy_from_slice(bytes);
    Ok(())
}

/// Reads one checked scalar field slice.
fn read(payload: &[u8], offset: usize, length: usize) -> Result<&[u8], ManagedMemoryError> {
    payload
        .get(offset..offset + length)
        .ok_or(ManagedMemoryError::InvalidAggregateShape)
}

/// Reads one fixed eight-byte scalar field.
fn read_array(payload: &[u8], offset: usize) -> Result<[u8; 8], ManagedMemoryError> {
    read(payload, offset, 8)?
        .try_into()
        .map_err(|_| ManagedMemoryError::InvalidAggregateShape)
}

/// Reads one fixed four-byte discriminant or atom field.
fn read_u32(payload: &[u8], offset: usize) -> Result<u32, ManagedMemoryError> {
    read(payload, offset, 4)?
        .try_into()
        .map(u32::from_le_bytes)
        .map_err(|_| ManagedMemoryError::InvalidAggregateShape)
}

#[cfg(test)]
#[path = "managed_aggregate_test.rs"]
mod managed_aggregate_test;
