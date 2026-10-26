//! Actor-local adaptive persistent RRB lists.

use std::sync::Arc;

use smallvec::SmallVec;

use super::aggregates::{decode_typed_slot, encode_validated_typed_slot, validate_typed_value};
use super::slots::{packed_slot_layout, packed_slot_offset};
use super::{
    ActorHeap, AllocationClass, ManagedFieldType, ManagedFieldValue, ManagedMemoryError,
    ManagedTypeDescriptor, SemanticTypeId, TvmRef,
};

#[path = "lists/construction.rs"]
mod construction;
#[cfg(test)]
#[path = "lists/construction_test.rs"]
mod construction_test;
#[path = "lists/persistent.rs"]
mod persistent;
use construction::{allocate_empty_root, allocate_internal, allocate_leaf, allocate_tree_root};
#[path = "lists/transient.rs"]
mod transient;
#[path = "lists/traversal.rs"]
mod traversal;

pub use transient::ManagedListBuilder;

const ROOT_MAGIC: u32 = 0x3154_534c;
const NODE_MAGIC: u32 = 0x3142_5252;
const ROOT_HEADER_BYTES: usize = 16;
const TREE_ROOT_BYTES: usize = 32;
const NODE_HEADER_BYTES: usize = 16;
const TREE_REFERENCE_OFFSET: usize = 24;
const BRANCH_FACTOR: usize = 32;
const INLINE_LIMIT: usize = 8;
const MAX_LIST_ELEMENTS: usize = 1 << 24;
const FORM_EMPTY: u8 = 0;
const FORM_INLINE: u8 = 1;
const FORM_TREE: u8 = 2;
const NODE_LEAF: u8 = 0;
const NODE_REGULAR: u8 = 1;
const NODE_RELAXED: u8 = 2;

/// Compile-time marker for one actor-local persistent list root.
#[derive(Debug)]
pub struct ManagedList;

/// Observable representation family used only for conformance and benchmarks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ManagedListProfile {
    Empty,
    Inline,
    RegularTree,
    RelaxedTree,
}

/// Canonical typed list descriptor shared by all physical forms of `List[T]`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ManagedListDescriptor {
    semantic_id: SemanticTypeId,
    leaf_semantic_id: SemanticTypeId,
    node_semantic_id: SemanticTypeId,
    element_type: ManagedFieldType,
    reference_variants: Box<[SemanticTypeId]>,
}

impl ManagedListDescriptor {
    /// Creates one bounded list profile for a canonical checked element type.
    pub fn new(
        canonical_type: &str,
        element_type: ManagedFieldType,
    ) -> Result<Self, ManagedMemoryError> {
        Self::with_reference_variants(canonical_type, element_type, Vec::new())
    }

    /// Creates one bounded list profile whose transparent reference union
    /// admits only the supplied concrete semantic identities.
    pub fn with_reference_variants(
        canonical_type: &str,
        element_type: ManagedFieldType,
        mut reference_variants: Vec<SemanticTypeId>,
    ) -> Result<Self, ManagedMemoryError> {
        if canonical_type.is_empty() {
            return Err(ManagedMemoryError::InvalidAggregateShape);
        }
        if !reference_variants.is_empty() && !matches!(element_type, ManagedFieldType::Reference(_))
        {
            return Err(ManagedMemoryError::InvalidAggregateShape);
        }
        reference_variants.sort_unstable();
        reference_variants.dedup();
        if let ManagedFieldType::Reference(expected) = element_type {
            reference_variants.retain(|semantic| *semantic != expected);
        }
        Ok(Self {
            semantic_id: SemanticTypeId::from_canonical(canonical_type)?,
            leaf_semantic_id: SemanticTypeId::from_canonical(&format!(
                "{canonical_type}#rrb-leaf"
            ))?,
            node_semantic_id: SemanticTypeId::from_canonical(&format!(
                "{canonical_type}#rrb-node"
            ))?,
            element_type,
            reference_variants: reference_variants.into_boxed_slice(),
        })
    }

    /// Returns the semantic identity of the opaque list root.
    pub fn semantic_id(&self) -> SemanticTypeId {
        self.semantic_id
    }

