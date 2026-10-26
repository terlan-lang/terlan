//! Bounded leaf-wise traversal of immutable regular, relaxed, and sliced lists.

use std::ops::Range;

use super::*;

pub(super) fn visit(
    heap: &ActorHeap,
    descriptor: &ManagedListDescriptor,
    list: TvmRef<ManagedList>,
    start: usize,
    visitor: &mut impl FnMut(ManagedFieldValue) -> Result<(), ManagedMemoryError>,
) -> Result<(), ManagedMemoryError> {
    let (header, payload) = read_root(heap, descriptor, list)?;
    validate_element_count(header.length)?;
    if start > header.length {
        return Err(ManagedMemoryError::CollectionIndexOutOfBounds);
    }
    if start == header.length {
        return Ok(());
    }
    match header.form {
        FORM_INLINE if header.length <= INLINE_LIMIT => visit_slots(
            heap,
            descriptor,
            list.erase(),
            payload,
            ROOT_HEADER_BYTES,
            start..header.length,
            visitor,
        ),
        FORM_TREE => {
            let end = header
                .start
                .checked_add(header.length)
                .ok_or(ManagedMemoryError::CorruptedCollection)?;
            let start = header
                .start
                .checked_add(start)
                .ok_or(ManagedMemoryError::CorruptedCollection)?;
            let tree = heap.reference_field(list, TREE_REFERENCE_OFFSET)?.cast();
            visit_node(heap, descriptor, tree, start..end, visitor)
        }
        _ => Err(ManagedMemoryError::CorruptedCollection),
    }
}

fn visit_node(
    heap: &ActorHeap,
    descriptor: &ManagedListDescriptor,
    node: TvmRef<RrbNode>,
    range: Range<usize>,
    visitor: &mut impl FnMut(ManagedFieldValue) -> Result<(), ManagedMemoryError>,
) -> Result<(), ManagedMemoryError> {
    let header = read_node(heap, descriptor, node)?;
    if range.start > range.end || range.end > header.total {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let payload = heap.read(node)?;
    if header.kind == NODE_LEAF {
        if header.total != header.count {
            return Err(ManagedMemoryError::CorruptedCollection);
        }
        return visit_slots(
            heap,
            descriptor,
            node.erase(),
            payload,
            NODE_HEADER_BYTES,
            range,
            visitor,
        );
    }
    let mut prior = 0usize;
    for index in 0..header.count {
        let child = heap
            .reference_field(node, NODE_HEADER_BYTES + index * 8)?
            .cast();
        let child_header = read_node(heap, descriptor, child)?;
        if child_header.height.checked_add(1) != Some(header.height) {
            return Err(ManagedMemoryError::CorruptedCollection);
        }
        let end = prior
            .checked_add(child_header.total)
            .ok_or(ManagedMemoryError::CorruptedCollection)?;
        if header.kind == NODE_RELAXED
            && read_u64(payload, NODE_HEADER_BYTES + header.count * 8 + index * 8)? != end
        {
            return Err(ManagedMemoryError::CorruptedCollection);
        }
        if range.start < end && range.end > prior {
            visit_node(
                heap,
                descriptor,
                child,
                range.start.saturating_sub(prior)..range.end.min(end) - prior,
                visitor,
            )?;
        }
        prior = end;
    }
    if prior != header.total {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    Ok(())
}

/// Bounds are proved once per leaf; typed slots retain owner and value validation.
fn visit_slots(
    heap: &ActorHeap,
    descriptor: &ManagedListDescriptor,
    object: TvmRef<()>,
    payload: &[u8],
    base: usize,
    range: Range<usize>,
    visitor: &mut impl FnMut(ManagedFieldValue) -> Result<(), ManagedMemoryError>,
) -> Result<(), ManagedMemoryError> {
    let (end, stride) = packed_slot_layout(descriptor.element_type, base, range.end)?;
    if end > payload.len() {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let mut offset = packed_slot_offset(descriptor.element_type, base, range.start)?;
    for _ in range {
        visitor(decode_list_slot(heap, descriptor, object, payload, offset)?)?;
        offset += stride;
    }
    Ok(())
}
