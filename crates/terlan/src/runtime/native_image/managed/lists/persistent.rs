//! Path-copy operations for actor-local persistent RRB lists.

use super::*;

/// Result of appending beneath one node, including a sibling that did not fit.
struct AppendOutcome {
    node: NodeSummary,
    overflow: Option<NodeSummary>,
}

/// Result of prepending beneath one node, including a sibling that did not fit.
struct PrependOutcome {
    overflow: Option<NodeSummary>,
    node: NodeSummary,
}

/// Prepends one value while sharing every unchanged subtree.
pub(super) fn prepend(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    list: TvmRef<ManagedList>,
    value: ManagedFieldValue,
) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
    let (header, _) = read_root(heap, descriptor, list)?;
    let length = header
        .length
        .checked_add(1)
        .ok_or(ManagedMemoryError::CollectionTooLarge)?;
    validate_element_count(length)?;
    if header.form != FORM_TREE || header.start != 0 {
        let mut elements = heap.list_elements_from(descriptor, list, 0)?;
        elements.insert(0, value);
        return heap.list_from_elements(descriptor, &elements);
    }

    let root = tree_summary(heap, descriptor, list)?;
    let prepended = prepend_node(heap, descriptor, root, value)?;
    let tree = match prepended.overflow {
        Some(overflow) => allocate_internal(heap, descriptor, &[overflow, prepended.node])?,
        None => prepended.node,
    };
    allocate_tree_root(heap, descriptor, tree, 0, length)
}

/// Appends one value while sharing every unchanged subtree.
pub(super) fn append(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    list: TvmRef<ManagedList>,
    value: ManagedFieldValue,
) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
    let (header, _) = read_root(heap, descriptor, list)?;
    let length = header
        .length
        .checked_add(1)
        .ok_or(ManagedMemoryError::CollectionTooLarge)?;
    validate_element_count(length)?;
    if header.form != FORM_TREE || header.start != 0 {
        let mut elements = heap.list_elements_from(descriptor, list, 0)?;
        elements.push(value);
        return heap.list_from_elements(descriptor, &elements);
    }

    let root = tree_summary(heap, descriptor, list)?;
    let appended = append_node(heap, descriptor, root, value)?;
    let tree = match appended.overflow {
        Some(overflow) => allocate_internal(heap, descriptor, &[appended.node, overflow])?,
        None => appended.node,
    };
    allocate_tree_root(heap, descriptor, tree, 0, length)
}

/// Replaces one value by copying only its leaf and ancestor path.
pub(super) fn update(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    list: TvmRef<ManagedList>,
    index: usize,
    value: ManagedFieldValue,
) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
    let (header, _) = read_root(heap, descriptor, list)?;
    if index >= header.length {
        return Err(ManagedMemoryError::CollectionIndexOutOfBounds);
    }
    if header.form != FORM_TREE || header.start != 0 {
        let mut elements = heap.list_elements_from(descriptor, list, 0)?;
        elements[index] = value;
        return heap.list_from_elements(descriptor, &elements);
    }

    let root = tree_summary(heap, descriptor, list)?;
    let tree = update_node(heap, descriptor, root, index, value)?;
    allocate_tree_root(heap, descriptor, tree, 0, header.length)
}

/// Concatenates two lists by rebalancing only their touching RRB fringes.
pub(super) fn concat(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    left: TvmRef<ManagedList>,
    right: TvmRef<ManagedList>,
) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
    let (left_header, _) = read_root(heap, descriptor, left)?;
    let (right_header, _) = read_root(heap, descriptor, right)?;
    let total = left_header
        .length
        .checked_add(right_header.length)
        .ok_or(ManagedMemoryError::CollectionTooLarge)?;
    validate_element_count(total)?;
    if left_header.length == 0 {
        return Ok(right);
    }
    if right_header.length == 0 {
        return Ok(left);
    }
    if total <= INLINE_LIMIT || left_header.start != 0 || right_header.start != 0 {
        return materialized_concat(heap, descriptor, left, right);
    }

    let mut left_node = root_as_node(heap, descriptor, left, left_header)?;
    let mut right_node = root_as_node(heap, descriptor, right, right_header)?;
    while left_node.height < right_node.height {
        left_node = allocate_internal(heap, descriptor, &[left_node])?;
    }
    while right_node.height < left_node.height {
        right_node = allocate_internal(heap, descriptor, &[right_node])?;
    }
    let mut level = concat_same_height(heap, descriptor, left_node, right_node)?;
    while level.len() > 1 {
        level = level
            .chunks(BRANCH_FACTOR)
            .map(|children| allocate_internal(heap, descriptor, children))
            .collect::<Result<Vec<_>, _>>()?;
    }
    allocate_tree_root(heap, descriptor, level[0], 0, total)
}