    /// Returns the statically selected element slot category.
    pub fn element_type(&self) -> ManagedFieldType {
        self.element_type
    }

    /// Returns the canonical concrete semantics admitted by a transparent
    /// reference union element type.
    pub fn reference_variants(&self) -> &[SemanticTypeId] {
        &self.reference_variants
    }

    /// Reports whether one live concrete semantic can inhabit this list slot.
    fn accepts_reference_semantic(&self, semantic: SemanticTypeId) -> bool {
        matches!(self.element_type, ManagedFieldType::Reference(expected) if expected == semantic)
            || self.reference_variants.binary_search(&semantic).is_ok()
    }
}

/// Internal allocation summary used while constructing one balanced RRB level.
#[derive(Clone, Copy, Debug)]
struct NodeSummary {
    reference: TvmRef<RrbNode>,
    total: usize,
    height: u8,
    relaxed: bool,
}

/// Opaque marker for private managed RRB leaf and internal-node references.
#[derive(Debug)]
struct RrbNode;

/// Decoded immutable list-root header.
#[derive(Clone, Copy, Debug)]
struct RootHeader {
    form: u8,
    length: usize,
    start: usize,
}

impl ActorHeap {
    /// Materializes one immutable list through bounded packed-leaf construction.
    pub fn list_from_elements(
        &mut self,
        descriptor: &ManagedListDescriptor,
        elements: &[ManagedFieldValue],
    ) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
        validate_element_count(elements.len())?;
        self.with_allocation_transaction(|heap| {
            construction::list_from_elements(heap, descriptor, elements)
        })
    }

    /// Returns the logical list length in constant time.
    pub fn list_length(
        &self,
        descriptor: &ManagedListDescriptor,
        list: TvmRef<ManagedList>,
    ) -> Result<usize, ManagedMemoryError> {
        let length = read_root(self, descriptor, list)?.0.length;
        validate_element_count(length)?;
        Ok(length)
    }

    /// Reports whether a typed list contains no elements.
    pub fn list_is_empty(
        &self,
        descriptor: &ManagedListDescriptor,
        list: TvmRef<ManagedList>,
    ) -> Result<bool, ManagedMemoryError> {
        Ok(self.list_length(descriptor, list)? == 0)
    }

    /// Returns the active physical profile without exposing node layouts.
    pub fn list_profile(
        &self,
        descriptor: &ManagedListDescriptor,
        list: TvmRef<ManagedList>,
    ) -> Result<ManagedListProfile, ManagedMemoryError> {
        let (header, payload) = read_root(self, descriptor, list)?;
        match header.form {
            FORM_EMPTY => Ok(ManagedListProfile::Empty),
            FORM_INLINE => Ok(ManagedListProfile::Inline),
            FORM_TREE => {
                let tree = self.reference_field(list, TREE_REFERENCE_OFFSET)?;
                let node = read_node(self, descriptor, tree.cast())?;
                if node.kind == NODE_RELAXED || node.relaxed_descendant {
                    Ok(ManagedListProfile::RelaxedTree)
                } else {
                    Ok(ManagedListProfile::RegularTree)
                }
            }
            _ => {
                let _ = payload;
                Err(ManagedMemoryError::CorruptedCollection)
            }
        }
    }

    /// Returns one typed element using bounded 32-way tree traversal.
    pub fn list_get(
        &self,
        descriptor: &ManagedListDescriptor,
        list: TvmRef<ManagedList>,
        index: usize,
    ) -> Result<ManagedFieldValue, ManagedMemoryError> {
        let (header, payload) = read_root(self, descriptor, list)?;
        if index >= header.length {
            return Err(ManagedMemoryError::CollectionIndexOutOfBounds);
        }
        match header.form {
            FORM_INLINE => {
                let offset = packed_slot_offset(descriptor.element_type, ROOT_HEADER_BYTES, index)?;
                decode_list_slot(self, descriptor, list.erase(), payload, offset)
            }
            FORM_TREE => {
                let absolute = header
                    .start
                    .checked_add(index)
                    .ok_or(ManagedMemoryError::CorruptedCollection)?;
                let tree = self.reference_field(list, TREE_REFERENCE_OFFSET)?;
                node_get(self, descriptor, tree.cast(), absolute)
            }
            _ => Err(ManagedMemoryError::CorruptedCollection),
        }
    }

    /// Returns the first element of a nonempty typed list.
    pub fn list_first(
        &self,
        descriptor: &ManagedListDescriptor,
        list: TvmRef<ManagedList>,
    ) -> Result<Option<ManagedFieldValue>, ManagedMemoryError> {
        if self.list_is_empty(descriptor, list)? {
            Ok(None)
        } else {
            self.list_get(descriptor, list, 0).map(Some)
        }
    }

    /// Returns a persistent tail view and trims fully excluded leaves.
    pub fn list_rest(
        &mut self,
        descriptor: &ManagedListDescriptor,
        list: TvmRef<ManagedList>,
    ) -> Result<Option<TvmRef<ManagedList>>, ManagedMemoryError> {
        let (header, _) = read_root(self, descriptor, list)?;
        if header.length == 0 {
            return Ok(None);
        }
        if header.length == 1 {
            return allocate_empty_root(self, descriptor).map(Some);
        }
        let remaining = header.length - 1;
        if remaining <= INLINE_LIMIT {
            let elements = self.list_elements_from(descriptor, list, 1)?;
            return self.list_from_elements(descriptor, &elements).map(Some);
        }
        if header.form == FORM_TREE && (header.start + 1) % BRANCH_FACTOR != 0 {
            let tree = self.reference_field(list, TREE_REFERENCE_OFFSET)?;
            return allocate_tree_root(
                self,
                descriptor,
                NodeSummary {
                    reference: tree.cast(),
                    total: header.start + header.length,
                    height: read_node(self, descriptor, tree.cast())?.height,
                    relaxed: false,
                },
                header.start + 1,
                remaining,
            )
            .map(Some);
        }
        if header.form == FORM_TREE {
            let trimmed = persistent::trim_prefix(self, descriptor, list, header.start + 1)?
                .ok_or(ManagedMemoryError::CorruptedCollection)?;
            return allocate_tree_root(self, descriptor, trimmed, 0, remaining).map(Some);
        }
        Err(ManagedMemoryError::CorruptedCollection)
    }

    /// Returns a persistent list with one element prepended.
    pub fn list_prepend(
        &mut self,
        descriptor: &ManagedListDescriptor,
        list: TvmRef<ManagedList>,
        value: ManagedFieldValue,
    ) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
        persistent::prepend(self, descriptor, list, value)
    }

    /// Returns a persistent list with one element appended.
    pub fn list_append(
        &mut self,
        descriptor: &ManagedListDescriptor,
        list: TvmRef<ManagedList>,
        value: ManagedFieldValue,
    ) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
        persistent::append(self, descriptor, list, value)
    }

    /// Returns a persistent list with one indexed value replaced.
    pub fn list_update(
        &mut self,
        descriptor: &ManagedListDescriptor,
        list: TvmRef<ManagedList>,
        index: usize,
        value: ManagedFieldValue,
    ) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
        persistent::update(self, descriptor, list, index, value)
    }

    /// Concatenates two typed lists by rebalancing their persistent fringes.
    pub fn list_concat(
        &mut self,
        descriptor: &ManagedListDescriptor,
        left: TvmRef<ManagedList>,
        right: TvmRef<ManagedList>,
    ) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
        persistent::concat(self, descriptor, left, right)
    }

    /// Removes the first structurally equal occurrence for each removal value.
    ///
    /// The compiler supplies equality specialized for `T`, allowing managed
    /// references to compare content without exposing or comparing addresses.
    pub fn list_subtract<F>(
        &mut self,
        descriptor: &ManagedListDescriptor,
        list: TvmRef<ManagedList>,
        removals: TvmRef<ManagedList>,
        equivalent: F,
    ) -> Result<TvmRef<ManagedList>, ManagedMemoryError>
    where
        F: FnMut(
            &ActorHeap,
            ManagedFieldValue,
            ManagedFieldValue,
        ) -> Result<bool, ManagedMemoryError>,
    {
        persistent::subtract(self, descriptor, list, removals, equivalent)
    }

    /// Returns a persistent list with two indexed values exchanged.
    pub fn list_swap(
        &mut self,
        descriptor: &ManagedListDescriptor,
        list: TvmRef<ManagedList>,
        left: usize,
        right: usize,
    ) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
        persistent::swap(self, descriptor, list, left, right)
    }

    /// Materializes list elements in deterministic traversal order.
    pub fn list_elements(
        &self,
        descriptor: &ManagedListDescriptor,
        list: TvmRef<ManagedList>,
    ) -> Result<Vec<ManagedFieldValue>, ManagedMemoryError> {
        self.list_elements_from(descriptor, list, 0)
    }

    /// Reads a suffix into a bounded transient construction buffer.
    fn list_elements_from(
        &self,
        descriptor: &ManagedListDescriptor,
        list: TvmRef<ManagedList>,
        start: usize,
    ) -> Result<Vec<ManagedFieldValue>, ManagedMemoryError> {
        let length = self.list_length(descriptor, list)?;
        let capacity = length
            .checked_sub(start)
            .ok_or(ManagedMemoryError::CollectionIndexOutOfBounds)?;
        let mut elements = Vec::with_capacity(capacity);
        traversal::visit(self, descriptor, list, start, &mut |value| {
            elements.push(value);
            Ok(())
        })?;
        Ok(elements)
    }

    /// Visits an immutable list leaf by leaf while this heap cannot move or mutate.
    pub(super) fn visit_list(
        &self,
        descriptor: &ManagedListDescriptor,
        list: TvmRef<ManagedList>,
        mut visitor: impl FnMut(ManagedFieldValue) -> Result<(), ManagedMemoryError>,
    ) -> Result<(), ManagedMemoryError> {
        traversal::visit(self, descriptor, list, 0, &mut visitor)
    }
}

