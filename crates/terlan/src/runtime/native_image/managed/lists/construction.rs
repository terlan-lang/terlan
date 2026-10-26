//! Direct construction of immutable RRB nodes in their final managed storage.

use super::*;

type ListReferences = SmallVec<[(usize, TvmRef<()>); BRANCH_FACTOR]>;

/// Builds a list inside the caller's allocation transaction.
pub(super) fn list_from_elements(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    elements: &[ManagedFieldValue],
) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
    if elements.is_empty() {
        return allocate_empty_root(heap, descriptor);
    }
    if elements.len() <= INLINE_LIMIT {
        return allocate_inline_root(heap, descriptor, elements);
    }
    let mut level = elements
        .chunks(BRANCH_FACTOR)
        .map(|chunk| allocate_leaf(heap, descriptor, chunk))
        .collect::<Result<Vec<_>, _>>()?;
    while level.len() > 1 {
        level = level
            .chunks(BRANCH_FACTOR)
            .map(|children| allocate_internal(heap, descriptor, children))
            .collect::<Result<Vec<_>, _>>()?;
    }
    allocate_tree_root(heap, descriptor, level[0], 0, elements.len())
}

/// Allocates the canonical empty-list root.
pub(super) fn allocate_empty_root(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
    let managed = root_descriptor(heap, descriptor, FORM_EMPTY, &[], &[])?;
    heap.allocate_initialized(managed, &[], |payload| {
        write_root_header(payload, FORM_EMPTY, 0, 0)
    })
}

/// Allocates a compact inline root with no child node.
fn allocate_inline_root(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    elements: &[ManagedFieldValue],
) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
    let references = validated_references(heap, descriptor, elements, ROOT_HEADER_BYTES)?;
    let reference_offsets = references
        .iter()
        .map(|item| item.0)
        .collect::<SmallVec<[usize; INLINE_LIMIT]>>();
    let managed = root_descriptor(heap, descriptor, FORM_INLINE, elements, &reference_offsets)?;
    heap.allocate_initialized(managed, &references, |payload| {
        write_root_header(payload, FORM_INLINE, elements.len(), 0)?;
        write_elements(payload, descriptor, elements, ROOT_HEADER_BYTES)
    })
}

/// Allocates one root that owns a tree and optional front cursor.
pub(super) fn allocate_tree_root(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    tree: NodeSummary,
    start: usize,
    length: usize,
) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
    if start.checked_add(length).is_none_or(|end| end > tree.total) {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let managed = root_descriptor(heap, descriptor, FORM_TREE, &[], &[TREE_REFERENCE_OFFSET])?;
    heap.allocate_initialized(
        managed,
        &[(TREE_REFERENCE_OFFSET, tree.reference.erase())],
        |payload| write_root_header(payload, FORM_TREE, length, start),
    )
}

/// Allocates one packed leaf of at most 32 typed elements.
pub(super) fn allocate_leaf(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    elements: &[ManagedFieldValue],
) -> Result<NodeSummary, ManagedMemoryError> {
    if elements.is_empty() || elements.len() > BRANCH_FACTOR {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let (size, _) = packed_slot_layout(descriptor.element_type, NODE_HEADER_BYTES, elements.len())?;
    let references = validated_references(heap, descriptor, elements, NODE_HEADER_BYTES)?;
    let reference_offsets = references
        .iter()
        .map(|item| item.0)
        .collect::<SmallVec<[usize; BRANCH_FACTOR]>>();
    let managed = node_descriptor(
        heap,
        ManagedListNodeShape {
            semantic_id: descriptor.leaf_semantic_id,
            kind: NODE_LEAF,
            height: 0,
            count: elements.len(),
            size,
            sizes: &[],
            reference_offsets: &reference_offsets,
        },
    )?;
    let reference = heap.allocate_initialized(managed, &references, |payload| {
        write_node_header(payload, NODE_LEAF, 0, elements.len(), elements.len(), false)?;
        write_elements(payload, descriptor, elements, NODE_HEADER_BYTES)
    })?;
    Ok(NodeSummary {
        reference,
        total: elements.len(),
        height: 0,
        relaxed: false,
    })
}

/// Allocates one regular or relaxed 32-way internal node.
pub(super) fn allocate_internal(
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
    let mut references = ListReferences::new();
    for (index, child) in children.iter().enumerate() {
        let offset = NODE_HEADER_BYTES + index * 8;
        references.push((offset, child.reference.erase()));
    }
    let reference_offsets = references
        .iter()
        .map(|item| item.0)
        .collect::<SmallVec<[usize; BRANCH_FACTOR]>>();
    let managed = node_descriptor(
        heap,
        ManagedListNodeShape {
            semantic_id: descriptor.node_semantic_id,
            kind,
            height: child_height + 1,
            count: children.len(),
            size,
            sizes: if regular { &[] } else { children },
            reference_offsets: &reference_offsets,
        },
    )?;
    let reference = heap.allocate_initialized(managed, &references, |payload| {
        write_node_header(
            payload,
            kind,
            child_height + 1,
            children.len(),
            total,
            children.iter().any(|child| child.relaxed),
        )?;
        if !regular {
            let sizes_offset = NODE_HEADER_BYTES + children.len() * 8;
            let mut cumulative = 0_usize;
            for (index, child) in children.iter().enumerate() {
                cumulative = cumulative
                    .checked_add(child.total)
                    .ok_or(ManagedMemoryError::CollectionTooLarge)?;
                write_u64(payload, sizes_offset + index * 8, cumulative)?;
            }
        }
        Ok(())
    })?;
    Ok(NodeSummary {
        reference,
        total,
        height: child_height + 1,
        relaxed: !regular || children.iter().any(|child| child.relaxed),
    })
}

/// Validates every input before reserving final storage; references stay on the stack.
fn validated_references(
    heap: &ActorHeap,
    descriptor: &ManagedListDescriptor,
    elements: &[ManagedFieldValue],
    base: usize,
) -> Result<ListReferences, ManagedMemoryError> {
    let mut references = SmallVec::new();
    for (index, value) in elements.iter().copied().enumerate() {
        validate_list_value(heap, descriptor, value)?;
        if let ManagedFieldValue::Reference(reference) = value {
            references.push((
                packed_slot_offset(descriptor.element_type, base, index)?,
                reference,
            ));
        }
    }
    Ok(references)
}

/// Initializes scalar slots once; the allocator installs the precise reference fields.
fn write_elements(
    payload: &mut [u8],
    descriptor: &ManagedListDescriptor,
    elements: &[ManagedFieldValue],
    base: usize,
) -> Result<(), ManagedMemoryError> {
    for (index, value) in elements.iter().copied().enumerate() {
        let offset = packed_slot_offset(descriptor.element_type, base, index)?;
        encode_validated_typed_slot(payload, offset, descriptor.element_type, value)?;
    }
    Ok(())
}