/// Removes one structural match per removal value while preserving both inputs.
pub(super) fn subtract<F>(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    list: TvmRef<ManagedList>,
    removals: TvmRef<ManagedList>,
    mut equivalent: F,
) -> Result<TvmRef<ManagedList>, ManagedMemoryError>
where
    F: FnMut(&ActorHeap, ManagedFieldValue, ManagedFieldValue) -> Result<bool, ManagedMemoryError>,
{
    let values = heap.list_elements_from(descriptor, list, 0)?;
    let removals = heap.list_elements_from(descriptor, removals, 0)?;
    if values.is_empty() || removals.is_empty() {
        return Ok(list);
    }

    let mut result = values;
    let mut changed = false;
    for removal in removals {
        let mut matched = None;
        for (index, candidate) in result.iter().copied().enumerate() {
            if equivalent(heap, candidate, removal)? {
                matched = Some(index);
                break;
            }
        }
        if let Some(index) = matched {
            result.remove(index);
            changed = true;
        }
    }
    if changed {
        heap.list_from_elements(descriptor, &result)
    } else {
        Ok(list)
    }
}

/// Exchanges two positions by copying the union of their RRB ancestor paths.
pub(super) fn swap(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    list: TvmRef<ManagedList>,
    left: usize,
    right: usize,
) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
    let (header, _) = read_root(heap, descriptor, list)?;
    if left >= header.length || right >= header.length {
        return Err(ManagedMemoryError::CollectionIndexOutOfBounds);
    }
    if left == right {
        return Ok(list);
    }
    if header.form != FORM_TREE || header.start != 0 {
        let mut elements = heap.list_elements_from(descriptor, list, 0)?;
        elements.swap(left, right);
        return heap.list_from_elements(descriptor, &elements);
    }

    let left_value = heap.list_get(descriptor, list, left)?;
    let right_value = heap.list_get(descriptor, list, right)?;
    let root = tree_summary(heap, descriptor, list)?;
    let tree = replace_two(
        heap,
        descriptor,
        root,
        (left, right_value),
        (right, left_value),
    )?;
    allocate_tree_root(heap, descriptor, tree, 0, header.length)
}

/// Appends below one node and returns a same-height overflow sibling when full.
fn append_node(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    node: NodeSummary,
    value: ManagedFieldValue,
) -> Result<AppendOutcome, ManagedMemoryError> {
    let header = read_node(heap, descriptor, node.reference)?;
    if header.kind == NODE_LEAF {
        if header.count == BRANCH_FACTOR {
            return Ok(AppendOutcome {
                node,
                overflow: Some(allocate_leaf(heap, descriptor, &[value])?),
            });
        }
        let mut elements = leaf_elements(heap, descriptor, node)?;
        elements.push(value);
        return Ok(AppendOutcome {
            node: allocate_leaf(heap, descriptor, &elements)?,
            overflow: None,
        });
    }

    let mut children = child_summaries(heap, descriptor, node)?;
    let last = children
        .pop()
        .ok_or(ManagedMemoryError::CorruptedCollection)?;
    let appended = append_node(heap, descriptor, last, value)?;
    children.push(appended.node);
    match appended.overflow {
        None => Ok(AppendOutcome {
            node: allocate_internal(heap, descriptor, &children)?,
            overflow: None,
        }),
        Some(overflow) if children.len() < BRANCH_FACTOR => {
            children.push(overflow);
            Ok(AppendOutcome {
                node: allocate_internal(heap, descriptor, &children)?,
                overflow: None,
            })
        }
        Some(overflow) => {
            let current = if appended.node.reference == last.reference {
                node
            } else {
                allocate_internal(heap, descriptor, &children)?
            };
            Ok(AppendOutcome {
                node: current,
                overflow: Some(lift_to_height(heap, descriptor, overflow, node.height)?),
            })
        }
    }
}

