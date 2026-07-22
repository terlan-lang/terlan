//! Actor-local adaptive persistent RRB lists.

use std::sync::Arc;

use super::aggregates::{decode_typed_slot, encode_typed_slot};
use super::slots::{packed_slot_layout, packed_slot_offset};
use super::{
    ActorHeap, AllocationClass, ManagedFieldType, ManagedFieldValue, ManagedMemoryError,
    ManagedTypeDescriptor, SemanticTypeId, TvmRef,
};

#[path = "lists/persistent.rs"]
mod persistent;
#[path = "lists/transient.rs"]
mod transient;

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
}

impl ManagedListDescriptor {
    /// Creates one bounded list profile for a canonical checked element type.
    pub fn new(
        canonical_type: &str,
        element_type: ManagedFieldType,
    ) -> Result<Self, ManagedMemoryError> {
        if canonical_type.is_empty() {
            return Err(ManagedMemoryError::InvalidAggregateShape);
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
        if elements.is_empty() {
            return allocate_empty_root(self, descriptor);
        }
        if elements.len() <= INLINE_LIMIT {
            return allocate_inline_root(self, descriptor, elements);
        }
        let mut level = elements
            .chunks(BRANCH_FACTOR)
            .map(|chunk| allocate_leaf(self, descriptor, chunk))
            .collect::<Result<Vec<_>, _>>()?;
        while level.len() > 1 {
            level = level
                .chunks(BRANCH_FACTOR)
                .map(|children| allocate_internal(self, descriptor, children))
                .collect::<Result<Vec<_>, _>>()?;
        }
        allocate_tree_root(self, descriptor, level[0], 0, elements.len())
    }

    /// Returns the logical list length in constant time.
    pub fn list_length(
        &self,
        descriptor: &ManagedListDescriptor,
        list: TvmRef<ManagedList>,
    ) -> Result<usize, ManagedMemoryError> {
        Ok(read_root(self, descriptor, list)?.0.length)
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
                decode_typed_slot(self, list.erase(), payload, offset, descriptor.element_type)
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
                header.length - 1,
            )
            .map(Some);
        }
        let elements = self.list_elements_from(descriptor, list, 1)?;
        self.list_from_elements(descriptor, &elements).map(Some)
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
        if start > length {
            return Err(ManagedMemoryError::CollectionIndexOutOfBounds);
        }
        (start..length)
            .map(|index| self.list_get(descriptor, list, index))
            .collect()
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

/// Allocates the canonical empty-list root.
fn allocate_empty_root(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
    let mut payload = vec![0; ROOT_HEADER_BYTES];
    write_root_header(&mut payload, FORM_EMPTY, 0, 0)?;
    heap.allocate(
        root_descriptor(descriptor, FORM_EMPTY, &[], vec![])?,
        &payload,
        &[],
    )
}

/// Allocates a compact inline root with no child node.
fn allocate_inline_root(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    elements: &[ManagedFieldValue],
) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
    let (size, _) = packed_slot_layout(descriptor.element_type, ROOT_HEADER_BYTES, elements.len())?;
    let mut payload = vec![0; size];
    write_root_header(&mut payload, FORM_INLINE, elements.len(), 0)?;
    let mut references = Vec::new();
    for (index, value) in elements.iter().enumerate() {
        let offset = packed_slot_offset(descriptor.element_type, ROOT_HEADER_BYTES, index)?;
        encode_typed_slot(
            heap,
            &mut payload,
            offset,
            descriptor.element_type,
            *value,
            &mut references,
        )?;
    }
    heap.allocate(
        root_descriptor(
            descriptor,
            FORM_INLINE,
            elements,
            references.iter().map(|item| item.0).collect(),
        )?,
        &payload,
        &references,
    )
}

/// Allocates one root that owns a tree and optional front cursor.
fn allocate_tree_root(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    tree: NodeSummary,
    start: usize,
    length: usize,
) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
    if start.checked_add(length).is_none_or(|end| end > tree.total) {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let mut payload = vec![0; TREE_ROOT_BYTES];
    write_root_header(&mut payload, FORM_TREE, length, start)?;
    heap.allocate(
        root_descriptor(descriptor, FORM_TREE, &[], vec![TREE_REFERENCE_OFFSET])?,
        &payload,
        &[(TREE_REFERENCE_OFFSET, tree.reference.erase())],
    )
}