/// Decoded immutable private RRB node header.
#[derive(Clone, Copy, Debug)]
struct NodeHeader {
    kind: u8,
    height: u8,
    count: usize,
    total: usize,
    relaxed_descendant: bool,
}

/// Validates one list element against its exact scalar type or declared
/// transparent reference-union alternatives.
fn validate_list_value(
    heap: &ActorHeap,
    descriptor: &ManagedListDescriptor,
    value: ManagedFieldValue,
) -> Result<(), ManagedMemoryError> {
    if let (ManagedFieldType::Reference(_), ManagedFieldValue::Reference(reference)) =
        (descriptor.element_type, value)
    {
        let actual = heap.descriptor(reference)?.semantic_id();
        return descriptor
            .accepts_reference_semantic(actual)
            .then_some(())
            .ok_or(ManagedMemoryError::InvalidAggregateField);
    }
    validate_typed_value(heap, descriptor.element_type, value)
}

/// Decodes one list slot while preserving transparent-union validation.
fn decode_list_slot(
    heap: &ActorHeap,
    descriptor: &ManagedListDescriptor,
    object: TvmRef<()>,
    payload: &[u8],
    offset: usize,
) -> Result<ManagedFieldValue, ManagedMemoryError> {
    if matches!(descriptor.element_type, ManagedFieldType::Reference(_)) {
        let value = ManagedFieldValue::Reference(heap.reference_field(object, offset)?);
        validate_list_value(heap, descriptor, value)?;
        return Ok(value);
    }
    decode_typed_slot(heap, object, payload, offset, descriptor.element_type)
}