/// Prepends below one node and returns a same-height overflow sibling when full.
fn prepend_node(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    node: NodeSummary,
    value: ManagedFieldValue,
) -> Result<PrependOutcome, ManagedMemoryError> {
    let header = read_node(heap, descriptor, node.reference)?;
    if header.kind == NODE_LEAF {
        if header.count == BRANCH_FACTOR {
            return Ok(PrependOutcome {
                overflow: Some(allocate_leaf(heap, descriptor, &[value])?),
                node,
            });
        }
        let mut elements = leaf_elements(heap, descriptor, node)?;
        elements.insert(0, value);
        return Ok(PrependOutcome {
            overflow: None,
            node: allocate_leaf(heap, descriptor, &elements)?,
        });
    }

    let mut children = child_summaries(heap, descriptor, node)?;
    if children.is_empty() {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let first = children.remove(0);
    let prepended = prepend_node(heap, descriptor, first, value)?;
    children.insert(0, prepended.node);
    match prepended.overflow {
        None => Ok(PrependOutcome {
            overflow: None,
            node: allocate_internal(heap, descriptor, &children)?,
        }),
        Some(overflow) if children.len() < BRANCH_FACTOR => {
            children.insert(0, overflow);
            Ok(PrependOutcome {
                overflow: None,
                node: allocate_internal(heap, descriptor, &children)?,
            })
        }
        Some(overflow) => {
            let current = if prepended.node.reference == first.reference {
                node
            } else {
                allocate_internal(heap, descriptor, &children)?
            };
            Ok(PrependOutcome {
                overflow: Some(lift_to_height(heap, descriptor, overflow, node.height)?),
                node: current,
            })
        }
    }
}

/// Replaces one node value and rebuilds only the selected ancestor path.
fn update_node(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    node: NodeSummary,
    index: usize,
    value: ManagedFieldValue,
) -> Result<NodeSummary, ManagedMemoryError> {
    if index >= node.total {
        return Err(ManagedMemoryError::CollectionIndexOutOfBounds);
    }
    if node.height == 0 {
        let mut elements = leaf_elements(heap, descriptor, node)?;
        elements[index] = value;
        return allocate_leaf(heap, descriptor, &elements);
    }

    let mut children = child_summaries(heap, descriptor, node)?;
    let mut prior = 0;
    let child_index = children
        .iter()
        .position(|child| {
            let end = prior + child.total;
            let selected = index < end;
            if !selected {
                prior = end;
            }
            selected
        })
        .ok_or(ManagedMemoryError::CorruptedCollection)?;
    children[child_index] = update_node(
        heap,
        descriptor,
        children[child_index],
        index - prior,
        value,
    )?;
    allocate_internal(heap, descriptor, &children)
}

/// Replaces two positions while allocating each shared ancestor only once.
fn replace_two(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    node: NodeSummary,
    first: (usize, ManagedFieldValue),
    second: (usize, ManagedFieldValue),
) -> Result<NodeSummary, ManagedMemoryError> {
    if first.0 >= node.total || second.0 >= node.total {
        return Err(ManagedMemoryError::CollectionIndexOutOfBounds);
    }
    if node.height == 0 {
        let mut elements = leaf_elements(heap, descriptor, node)?;
        elements[first.0] = first.1;
        elements[second.0] = second.1;
        return allocate_leaf(heap, descriptor, &elements);
    }

    let mut children = child_summaries(heap, descriptor, node)?;
    let (first_child, first_start) = locate_child(&children, first.0)?;
    let (second_child, second_start) = locate_child(&children, second.0)?;
    if first_child == second_child {
        children[first_child] = replace_two(
            heap,
            descriptor,
            children[first_child],
            (first.0 - first_start, first.1),
            (second.0 - second_start, second.1),
        )?;
    } else {
        children[first_child] = update_node(
            heap,
            descriptor,
            children[first_child],
            first.0 - first_start,
            first.1,
        )?;
        children[second_child] = update_node(
            heap,
            descriptor,
            children[second_child],
            second.0 - second_start,
            second.1,
        )?;
    }
    allocate_internal(heap, descriptor, &children)
}

/// Locates one logical index among a node's direct child summaries.
fn locate_child(
    children: &[NodeSummary],
    index: usize,
) -> Result<(usize, usize), ManagedMemoryError> {
    let mut start = 0_usize;
    for (child_index, child) in children.iter().enumerate() {
        let end = start
            .checked_add(child.total)
            .ok_or(ManagedMemoryError::CorruptedCollection)?;
        if index < end {
            return Ok((child_index, start));
        }
        start = end;
    }
    Err(ManagedMemoryError::CorruptedCollection)
}

/// Merges two equal-height RRB nodes into one or two balanced nodes.
fn concat_same_height(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    left: NodeSummary,
    right: NodeSummary,
) -> Result<Vec<NodeSummary>, ManagedMemoryError> {
    if left.height != right.height {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    if left.height == 0 {
        let mut elements = leaf_elements(heap, descriptor, left)?;
        elements.extend(leaf_elements(heap, descriptor, right)?);
        return elements
            .chunks(BRANCH_FACTOR)
            .map(|chunk| allocate_leaf(heap, descriptor, chunk))
            .collect();
    }

    let mut left_children = child_summaries(heap, descriptor, left)?;
    let mut right_children = child_summaries(heap, descriptor, right)?;
    let left_fringe = left_children
        .pop()
        .ok_or(ManagedMemoryError::CorruptedCollection)?;
    let right_fringe = right_children
        .first()
        .copied()
        .ok_or(ManagedMemoryError::CorruptedCollection)?;
    right_children.remove(0);
    let middle = concat_same_height(heap, descriptor, left_fringe, right_fringe)?;
    left_children.extend(middle);
    left_children.extend(right_children);
    left_children
        .chunks(BRANCH_FACTOR)
        .map(|children| allocate_internal(heap, descriptor, children))
        .collect()
}

/// Converts an inline or tree root into one private node summary.
fn root_as_node(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    list: TvmRef<ManagedList>,
    header: RootHeader,
) -> Result<NodeSummary, ManagedMemoryError> {
    if header.form == FORM_INLINE {
        let elements = heap.list_elements_from(descriptor, list, 0)?;
        allocate_leaf(heap, descriptor, &elements)
    } else if header.form == FORM_TREE {
        tree_summary(heap, descriptor, list)
    } else {
        Err(ManagedMemoryError::CorruptedCollection)
    }
}

/// Reads the private node summary owned by one tree root.
fn tree_summary(
    heap: &ActorHeap,
    descriptor: &ManagedListDescriptor,
    list: TvmRef<ManagedList>,
) -> Result<NodeSummary, ManagedMemoryError> {
    let reference = heap.reference_field(list, TREE_REFERENCE_OFFSET)?.cast();
    node_summary(heap, descriptor, reference)
}

/// Drops one bounded prefix by path-copying only the touched left fringe.
///
/// List-pattern traversal advances a root cursor for most elements. At a leaf
/// boundary this removes the now-unreachable prefix in O(log n), so a linear
/// head/tail walk neither retains the complete source tree nor materializes
/// every remaining suffix.
pub(super) fn trim_prefix(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    list: TvmRef<ManagedList>,
    count: usize,
) -> Result<Option<NodeSummary>, ManagedMemoryError> {
    let root = tree_summary(heap, descriptor, list)?;
    trim_node_prefix(heap, descriptor, root, count)
}

fn trim_node_prefix(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    node: NodeSummary,
    count: usize,
) -> Result<Option<NodeSummary>, ManagedMemoryError> {
    if count == 0 {
        return Ok(Some(node));
    }
    if count > node.total {
        return Err(ManagedMemoryError::CollectionIndexOutOfBounds);
    }
    if count == node.total {
        return Ok(None);
    }
    let header = read_node(heap, descriptor, node.reference)?;
    if header.kind == NODE_LEAF {
        let elements = leaf_elements(heap, descriptor, node)?;
        return allocate_leaf(heap, descriptor, &elements[count..]).map(Some);
    }

    let children = child_summaries(heap, descriptor, node)?;
    let mut consumed = 0_usize;
    let mut retained = Vec::with_capacity(children.len());
    for child in children {
        if consumed >= count {
            retained.push(child);
            continue;
        }
        let next = consumed
            .checked_add(child.total)
            .ok_or(ManagedMemoryError::CollectionTooLarge)?;
        if next <= count {
            consumed = next;
            continue;
        }
        let partial = trim_node_prefix(heap, descriptor, child, count - consumed)?
            .ok_or(ManagedMemoryError::CorruptedCollection)?;
        let partial = lift_to_height(heap, descriptor, partial, child.height)?;
        retained.push(partial);
        consumed = count;
    }
    if consumed != count || retained.is_empty() {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    if retained.len() == 1 {
        Ok(retained.first().copied())
    } else {
        allocate_internal(heap, descriptor, &retained).map(Some)
    }
}

/// Reads one validated summary from a private RRB node.
fn node_summary(
    heap: &ActorHeap,
    descriptor: &ManagedListDescriptor,
    reference: TvmRef<RrbNode>,
) -> Result<NodeSummary, ManagedMemoryError> {
    let header = read_node(heap, descriptor, reference)?;
    Ok(NodeSummary {
        reference,
        total: header.total,
        height: header.height,
        relaxed: header.kind == NODE_RELAXED || header.relaxed_descendant,
    })
}

/// Reads all direct child summaries from one validated internal node.
fn child_summaries(
    heap: &ActorHeap,
    descriptor: &ManagedListDescriptor,
    node: NodeSummary,
) -> Result<Vec<NodeSummary>, ManagedMemoryError> {
    let header = read_node(heap, descriptor, node.reference)?;
    if header.kind == NODE_LEAF || header.height != node.height {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    (0..header.count)
        .map(|index| {
            let reference = heap
                .reference_field(node.reference, NODE_HEADER_BYTES + index * 8)?
                .cast();
            node_summary(heap, descriptor, reference)
        })
        .collect()
}

/// Reads all values from one validated leaf into a bounded fringe buffer.
fn leaf_elements(
    heap: &ActorHeap,
    descriptor: &ManagedListDescriptor,
    leaf: NodeSummary,
) -> Result<Vec<ManagedFieldValue>, ManagedMemoryError> {
    let header = read_node(heap, descriptor, leaf.reference)?;
    if header.kind != NODE_LEAF || leaf.height != 0 {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    (0..header.count)
        .map(|index| node_get(heap, descriptor, leaf.reference, index))
        .collect()
}

/// Wraps a node in singleton parents until it reaches the requested height.
fn lift_to_height(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    mut node: NodeSummary,
    height: u8,
) -> Result<NodeSummary, ManagedMemoryError> {
    while node.height < height {
        node = allocate_internal(heap, descriptor, &[node])?;
    }
    if node.height == height {
        Ok(node)
    } else {
        Err(ManagedMemoryError::CorruptedCollection)
    }
}

/// Materializes concatenation when either operand is a tiny root or front view.
fn materialized_concat(
    heap: &mut ActorHeap,
    descriptor: &ManagedListDescriptor,
    left: TvmRef<ManagedList>,
    right: TvmRef<ManagedList>,
) -> Result<TvmRef<ManagedList>, ManagedMemoryError> {
    let mut elements = heap.list_elements_from(descriptor, left, 0)?;
    elements.extend(heap.list_elements_from(descriptor, right, 0)?);
    heap.list_from_elements(descriptor, &elements)
}
