//! Managed persistent A-CHAMP index nodes.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::runtime::map_layout::{
    select_achamp_node_layout, AChampNodeLayout, AChampSubtreeHint, CHAMP_BRANCH_FACTOR,
    COMPRESSED_PATH_MIN_SKIPPED_LEVELS, LEAF_BLOCK_LIMIT,
};

use super::{decode_entry, ManagedKeySemantics, ManagedMapDescriptor, MapEntry};
use crate::runtime::native_image::managed::{
    ActorHeap, AllocationClass, ManagedFieldValue, ManagedMemoryError, ManagedTypeDescriptor,
    TvmRef,
};

const NODE_MAGIC: u32 = 0x3148_4341;
const NODE_HEADER_BYTES: usize = 24;
const LEAF_ITEM_BYTES: usize = 16;
const LEAF_ENTRY_OFFSET: usize = 8;
const DENSE_TABLE_BYTES: usize = CHAMP_BRANCH_FACTOR;
const DENSE_CHILDREN_OFFSET: usize = NODE_HEADER_BYTES + DENSE_TABLE_BYTES;
const COMPRESSED_PREFIX_BYTES: usize = 13;
const COMPRESSED_CHILD_OFFSET: usize = 40;
const MAX_HASH_LEVELS: usize = 13;
const KIND_LEAF: u8 = 0;
const KIND_COLLISION: u8 = 1;
const KIND_SPARSE: u8 = 2;
const KIND_DENSE: u8 = 3;
const KIND_COMPRESSED: u8 = 4;
const EMPTY_DENSE_SLOT: u8 = u8::MAX;

/// Opaque marker for one private actor-heap A-CHAMP node.
#[derive(Debug)]
pub(super) struct AChampNode;

/// One stable hash and private map-entry reference indexed by A-CHAMP.
#[derive(Clone, Copy, Debug)]
pub(super) struct IndexedEntry {
    pub(super) hash: u64,
    pub(super) entry: TvmRef<MapEntry>,
}

/// Decoded node header common to every adaptive representation.
#[derive(Clone, Copy, Debug)]
struct NodeHeader {
    kind: u8,
    count: usize,
    total: usize,
    metadata: u64,
}