/// Allocates one packed leaf of at most 32 typed elements.
fn allocate_leaf(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    elements: &[ManagedFieldValue],
) -> Result<NodeSummary, ManagedMemoryError> {
    if elements.is_empty() || elements.len() > BRANCH_FACTOR {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let (size, _) = packed_slot_layout(descriptor.element_type, NODE_HEADER_BYTES, elements.len())?;
    let mut payload = vec![0; size];
    write_node_header(
        &mut payload,
        NODE_LEAF,
        0,
        elements.len(),
        elements.len(),
        false,
    )?;
    let mut references = Vec::new();
    for (index, value) in elements.iter().enumerate() {
        let offset = packed_slot_offset(descriptor.element_type, NODE_HEADER_BYTES, index)?;
        encode_typed_slot(
            heap,
            &mut payload,
            offset,
            descriptor.element_type,
            *value,
            &mut references,
        )?;
    }
    let reference = heap.allocate(
        node_descriptor(
            descriptor.leaf_semantic_id,
            NODE_LEAF,
            0,
            elements.len(),
            size,
            &[],
            references.iter().map(|item| item.0).collect(),
        )?,
        &payload,
        &references,
    )?;
    Ok(NodeSummary {
        reference,
        total: elements.len(),
        height: 0,
        relaxed: false,
    })
}

/// Allocates one regular or relaxed 32-way internal node.
fn allocate_internal(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    children: &[NodeSummary],
) -> Result<NodeSummary, ManagedMemoryError> {
    if children.is_empty() || children.len() > BRANCH_FACTOR {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let child_height = children[0].height;
    if children.iter().any(|child| child.height != child_height) {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let regular = children
        .windows(2)
        .all(|pair| pair[0].total == pair[1].total)
        && children.iter().all(|child| !child.relaxed);
    let kind = if regular { NODE_REGULAR } else { NODE_RELAXED };
    let sizes_bytes = if regular { 0 } else { children.len() * 8 };
    let size = NODE_HEADER_BYTES
        .checked_add(children.len() * 8)
        .and_then(|value| value.checked_add(sizes_bytes))
        .ok_or(ManagedMemoryError::CollectionTooLarge)?;
    let total = children.iter().try_fold(0_usize, |total, child| {
        total
            .checked_add(child.total)
            .ok_or(ManagedMemoryError::CollectionTooLarge)
    })?;
    let mut payload = vec![0; size];
    write_node_header(
        &mut payload,
        kind,
        child_height + 1,
        children.len(),
        total,
        children.iter().any(|child| child.relaxed),
    )?;
    let mut references = Vec::with_capacity(children.len());
    let sizes_offset = NODE_HEADER_BYTES + children.len() * 8;
    let mut cumulative = 0_usize;
    for (index, child) in children.iter().enumerate() {
        let offset = NODE_HEADER_BYTES + index * 8;
        references.push((offset, child.reference.erase()));
        if !regular {
            cumulative = cumulative
                .checked_add(child.total)
                .ok_or(ManagedMemoryError::CollectionTooLarge)?;
            write_u64(&mut payload, sizes_offset + index * 8, cumulative)?;
        }
    }
    let reference = heap.allocate(
        node_descriptor(
            descriptor.node_semantic_id,
            kind,
            child_height + 1,
            children.len(),
            size,
            if regular { &[] } else { children },
            references.iter().map(|item| item.0).collect(),
        )?,
        &payload,
        &references,
    )?;
    Ok(NodeSummary {
        reference,
        total,
        height: child_height + 1,
        relaxed: !regular || children.iter().any(|child| child.relaxed),
    })
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
        return decode_typed_slot(heap, node.erase(), payload, offset, descriptor.element_type);
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
    descriptor: &ManagedListDescriptor,
    form: u8,
    elements: &[ManagedFieldValue],
    reference_offsets: Vec<usize>,
) -> Result<Arc<ManagedTypeDescriptor>, ManagedMemoryError> {
    let size = if form == FORM_INLINE {
        packed_slot_layout(descriptor.element_type, ROOT_HEADER_BYTES, elements.len())?.0
    } else if form == FORM_TREE {
        TREE_ROOT_BYTES
    } else {
        ROOT_HEADER_BYTES
    };
    let mut representation = vec![b'L', form];
    descriptor.element_type.encode(&mut representation);
    representation.extend_from_slice(&(elements.len() as u64).to_le_bytes());
    ManagedTypeDescriptor::new_specialized(
        descriptor.semantic_id,
        size,
        8,
        reference_offsets,
        AllocationClass::Young,
        &representation,
    )
    .map(Arc::new)
}

/// Builds a shape-specific private leaf or internal-node descriptor.
fn node_descriptor(
    semantic_id: SemanticTypeId,
    kind: u8,
    height: u8,
    count: usize,
    size: usize,
    sizes: &[NodeSummary],
    reference_offsets: Vec<usize>,
) -> Result<Arc<ManagedTypeDescriptor>, ManagedMemoryError> {
    let mut representation = vec![b'N', kind, height];
    representation.extend_from_slice(&(count as u64).to_le_bytes());
    for child in sizes {
        representation.extend_from_slice(&(child.total as u64).to_le_bytes());
    }
    ManagedTypeDescriptor::new_specialized(
        semantic_id,
        size,
        8,
        reference_offsets,
        AllocationClass::Young,
        &representation,
    )
    .map(Arc::new)
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
mod managed_list_test;

#[cfg(test)]
#[path = "managed_list_profile_benchmark_test.rs"]
mod managed_list_profile_benchmark_test;