/// Traverses one RRB node using regular arithmetic or relaxed cumulative sizes.
fn node_get(
    heap: &ActorHeap,
    descriptor: &ManagedListDescriptor,
    node: TvmRef<RrbNode>,
    index: usize,
) -> Result<ManagedFieldValue, ManagedMemoryError> {
    let header = read_node(heap, descriptor, node)?;
    if index >= header.total {
        return Err(ManagedMemoryError::CollectionIndexOutOfBounds);
    }
    let payload = heap.read(node)?;
    if header.kind == NODE_LEAF {
        let offset = packed_slot_offset(descriptor.element_type, NODE_HEADER_BYTES, index)?;
        return decode_list_slot(heap, descriptor, node.erase(), payload, offset);
    }
    let (child_index, child_start) = if header.kind == NODE_REGULAR {
        let first: TvmRef<RrbNode> = heap.reference_field(node, NODE_HEADER_BYTES)?.cast();
        let child_total = read_node(heap, descriptor, first)?.total;
        (index / child_total, (index / child_total) * child_total)
    } else if header.kind == NODE_RELAXED {
        let sizes_offset = NODE_HEADER_BYTES + header.count * 8;
        let mut prior = 0;
        let mut selected = None;
        for child in 0..header.count {
            let cumulative = read_u64(payload, sizes_offset + child * 8)?;
            if index < cumulative {
                selected = Some((child, prior));
                break;
            }
            prior = cumulative;
        }
        selected.ok_or(ManagedMemoryError::CorruptedCollection)?
    } else {
        return Err(ManagedMemoryError::CorruptedCollection);
    };
    if child_index >= header.count {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let child = heap
        .reference_field(node, NODE_HEADER_BYTES + child_index * 8)?
        .cast();
    node_get(heap, descriptor, child, index - child_start)
}

/// Validates and reads one list root payload.
fn read_root<'a>(
    heap: &'a ActorHeap,
    descriptor: &ManagedListDescriptor,
    list: TvmRef<ManagedList>,
) -> Result<(RootHeader, &'a [u8]), ManagedMemoryError> {
    if heap.descriptor(list)?.semantic_id() != descriptor.semantic_id {
        return Err(ManagedMemoryError::ManagedTypeMismatch);
    }
    let payload = heap.read(list)?;
    if read_u32(payload, 0)? != ROOT_MAGIC {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let form = *payload
        .get(4)
        .ok_or(ManagedMemoryError::CorruptedCollection)?;
    let length = read_u64(payload, 8)?;
    let start = if form == FORM_TREE {
        read_u64(payload, 16)?
    } else {
        0
    };
    if form == FORM_EMPTY && length != 0
        || form == FORM_INLINE && (length == 0 || length > INLINE_LIMIT)
        || form == FORM_TREE && length == 0
    {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    Ok((
        RootHeader {
            form,
            length,
            start,
        },
        payload,
    ))
}

/// Validates and reads one private RRB node header.
fn read_node(
    heap: &ActorHeap,
    descriptor: &ManagedListDescriptor,
    node: TvmRef<RrbNode>,
) -> Result<NodeHeader, ManagedMemoryError> {
    let semantic = heap.descriptor(node)?.semantic_id();
    if semantic != descriptor.leaf_semantic_id && semantic != descriptor.node_semantic_id {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let payload = heap.read(node)?;
    if read_u32(payload, 0)? != NODE_MAGIC {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let kind = payload[4];
    let height = payload[5];
    let count = u16::from_le_bytes(
        payload[6..8]
            .try_into()
            .map_err(|_| ManagedMemoryError::CorruptedCollection)?,
    ) as usize;
    let total = read_u64(payload, 8)?;
    let relaxed_descendant = payload.get(5).is_some_and(|_| payload[4] == NODE_RELAXED);
    if count == 0
        || count > BRANCH_FACTOR
        || total == 0
        || kind == NODE_LEAF && (height != 0 || total != count)
        || kind != NODE_LEAF && height == 0
        || !matches!(kind, NODE_LEAF | NODE_REGULAR | NODE_RELAXED)
    {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    Ok(NodeHeader {
        kind,
        height,
        count,
        total,
        relaxed_descendant,
    })
}

/// Builds a shape-specific opaque root descriptor.
fn root_descriptor(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    form: u8,
    elements: &[ManagedFieldValue],
    reference_offsets: &[usize],
) -> Result<Arc<ManagedTypeDescriptor>, ManagedMemoryError> {
    let size = if form == FORM_INLINE {
        packed_slot_layout(descriptor.element_type, ROOT_HEADER_BYTES, elements.len())?.0
    } else if form == FORM_TREE {
        TREE_ROOT_BYTES
    } else {
        ROOT_HEADER_BYTES
    };
    let mut representation = SmallVec::<[u8; 48]>::from_slice(&[b'L', form]);
    descriptor.element_type.encode(&mut representation);
    representation.extend_from_slice(&(elements.len() as u64).to_le_bytes());
    heap.specialized_descriptor(
        descriptor.semantic_id,
        size,
        8,
        reference_offsets,
        AllocationClass::Young,
        &representation,
    )
}

/// Builds a shape-specific private leaf or internal-node descriptor.
struct ManagedListNodeShape<'a> {
    semantic_id: SemanticTypeId,
    kind: u8,
    height: u8,
    count: usize,
    size: usize,
    sizes: &'a [NodeSummary],
    reference_offsets: &'a [usize],
}

fn node_descriptor(
    heap: &mut ActorHeap,
    shape: ManagedListNodeShape<'_>,
) -> Result<Arc<ManagedTypeDescriptor>, ManagedMemoryError> {
    let ManagedListNodeShape {
        semantic_id,
        kind,
        height,
        count,
        size,
        sizes,
        reference_offsets,
    } = shape;
    let mut representation = SmallVec::<[u8; 288]>::from_slice(&[b'N', kind, height]);
    representation.extend_from_slice(&(count as u64).to_le_bytes());
    for child in sizes {
        representation.extend_from_slice(&(child.total as u64).to_le_bytes());
    }
    heap.specialized_descriptor(
        semantic_id,
        size,
        8,
        reference_offsets,
        AllocationClass::Young,
        &representation,
    )
}

/// Writes one root header.
fn write_root_header(
    payload: &mut [u8],
    form: u8,
    length: usize,
    start: usize,
) -> Result<(), ManagedMemoryError> {
    payload[..4].copy_from_slice(&ROOT_MAGIC.to_le_bytes());
    payload[4] = form;
    write_u64(payload, 8, length)?;
    if form == FORM_TREE {
        write_u64(payload, 16, start)?;
    }
    Ok(())
}

/// Writes one private RRB node header.
fn write_node_header(
    payload: &mut [u8],
    kind: u8,
    height: u8,
    count: usize,
    total: usize,
    _relaxed_descendant: bool,
) -> Result<(), ManagedMemoryError> {
    payload[..4].copy_from_slice(&NODE_MAGIC.to_le_bytes());
    payload[4] = kind;
    payload[5] = height;
    payload[6..8].copy_from_slice(
        &u16::try_from(count)
            .map_err(|_| ManagedMemoryError::CorruptedCollection)?
            .to_le_bytes(),
    );
    write_u64(payload, 8, total)
}

/// Reads one checked little-endian u32 field.
fn read_u32(payload: &[u8], offset: usize) -> Result<u32, ManagedMemoryError> {
    payload
        .get(offset..offset + 4)
        .ok_or(ManagedMemoryError::CorruptedCollection)?
        .try_into()
        .map(u32::from_le_bytes)
        .map_err(|_| ManagedMemoryError::CorruptedCollection)
}

/// Reads one checked little-endian collection length.
fn read_u64(payload: &[u8], offset: usize) -> Result<usize, ManagedMemoryError> {
    let bytes = payload
        .get(offset..offset + 8)
        .ok_or(ManagedMemoryError::CorruptedCollection)?
        .try_into()
        .map_err(|_| ManagedMemoryError::CorruptedCollection)?;
    usize::try_from(u64::from_le_bytes(bytes)).map_err(|_| ManagedMemoryError::CorruptedCollection)
}

/// Writes one checked little-endian collection length.
fn write_u64(payload: &mut [u8], offset: usize, value: usize) -> Result<(), ManagedMemoryError> {
    let value = u64::try_from(value).map_err(|_| ManagedMemoryError::CollectionTooLarge)?;
    payload
        .get_mut(offset..offset + 8)
        .ok_or(ManagedMemoryError::CorruptedCollection)?
        .copy_from_slice(&value.to_le_bytes());
    Ok(())
}

/// Enforces the bounded list shape before materialization.
fn validate_element_count(count: usize) -> Result<(), ManagedMemoryError> {
    if count > MAX_LIST_ELEMENTS {
        Err(ManagedMemoryError::CollectionTooLarge)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "managed_list_test.rs"]
#[cfg(test)]
mod managed_list_test;

#[cfg(test)]
#[path = "managed_list_profile_benchmark_test.rs"]
#[cfg(test)]
mod managed_list_profile_benchmark_test;