/// Builds one canonical A-CHAMP subtree from nonempty indexed entries.
pub(super) fn build(
    heap: &mut ActorHeap,
    descriptor: &ManagedMapDescriptor,
    entries: &[IndexedEntry],
    level: usize,
) -> Result<TvmRef<AChampNode>, ManagedMemoryError> {
    if entries.is_empty() || level > MAX_HASH_LEVELS {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    if entries.len() <= LEAF_BLOCK_LIMIT {
        return allocate_leaf(heap, descriptor, KIND_LEAF, entries);
    }
    if entries
        .first()
        .is_some_and(|first| entries.iter().all(|entry| entry.hash == first.hash))
    {
        return allocate_leaf(heap, descriptor, KIND_COLLISION, entries);
    }
    let prefix = shared_slot_prefix(entries, level);
    if prefix.len() >= COMPRESSED_PATH_MIN_SKIPPED_LEVELS {
        let child = build(heap, descriptor, entries, level + prefix.len())?;
        return allocate_compressed(heap, descriptor, &prefix, child, entries.len());
    }
    let groups = group_by_slot(entries, level)?;
    let mut children = Vec::with_capacity(groups.len());
    for (slot, group) in groups {
        children.push((slot, build(heap, descriptor, &group, level + 1)?));
    }
    allocate_children(heap, descriptor, &children, entries.len())
}

/// Looks up one structurally equal key through its stable hash path.
pub(super) fn lookup<S: ManagedKeySemantics>(
    heap: &ActorHeap,
    descriptor: &ManagedMapDescriptor,
    node: TvmRef<AChampNode>,
    hash: u64,
    key: ManagedFieldValue,
    semantics: &mut S,
) -> Result<Option<TvmRef<MapEntry>>, ManagedMemoryError> {
    lookup_at(heap, descriptor, node, hash, key, semantics, 0)
}

/// Replaces one exact indexed entry through path-copy allocation.
pub(super) fn replace(
    heap: &mut ActorHeap,
    descriptor: &ManagedMapDescriptor,
    node: TvmRef<AChampNode>,
    hash: u64,
    previous: TvmRef<MapEntry>,
    replacement: TvmRef<MapEntry>,
    level: usize,
) -> Result<TvmRef<AChampNode>, ManagedMemoryError> {
    let header = read_header(heap, descriptor, node)?;
    match header.kind {
        KIND_LEAF | KIND_COLLISION => {
            let mut entries = read_leaf_entries(heap, descriptor, node, header)?;
            let Some(item) = entries.iter_mut().find(|item| item.entry == previous) else {
                return Err(ManagedMemoryError::CorruptedCollection);
            };
            item.entry = replacement;
            allocate_leaf(heap, descriptor, header.kind, &entries)
        }
        KIND_SPARSE | KIND_DENSE => {
            let mut children = read_children(heap, descriptor, node, header)?;
            let slot = hash_slot(hash, level)?;
            let Some((_, child)) = children
                .iter_mut()
                .find(|(child_slot, _)| *child_slot == slot)
            else {
                return Err(ManagedMemoryError::CorruptedCollection);
            };
            *child = replace(
                heap,
                descriptor,
                *child,
                hash,
                previous,
                replacement,
                level + 1,
            )?;
            allocate_children(heap, descriptor, &children, header.total)
        }
        KIND_COMPRESSED => {
            let (prefix, child) = read_compressed(heap, descriptor, node, header)?;
            validate_prefix(&prefix, hash, level)?;
            let replacement_child = replace(
                heap,
                descriptor,
                child,
                hash,
                previous,
                replacement,
                level + prefix.len(),
            )?;
            allocate_compressed(heap, descriptor, &prefix, replacement_child, header.total)
        }
        _ => Err(ManagedMemoryError::CorruptedCollection),
    }
}

/// Inserts one absent indexed entry through path-copy allocation.
pub(super) fn insert(
    heap: &mut ActorHeap,
    descriptor: &ManagedMapDescriptor,
    node: TvmRef<AChampNode>,
    hash: u64,
    entry: TvmRef<MapEntry>,
    level: usize,
) -> Result<TvmRef<AChampNode>, ManagedMemoryError> {
    let header = read_header(heap, descriptor, node)?;
    match header.kind {
        KIND_LEAF | KIND_COLLISION => {
            let mut entries = read_leaf_entries(heap, descriptor, node, header)?;
            entries.push(IndexedEntry { hash, entry });
            build(heap, descriptor, &entries, level)
        }
        KIND_SPARSE | KIND_DENSE => {
            let mut children = read_children(heap, descriptor, node, header)?;
            let slot = hash_slot(hash, level)?;
            if let Some((_, child)) = children
                .iter_mut()
                .find(|(child_slot, _)| *child_slot == slot)
            {
                *child = insert(heap, descriptor, *child, hash, entry, level + 1)?;
            } else {
                let child =
                    allocate_leaf(heap, descriptor, KIND_LEAF, &[IndexedEntry { hash, entry }])?;
                children.push((slot, child));
                children.sort_by_key(|(child_slot, _)| *child_slot);
            }
            allocate_children(heap, descriptor, &children, header.total + 1)
        }
        KIND_COMPRESSED => {
            let (prefix, child) = read_compressed(heap, descriptor, node, header)?;
            if prefix_matches(&prefix, hash, level)? {
                let child = insert(heap, descriptor, child, hash, entry, level + prefix.len())?;
                allocate_compressed(heap, descriptor, &prefix, child, header.total + 1)
            } else {
                let mut entries = collect_entries(heap, descriptor, node)?;
                entries.push(IndexedEntry { hash, entry });
                build(heap, descriptor, &entries, level)
            }
        }
        _ => Err(ManagedMemoryError::CorruptedCollection),
    }
}

/// Removes one exact indexed entry and returns the optional remaining subtree.
pub(super) fn remove(
    heap: &mut ActorHeap,
    descriptor: &ManagedMapDescriptor,
    node: TvmRef<AChampNode>,
    hash: u64,
    entry: TvmRef<MapEntry>,
    level: usize,
) -> Result<Option<TvmRef<AChampNode>>, ManagedMemoryError> {
    let header = read_header(heap, descriptor, node)?;
    match header.kind {
        KIND_LEAF | KIND_COLLISION => {
            let mut entries = read_leaf_entries(heap, descriptor, node, header)?;
            let before = entries.len();
            entries.retain(|item| item.entry != entry);
            if entries.len() == before {
                return Err(ManagedMemoryError::CorruptedCollection);
            }
            if entries.is_empty() {
                Ok(None)
            } else {
                build(heap, descriptor, &entries, level).map(Some)
            }
        }
        KIND_SPARSE | KIND_DENSE => {
            let mut children = read_children(heap, descriptor, node, header)?;
            let slot = hash_slot(hash, level)?;
            let Some(index) = children
                .iter()
                .position(|(child_slot, _)| *child_slot == slot)
            else {
                return Err(ManagedMemoryError::CorruptedCollection);
            };
            match remove(heap, descriptor, children[index].1, hash, entry, level + 1)? {
                Some(child) => children[index].1 = child,
                None => {
                    children.remove(index);
                }
            }
            if children.is_empty() {
                return Ok(None);
            }
            let total = header.total - 1;
            if total <= LEAF_BLOCK_LIMIT {
                let mut entries = Vec::with_capacity(total);
                for (_, child) in &children {
                    entries.extend(collect_entries(heap, descriptor, *child)?);
                }
                return build(heap, descriptor, &entries, level).map(Some);
            }
            allocate_children(heap, descriptor, &children, total).map(Some)
        }
        KIND_COMPRESSED => {
            let (prefix, child) = read_compressed(heap, descriptor, node, header)?;
            validate_prefix(&prefix, hash, level)?;
            let Some(child) = remove(heap, descriptor, child, hash, entry, level + prefix.len())?
            else {
                return Ok(None);
            };
            allocate_compressed(heap, descriptor, &prefix, child, header.total - 1).map(Some)
        }
        _ => Err(ManagedMemoryError::CorruptedCollection),
    }
}

/// Traverses one node at a known hash-fragment level.
fn lookup_at<S: ManagedKeySemantics>(
    heap: &ActorHeap,
    descriptor: &ManagedMapDescriptor,
    node: TvmRef<AChampNode>,
    hash: u64,
    key: ManagedFieldValue,
    semantics: &mut S,
    level: usize,
) -> Result<Option<TvmRef<MapEntry>>, ManagedMemoryError> {
    let header = read_header(heap, descriptor, node)?;
    match header.kind {
        KIND_LEAF | KIND_COLLISION => {
            for item in read_leaf_entries(heap, descriptor, node, header)? {
                if item.hash != hash {
                    continue;
                }
                let (candidate, _) = decode_entry(heap, descriptor, item.entry)?;
                if semantics.equivalent(heap, candidate, key)? {
                    return Ok(Some(item.entry));
                }
            }
            Ok(None)
        }
        KIND_SPARSE | KIND_DENSE => {
            let slot = hash_slot(hash, level)?;
            let children = read_children(heap, descriptor, node, header)?;
            let Some((_, child)) = children
                .into_iter()
                .find(|(child_slot, _)| *child_slot == slot)
            else {
                return Ok(None);
            };
            lookup_at(heap, descriptor, child, hash, key, semantics, level + 1)
        }
        KIND_COMPRESSED => {
            let (prefix, child) = read_compressed(heap, descriptor, node, header)?;
            if !prefix_matches(&prefix, hash, level)? {
                return Ok(None);
            }
            lookup_at(
                heap,
                descriptor,
                child,
                hash,
                key,
                semantics,
                level + prefix.len(),
            )
        }
        _ => Err(ManagedMemoryError::CorruptedCollection),
    }
}

/// Allocates a compact leaf or full-hash collision node.
fn allocate_leaf(
    heap: &mut ActorHeap,
    descriptor: &ManagedMapDescriptor,
    kind: u8,
    entries: &[IndexedEntry],
) -> Result<TvmRef<AChampNode>, ManagedMemoryError> {
    if entries.is_empty()
        || kind == KIND_LEAF && entries.len() > LEAF_BLOCK_LIMIT
        || kind == KIND_COLLISION && !entries.iter().all(|entry| entry.hash == entries[0].hash)
    {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let size = NODE_HEADER_BYTES
        .checked_add(
            LEAF_ITEM_BYTES
                .checked_mul(entries.len())
                .ok_or(ManagedMemoryError::CollectionTooLarge)?,
        )
        .ok_or(ManagedMemoryError::CollectionTooLarge)?;
    let mut payload = vec![0; size];
    write_header(&mut payload, kind, entries.len(), entries.len(), 0)?;
    let mut references = Vec::with_capacity(entries.len());
    for (index, item) in entries.iter().enumerate() {
        let base = NODE_HEADER_BYTES + index * LEAF_ITEM_BYTES;
        payload[base..base + 8].copy_from_slice(&item.hash.to_le_bytes());
        references.push((base + LEAF_ENTRY_OFFSET, item.entry.erase()));
    }
    allocate_node(heap, descriptor, kind, size, references, &payload)
}

/// Allocates the sparse or dense representation selected by child occupancy.
fn allocate_children(
    heap: &mut ActorHeap,
    descriptor: &ManagedMapDescriptor,
    children: &[(usize, TvmRef<AChampNode>)],
    total: usize,
) -> Result<TvmRef<AChampNode>, ManagedMemoryError> {
    if children.is_empty()
        || children.len() > CHAMP_BRANCH_FACTOR
        || children.windows(2).any(|pair| pair[0].0 >= pair[1].0)
    {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let layout = select_achamp_node_layout(AChampSubtreeHint {
        entry_count: total,
        occupied_slots: children.len(),
        shared_prefix_levels: 0,
        full_hash_collision: false,
    });
    match layout {
        AChampNodeLayout::DenseNode => allocate_dense(heap, descriptor, children, total),
        AChampNodeLayout::SparseNode => allocate_sparse(heap, descriptor, children, total),
        _ => Err(ManagedMemoryError::CorruptedCollection),
    }
}

/// Allocates one bitmap-indexed sparse child node.
fn allocate_sparse(
    heap: &mut ActorHeap,
    descriptor: &ManagedMapDescriptor,
    children: &[(usize, TvmRef<AChampNode>)],
    total: usize,
) -> Result<TvmRef<AChampNode>, ManagedMemoryError> {
    let size = NODE_HEADER_BYTES + children.len() * 8;
    let bitmap = children.iter().try_fold(0_u32, |bitmap, (slot, _)| {
        let shift = u32::try_from(*slot).map_err(|_| ManagedMemoryError::CorruptedCollection)?;
        Ok::<_, ManagedMemoryError>(bitmap | (1_u32 << shift))
    })?;
    let mut payload = vec![0; size];
    write_header(
        &mut payload,
        KIND_SPARSE,
        children.len(),
        total,
        u64::from(bitmap),
    )?;
    let references = children
        .iter()
        .enumerate()
        .map(|(index, (_, child))| (NODE_HEADER_BYTES + index * 8, child.erase()))
        .collect::<Vec<_>>();
    allocate_node(heap, descriptor, KIND_SPARSE, size, references, &payload)
}

/// Allocates one direct-slot dense child node with packed precise references.
fn allocate_dense(
    heap: &mut ActorHeap,
    descriptor: &ManagedMapDescriptor,
    children: &[(usize, TvmRef<AChampNode>)],
    total: usize,
) -> Result<TvmRef<AChampNode>, ManagedMemoryError> {
    let size = DENSE_CHILDREN_OFFSET + children.len() * 8;
    let mut payload = vec![EMPTY_DENSE_SLOT; size];
    payload[DENSE_CHILDREN_OFFSET..].fill(0);
    write_header(&mut payload, KIND_DENSE, children.len(), total, 0)?;
    for (index, (slot, _)) in children.iter().enumerate() {
        payload[NODE_HEADER_BYTES + slot] =
            u8::try_from(index).map_err(|_| ManagedMemoryError::CorruptedCollection)?;
    }
    let references = children
        .iter()
        .enumerate()
        .map(|(index, (_, child))| (DENSE_CHILDREN_OFFSET + index * 8, child.erase()))
        .collect::<Vec<_>>();
    allocate_node(heap, descriptor, KIND_DENSE, size, references, &payload)
}

/// Allocates one compressed sequence of shared hash slots and its only child.
fn allocate_compressed(
    heap: &mut ActorHeap,
    descriptor: &ManagedMapDescriptor,
    prefix: &[usize],
    child: TvmRef<AChampNode>,
    total: usize,
) -> Result<TvmRef<AChampNode>, ManagedMemoryError> {
    if prefix.len() < COMPRESSED_PATH_MIN_SKIPPED_LEVELS || prefix.len() > COMPRESSED_PREFIX_BYTES {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let mut payload = vec![0; COMPRESSED_CHILD_OFFSET + 8];
    write_header(&mut payload, KIND_COMPRESSED, prefix.len(), total, 0)?;
    for (index, slot) in prefix.iter().enumerate() {
        payload[NODE_HEADER_BYTES + index] =
            u8::try_from(*slot).map_err(|_| ManagedMemoryError::CorruptedCollection)?;
    }
    allocate_node(
        heap,
        descriptor,
        KIND_COMPRESSED,
        payload.len(),
        vec![(COMPRESSED_CHILD_OFFSET, child.erase())],
        &payload,
    )
}

/// Allocates one node with a representation-specific precise reference map.
fn allocate_node(
    heap: &mut ActorHeap,
    descriptor: &ManagedMapDescriptor,
    kind: u8,
    size: usize,
    references: Vec<(usize, TvmRef<()>)>,
    payload: &[u8],
) -> Result<TvmRef<AChampNode>, ManagedMemoryError> {
    let representation = [b'A', kind];
    heap.allocate(
        Arc::new(ManagedTypeDescriptor::new_specialized(
            descriptor.node_semantic_id(),
            size,
            8,
            references.iter().map(|item| item.0).collect(),
            AllocationClass::Young,
            &representation,
        )?),
        payload,
        &references,
    )
}

/// Validates and decodes one node header.
fn read_header(
    heap: &ActorHeap,
    descriptor: &ManagedMapDescriptor,
    node: TvmRef<AChampNode>,
) -> Result<NodeHeader, ManagedMemoryError> {
    if heap.descriptor(node)?.semantic_id() != descriptor.node_semantic_id() {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let payload = heap.read(node)?;
    if read_u32(payload, 0)? != NODE_MAGIC {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let kind = *payload
        .get(4)
        .ok_or(ManagedMemoryError::CorruptedCollection)?;
    let count = usize::from(read_u16(payload, 6)?);
    let total = read_usize(payload, 8)?;
    let metadata = read_u64(payload, 16)?;
    if total == 0
        || !matches!(
            kind,
            KIND_LEAF | KIND_COLLISION | KIND_SPARSE | KIND_DENSE | KIND_COMPRESSED
        )
        || matches!(kind, KIND_LEAF | KIND_COLLISION) && count != total
        || kind == KIND_LEAF && count > LEAF_BLOCK_LIMIT
        || matches!(kind, KIND_SPARSE | KIND_DENSE) && (count == 0 || count > CHAMP_BRANCH_FACTOR)
        || kind == KIND_COMPRESSED
            && !(COMPRESSED_PATH_MIN_SKIPPED_LEVELS..=COMPRESSED_PREFIX_BYTES).contains(&count)
    {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    Ok(NodeHeader {
        kind,
        count,
        total,
        metadata,
    })
}

/// Decodes all entries retained directly by a leaf or collision node.
fn read_leaf_entries(
    heap: &ActorHeap,
    descriptor: &ManagedMapDescriptor,
    node: TvmRef<AChampNode>,
    header: NodeHeader,
) -> Result<Vec<IndexedEntry>, ManagedMemoryError> {
    if !matches!(header.kind, KIND_LEAF | KIND_COLLISION) {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let payload = heap.read(node)?;
    if payload.len() != NODE_HEADER_BYTES + header.count * LEAF_ITEM_BYTES {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let mut entries = Vec::with_capacity(header.count);
    for index in 0..header.count {
        let base = NODE_HEADER_BYTES + index * LEAF_ITEM_BYTES;
        let hash = read_u64(payload, base)?;
        let entry = heap.reference_field(node, base + LEAF_ENTRY_OFFSET)?.cast();
        if heap.descriptor(entry)?.semantic_id() != descriptor.entry_semantic_id() {
            return Err(ManagedMemoryError::CorruptedCollection);
        }
        entries.push(IndexedEntry { hash, entry });
    }
    if header.kind == KIND_COLLISION
        && entries
            .first()
            .is_some_and(|first| entries.iter().any(|entry| entry.hash != first.hash))
    {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    Ok(entries)
}

/// Decodes sparse or dense children in ascending hash-slot order.
fn read_children(
    heap: &ActorHeap,
    _descriptor: &ManagedMapDescriptor,
    node: TvmRef<AChampNode>,
    header: NodeHeader,
) -> Result<Vec<(usize, TvmRef<AChampNode>)>, ManagedMemoryError> {
    let payload = heap.read(node)?;
    match header.kind {
        KIND_SPARSE => {
            if payload.len() != NODE_HEADER_BYTES + header.count * 8 {
                return Err(ManagedMemoryError::CorruptedCollection);
            }
            let bitmap = u32::try_from(header.metadata)
                .map_err(|_| ManagedMemoryError::CorruptedCollection)?;
            if bitmap.count_ones() as usize != header.count {
                return Err(ManagedMemoryError::CorruptedCollection);
            }
            let mut children = Vec::with_capacity(header.count);
            for slot in 0..CHAMP_BRANCH_FACTOR {
                if bitmap & (1_u32 << slot) != 0 {
                    let index = children.len();
                    children.push((
                        slot,
                        heap.reference_field(node, NODE_HEADER_BYTES + index * 8)?
                            .cast(),
                    ));
                }
            }
            Ok(children)
        }
        KIND_DENSE => {
            if payload.len() != DENSE_CHILDREN_OFFSET + header.count * 8 {
                return Err(ManagedMemoryError::CorruptedCollection);
            }
            let mut children = Vec::with_capacity(header.count);
            for slot in 0..CHAMP_BRANCH_FACTOR {
                let index = payload[NODE_HEADER_BYTES + slot];
                if index == EMPTY_DENSE_SLOT {
                    continue;
                }
                let index = usize::from(index);
                if index >= header.count {
                    return Err(ManagedMemoryError::CorruptedCollection);
                }
                children.push((
                    slot,
                    heap.reference_field(node, DENSE_CHILDREN_OFFSET + index * 8)?
                        .cast(),
                ));
            }
            if children.len() != header.count {
                return Err(ManagedMemoryError::CorruptedCollection);
            }
            Ok(children)
        }
        _ => Err(ManagedMemoryError::CorruptedCollection),
    }
}

/// Decodes one compressed prefix and child reference.
fn read_compressed(
    heap: &ActorHeap,
    _descriptor: &ManagedMapDescriptor,
    node: TvmRef<AChampNode>,
    header: NodeHeader,
) -> Result<(Vec<usize>, TvmRef<AChampNode>), ManagedMemoryError> {
    let payload = heap.read(node)?;
    if header.kind != KIND_COMPRESSED || payload.len() != COMPRESSED_CHILD_OFFSET + 8 {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    let prefix = payload[NODE_HEADER_BYTES..NODE_HEADER_BYTES + header.count]
        .iter()
        .map(|slot| usize::from(*slot))
        .collect::<Vec<_>>();
    if prefix.iter().any(|slot| *slot >= CHAMP_BRANCH_FACTOR) {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    Ok((
        prefix,
        heap.reference_field(node, COMPRESSED_CHILD_OFFSET)?.cast(),
    ))
}

/// Collects every indexed entry retained by one subtree.
fn collect_entries(
    heap: &ActorHeap,
    descriptor: &ManagedMapDescriptor,
    node: TvmRef<AChampNode>,
) -> Result<Vec<IndexedEntry>, ManagedMemoryError> {
    let header = read_header(heap, descriptor, node)?;
    match header.kind {
        KIND_LEAF | KIND_COLLISION => read_leaf_entries(heap, descriptor, node, header),
        KIND_SPARSE | KIND_DENSE => {
            let mut entries = Vec::with_capacity(header.total);
            for (_, child) in read_children(heap, descriptor, node, header)? {
                entries.extend(collect_entries(heap, descriptor, child)?);
            }
            Ok(entries)
        }
        KIND_COMPRESSED => {
            let (_, child) = read_compressed(heap, descriptor, node, header)?;
            collect_entries(heap, descriptor, child)
        }
        _ => Err(ManagedMemoryError::CorruptedCollection),
    }
}

/// Groups indexed entries by one five-bit hash fragment.
fn group_by_slot(
    entries: &[IndexedEntry],
    level: usize,
) -> Result<BTreeMap<usize, Vec<IndexedEntry>>, ManagedMemoryError> {
    let mut groups = BTreeMap::new();
    for entry in entries {
        groups
            .entry(hash_slot(entry.hash, level)?)
            .or_insert_with(Vec::new)
            .push(*entry);
    }
    Ok(groups)
}

/// Returns the consecutive shared hash-slot prefix at one subtree level.
fn shared_slot_prefix(entries: &[IndexedEntry], level: usize) -> Vec<usize> {
    let mut prefix = Vec::new();
    for current in level..MAX_HASH_LEVELS {
        let Some(first) = entries.first() else {
            break;
        };
        let Ok(slot) = hash_slot(first.hash, current) else {
            break;
        };
        if entries
            .iter()
            .all(|entry| hash_slot(entry.hash, current) == Ok(slot))
        {
            prefix.push(slot);
        } else {
            break;
        }
    }
    prefix
}

/// Computes one five-bit A-CHAMP hash slot.
fn hash_slot(hash: u64, level: usize) -> Result<usize, ManagedMemoryError> {
    let shift = level
        .checked_mul(5)
        .ok_or(ManagedMemoryError::CorruptedCollection)?;
    if shift >= u64::BITS as usize {
        return Err(ManagedMemoryError::CorruptedCollection);
    }
    Ok(((hash >> shift) & (CHAMP_BRANCH_FACTOR as u64 - 1)) as usize)
}

/// Reports whether one hash follows an entire compressed prefix.
fn prefix_matches(prefix: &[usize], hash: u64, level: usize) -> Result<bool, ManagedMemoryError> {
    for (offset, expected) in prefix.iter().enumerate() {
        if hash_slot(hash, level + offset)? != *expected {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Rejects an impossible path-copy operation through the wrong prefix.
fn validate_prefix(prefix: &[usize], hash: u64, level: usize) -> Result<(), ManagedMemoryError> {
    if prefix_matches(prefix, hash, level)? {
        Ok(())
    } else {
        Err(ManagedMemoryError::CorruptedCollection)
    }
}

/// Writes one checked A-CHAMP node header.
fn write_header(
    payload: &mut [u8],
    kind: u8,
    count: usize,
    total: usize,
    metadata: u64,
) -> Result<(), ManagedMemoryError> {
    payload[..4].copy_from_slice(&NODE_MAGIC.to_le_bytes());
    payload[4] = kind;
    payload[6..8].copy_from_slice(
        &u16::try_from(count)
            .map_err(|_| ManagedMemoryError::CollectionTooLarge)?
            .to_le_bytes(),
    );
    payload[8..16].copy_from_slice(
        &u64::try_from(total)
            .map_err(|_| ManagedMemoryError::CollectionTooLarge)?
            .to_le_bytes(),
    );
    payload[16..24].copy_from_slice(&metadata.to_le_bytes());
    Ok(())
}

/// Reads one checked little-endian `u16` field.
fn read_u16(payload: &[u8], offset: usize) -> Result<u16, ManagedMemoryError> {
    let bytes = payload
        .get(offset..offset + 2)
        .ok_or(ManagedMemoryError::CorruptedCollection)?
        .try_into()
        .map_err(|_| ManagedMemoryError::CorruptedCollection)?;
    Ok(u16::from_le_bytes(bytes))
}

/// Reads one checked little-endian `u32` field.
fn read_u32(payload: &[u8], offset: usize) -> Result<u32, ManagedMemoryError> {
    let bytes = payload
        .get(offset..offset + 4)
        .ok_or(ManagedMemoryError::CorruptedCollection)?
        .try_into()
        .map_err(|_| ManagedMemoryError::CorruptedCollection)?;
    Ok(u32::from_le_bytes(bytes))
}

/// Reads one checked little-endian `u64` field.
fn read_u64(payload: &[u8], offset: usize) -> Result<u64, ManagedMemoryError> {
    let bytes = payload
        .get(offset..offset + 8)
        .ok_or(ManagedMemoryError::CorruptedCollection)?
        .try_into()
        .map_err(|_| ManagedMemoryError::CorruptedCollection)?;
    Ok(u64::from_le_bytes(bytes))
}

/// Reads one checked target-size cardinality.
fn read_usize(payload: &[u8], offset: usize) -> Result<usize, ManagedMemoryError> {
    usize::try_from(read_u64(payload, offset)?).map_err(|_| ManagedMemoryError::CorruptedCollection)
}
