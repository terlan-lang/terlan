use std::collections::{hash_map::DefaultHasher, BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use super::map_layout::{select_achamp_node_layout, AChampSubtreeHint, CHAMP_BRANCH_FACTOR};

/// Maximum shared persistent patch depth before the map is rebuilt.
const SHARED_PATCH_REBUILD_THRESHOLD: usize = 8;

/// VM-owned map storage selected by cardinality and shape.
///
/// Inputs:
/// - Insertion-ordered key/value entries.
/// - Optional hash-index buckets for larger dynamic maps.
///
/// Output:
/// - Persistent map value preserving Terlan insertion order while allowing
///   faster lookup/update on larger maps.
///
/// Transformation:
/// - Keeps small maps flat and switches larger maps to an indexed large-map
///   backend until the full A-CHAMP trie replaces the bucket index.
#[derive(Clone, Debug)]
pub(crate) enum VmMapValue<K, V> {
    Flat(Vec<(K, V)>),
    Indexed {
        base: Arc<Vec<(K, V)>>,
        root: Arc<AChampNode>,
        patches: Option<Arc<MapPatch<K, V>>>,
        tombstone_hashes: Arc<BTreeSet<u64>>,
        len: usize,
    },
}

/// Persistent A-CHAMP node used by the VM large-map backend.
#[derive(Clone, Debug)]
pub(crate) enum AChampNode {
    LeafBlock(Vec<usize>),
    SparseNode(Vec<(usize, Arc<AChampNode>)>),
    DenseNode(Vec<Option<Arc<AChampNode>>>),
    CollisionNode(Vec<usize>),
    CompressedPathNode {
        slots: Vec<usize>,
        child: Arc<AChampNode>,
    },
}

#[derive(Clone, Copy, Debug)]
struct HashedIndex {
    hash: u64,
    index: usize,
}

/// One structurally shared persistent map update.
#[derive(Clone, Debug)]
pub(crate) struct MapPatch<K, V> {
    key: K,
    value: Option<V>,
    previous: Option<Arc<MapPatch<K, V>>>,
    has_tombstone: bool,
}

impl<K, V> PartialEq for VmMapValue<K, V>
where
    K: Clone + Hash + PartialEq,
    V: Clone + PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.to_entries() == other.to_entries()
    }
}

impl<K, V> VmMapValue<K, V>
where
    K: Clone + Hash + PartialEq,
    V: Clone,
{
    /// Creates a map from insertion-ordered entries.
    pub(crate) fn from_entries(entries: Vec<(K, V)>) -> Self {
        if should_use_indexed(entries.len()) {
            let root = build_achamp_root(&entries);
            let len = entries.len();
            Self::Indexed {
                base: Arc::new(entries),
                root,
                patches: None,
                tombstone_hashes: Arc::new(BTreeSet::new()),
                len,
            }
        } else {
            Self::Flat(entries)
        }
    }

    /// Materializes entries with persistent patches applied.
    pub(crate) fn to_entries(&self) -> Vec<(K, V)> {
        match self {
            Self::Flat(entries) => entries.clone(),
            Self::Indexed { base, patches, .. } => {
                let mut entries = (**base).clone();
                for patch in patches_oldest_first(patches) {
                    insert_optional_patch(&mut entries, &patch.key, patch.value.clone());
                }
                entries
            }
        }
    }

    /// Returns the number of entries.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        match self {
            Self::Flat(entries) => entries.len(),
            Self::Indexed { len, .. } => *len,
        }
    }

    /// Visits every retained key/value allocation without materializing the map.
    #[cfg(test)]
    pub(crate) fn visit_retained_entries<'a>(
        &'a self,
        mut visit: impl FnMut(&'a K, Option<&'a V>),
    ) {
        match self {
            Self::Flat(entries) => {
                for (key, value) in entries {
                    visit(key, Some(value));
                }
            }
            Self::Indexed { base, patches, .. } => {
                for (key, value) in base.iter() {
                    visit(key, Some(value));
                }
                let mut patch = patches.as_deref();
                while let Some(current) = patch {
                    visit(&current.key, current.value.as_ref());
                    patch = current.previous.as_deref();
                }
            }
        }
    }

    vm_map_profile_component! {
        /// Looks up one key.
        pub(crate) fn lookup(&self, key: &K) -> Option<&V> {
            match self {
                Self::Flat(entries) => lookup(entries, key),
                Self::Indexed {
                    base,
                    root,
                    patches,
                    tombstone_hashes,
                    ..
                } => lookup_indexed(base, root, patches, tombstone_hashes, key),
            }
        }
    }

    /// Inserts or replaces one key in place.
    pub(crate) fn insert_or_replace(&mut self, key: K, value: V) {
        match self {
            Self::Flat(entries) => {
                insert_or_replace(entries, key, value);
                if should_use_indexed(entries.len()) {
                    let entries = std::mem::take(entries);
                    *self = Self::from_entries(entries);
                }
            }
            Self::Indexed {
                base,
                root,
                patches,
                tombstone_hashes,
                len,
            } => {
                let base_contains =
                    lookup_achamp_index(base, root, &key, hash_key(&key), 0).is_some();
                if let Some(base) = Arc::get_mut(base) {
                    if let Some(index) = lookup_achamp_index(base, root, &key, hash_key(&key), 0) {
                        if let Some((_, existing)) = base.get_mut(index) {
                            *existing = value;
                            return;
                        }
                    }
                    let index = base.len();
                    let hash = hash_key(&key);
                    base.push((key, value));
                    insert_achamp_index(root, base, HashedIndex { hash, index }, 0);
                    *len += 1;
                    return;
                }

                let existed = patched_key_exists(base_contains, tombstone_hashes, patches, &key);
                let has_tombstone = patches_have_tombstone(patches);
                *patches = Some(Arc::new(MapPatch {
                    key,
                    value: Some(value),
                    previous: patches.clone(),
                    has_tombstone,
                }));
                if !existed {
                    *len += 1;
                }
            }
        }
        compact_shared_patch_chain_if_needed(self);
    }

    /// Returns a persistent-style map with one key inserted or replaced.
    pub(crate) fn put_persistent(&self, key: K, value: V) -> Self {
        let mut updated = self.clone();
        updated.insert_or_replace(key, value);
        updated
    }

    vm_map_profile_component! {
        /// Returns an updated map when the previous value is compiler-proven dead.
        pub(crate) fn put_persistent_owned(mut self, key: K, value: V) -> Self {
        match &mut self {
            Self::Flat(entries) => {
                insert_or_replace(entries, key, value);
                if should_use_indexed(entries.len()) {
                    let entries = std::mem::take(entries);
                    self = Self::from_entries(entries);
                }
            }
            Self::Indexed {
                base,
                root,
                patches,
                tombstone_hashes,
                len,
            } => {
                let base_contains =
                    lookup_achamp_index(base, root, &key, hash_key(&key), 0).is_some();
                if base_contains {
                    if let Some(base) = Arc::get_mut(base) {
                        let Some(index) = lookup_achamp_index(base, root, &key, hash_key(&key), 0)
                        else {
                            return self;
                        };
                        if let Some((_, existing)) = base.get_mut(index) {
                            *existing = value;
                            return self;
                        }
                    }
                }
                let existed = patched_key_exists(base_contains, tombstone_hashes, patches, &key);
                let has_tombstone = patches_have_tombstone(patches);
                *patches = Some(Arc::new(MapPatch {
                    key,
                    value: Some(value),
                    previous: patches.clone(),
                    has_tombstone,
                }));
                if !existed {
                    *len += 1;
                }
            }
        }
        compact_shared_patch_chain_if_needed(&mut self);
            self
        }
    }

    /// Removes one key in place.
    pub(crate) fn remove(&mut self, key: &K) {
        match self {
            Self::Flat(entries) => entries.retain(|(entry_key, _)| entry_key != key),
            Self::Indexed {
                base,
                root,
                patches,
                tombstone_hashes,
                len,
            } => {
                let base_contains =
                    lookup_achamp_index(base, root, key, hash_key(key), 0).is_some();
                let existed = patched_key_exists(base_contains, tombstone_hashes, patches, key);
                if existed {
                    push_tombstone_hash(tombstone_hashes, hash_key(key));
                    *patches = Some(Arc::new(MapPatch {
                        key: key.clone(),
                        value: None,
                        previous: patches.clone(),
                        has_tombstone: true,
                    }));
                    *len = len.saturating_sub(1);
                }
            }
        }
        compact_shared_patch_chain_if_needed(self);
    }

    /// Clears the map.
    #[cfg(test)]
    pub(crate) fn clear(&mut self) {
        match self {
            Self::Flat(entries) => entries.clear(),
            Self::Indexed { .. } => {
                *self = Self::Flat(Vec::new());
            }
        }
    }

    #[cfg(test)]
    fn root_node_layout_for_test(&self) -> Option<super::map_layout::AChampNodeLayout> {
        match self {
            Self::Indexed { root, .. } => Some(achamp_node_layout(root)),
            Self::Flat(_) => None,
        }
    }

    #[cfg(test)]
    fn patch_depth_for_test(&self) -> usize {
        match self {
            Self::Indexed { patches, .. } => patch_chain_depth(patches),
            Self::Flat(_) => 0,
        }
    }
}

#[cfg(test)]
fn achamp_node_layout(node: &AChampNode) -> super::map_layout::AChampNodeLayout {
    match node {
        AChampNode::LeafBlock(_) => super::map_layout::AChampNodeLayout::LeafBlock,
        AChampNode::SparseNode(_) => super::map_layout::AChampNodeLayout::SparseNode,
        AChampNode::DenseNode(_) => super::map_layout::AChampNodeLayout::DenseNode,
        AChampNode::CollisionNode(_) => super::map_layout::AChampNodeLayout::CollisionNode,
        AChampNode::CompressedPathNode { .. } => {
            super::map_layout::AChampNodeLayout::CompressedPathNode
        }
    }
}

fn patches_oldest_first<K, V>(patches: &Option<Arc<MapPatch<K, V>>>) -> Vec<&MapPatch<K, V>> {
    let mut ordered = Vec::new();
    let mut current = patches.as_deref();
    while let Some(patch) = current {
        ordered.push(patch);
        current = patch.previous.as_deref();
    }
    ordered.reverse();
    ordered
}

/// Rebuilds shared persistent maps once patch lookup depth gets too high.
fn compact_shared_patch_chain_if_needed<K, V>(map: &mut VmMapValue<K, V>)
where
    K: Clone + Hash + PartialEq,
    V: Clone,
{
    let should_rebuild = match map {
        VmMapValue::Flat(_) => false,
        VmMapValue::Indexed { patches, .. } => {
            patch_chain_depth(patches) > SHARED_PATCH_REBUILD_THRESHOLD
        }
    };
    if should_rebuild {
        let entries = map.to_entries();
        *map = VmMapValue::from_entries(entries);
    }
}

/// Returns the number of structurally shared patches.
fn patch_chain_depth<K, V>(patches: &Option<Arc<MapPatch<K, V>>>) -> usize {
    let mut depth = 0;
    let mut current = patches.as_deref();
    while let Some(patch) = current {
        depth += 1;
        current = patch.previous.as_deref();
    }
    depth
}

fn lookup_patch<'a, K, V>(
    patches: &'a Option<Arc<MapPatch<K, V>>>,
    key: &K,
) -> Option<&'a Option<V>>
where
    K: PartialEq,
{
    let mut current = patches.as_deref();
    while let Some(patch) = current {
        if &patch.key == key {
            return Some(&patch.value);
        }
        current = patch.previous.as_deref();
    }
    None
}

fn patches_have_tombstone<K, V>(patches: &Option<Arc<MapPatch<K, V>>>) -> bool {
    patches.as_ref().is_some_and(|patch| patch.has_tombstone)
}

fn push_tombstone_hash(tombstone_hashes: &mut Arc<BTreeSet<u64>>, hash: u64) {
    Arc::make_mut(tombstone_hashes).insert(hash);
}

fn tombstone_hashes_contain<K>(tombstone_hashes: &BTreeSet<u64>, key: &K) -> bool
where
    K: Hash,
{
    tombstone_hashes.contains(&hash_key(key))
}

fn patched_key_exists<K, V>(
    base_contains: bool,
    tombstone_hashes: &BTreeSet<u64>,
    patches: &Option<Arc<MapPatch<K, V>>>,
    key: &K,
) -> bool
where
    K: Hash + PartialEq,
{
    if base_contains && !tombstone_hashes_contain(tombstone_hashes, key) {
        return true;
    }
    match lookup_patch(patches, key) {
        Some(Some(_)) => true,
        Some(None) => false,
        None => base_contains,
    }
}

vm_map_profile_component! {
fn lookup_indexed<'a, K, V>(
    base: &'a [(K, V)],
    root: &AChampNode,
    patches: &'a Option<Arc<MapPatch<K, V>>>,
    _tombstone_hashes: &BTreeSet<u64>,
    key: &K,
) -> Option<&'a V>
where
    K: Hash + PartialEq,
{
    if let Some(value) = lookup_patch(patches, key) {
        return value.as_ref();
    }
    lookup_achamp_node(base, root, key, hash_key(key), 0)
}
}

fn insert_optional_patch<K, V>(entries: &mut Vec<(K, V)>, key: &K, value: Option<V>)
where
    K: Clone + PartialEq,
{
    match value {
        Some(value) => insert_or_replace(entries, key.clone(), value),
        None => entries.retain(|(entry_key, _)| entry_key != key),
    }
}

/// Returns whether active VM map storage should leave the flat-small path.
///
/// Inputs:
/// - `len`: map cardinality after construction or mutation.
///
/// Output:
/// - `true` after the benchmarked flat-map inflection point.
///
/// Transformation:
/// - Keeps Terlan flat maps before the measured inflection point, then selects
///   the large-map backend for dynamic dictionary workloads.
pub(crate) fn should_use_indexed(len: usize) -> bool {
    super::map_layout::should_use_indexed_map(len)
}

fn build_achamp_root<K, V>(entries: &[(K, V)]) -> Arc<AChampNode>
where
    K: Hash,
{
    let hashed = entries
        .iter()
        .enumerate()
        .map(|(index, (key, _))| HashedIndex {
            hash: hash_key(key),
            index,
        })
        .collect::<Vec<_>>();
    build_achamp_node(&hashed, 0)
}

fn build_achamp_node(entries: &[HashedIndex], level: usize) -> Arc<AChampNode> {
    if entries.len() <= super::map_layout::LEAF_BLOCK_LIMIT {
        return Arc::new(AChampNode::LeafBlock(
            entries.iter().map(|entry| entry.index).collect(),
        ));
    }
    if entries
        .first()
        .is_some_and(|first| entries.iter().all(|entry| entry.hash == first.hash))
    {
        return Arc::new(AChampNode::CollisionNode(
            entries.iter().map(|entry| entry.index).collect(),
        ));
    }

    let shared_slots = shared_slot_prefix(entries, level);
    if shared_slots.len() >= super::map_layout::COMPRESSED_PATH_MIN_SKIPPED_LEVELS {
        return Arc::new(AChampNode::CompressedPathNode {
            slots: shared_slots.clone(),
            child: build_achamp_node(entries, level + shared_slots.len()),
        });
    }

    let groups = group_by_slot(entries, level);
    let layout = select_achamp_node_layout(AChampSubtreeHint {
        entry_count: entries.len(),
        occupied_slots: groups.len(),
        shared_prefix_levels: shared_slots.len(),
        full_hash_collision: false,
    });
    match layout {
        super::map_layout::AChampNodeLayout::DenseNode => {
            let mut slots = vec![None; CHAMP_BRANCH_FACTOR];
            for (slot, slot_entries) in groups {
                slots[slot] = Some(build_achamp_node(&slot_entries, level + 1));
            }
            Arc::new(AChampNode::DenseNode(slots))
        }
        _ => Arc::new(AChampNode::SparseNode(
            groups
                .into_iter()
                .map(|(slot, slot_entries)| (slot, build_achamp_node(&slot_entries, level + 1)))
                .collect(),
        )),
    }
}

vm_map_profile_component! {
fn lookup_achamp_node<'a, K, V>(
    base: &'a [(K, V)],
    node: &AChampNode,
    key: &K,
    hash: u64,
    level: usize,
) -> Option<&'a V>
where
    K: PartialEq,
{
    match node {
        AChampNode::LeafBlock(indices) | AChampNode::CollisionNode(indices) => {
            indices.iter().find_map(|index| {
                let (entry_key, value) = base.get(*index)?;
                (entry_key == key).then_some(value)
            })
        }
        AChampNode::SparseNode(children) => {
            let slot = hash_slot(hash, level);
            let child = children
                .iter()
                .find_map(|(child_slot, child)| (*child_slot == slot).then_some(child))?;
            lookup_achamp_node(base, child, key, hash, level + 1)
        }
        AChampNode::DenseNode(children) => {
            let slot = hash_slot(hash, level);
            let child = children.get(slot).and_then(Option::as_ref)?;
            lookup_achamp_node(base, child, key, hash, level + 1)
        }
        AChampNode::CompressedPathNode { slots, child } => {
            for (offset, expected_slot) in slots.iter().enumerate() {
                if hash_slot(hash, level + offset) != *expected_slot {
                    return None;
                }
            }
            lookup_achamp_node(base, child, key, hash, level + slots.len())
        }
    }
}
}

fn lookup_achamp_index<K, V>(
    base: &[(K, V)],
    node: &AChampNode,
    key: &K,
    hash: u64,
    level: usize,
) -> Option<usize>
where
    K: PartialEq,
{
    match node {
        AChampNode::LeafBlock(indices) | AChampNode::CollisionNode(indices) => {
            indices.iter().find_map(|index| {
                let (entry_key, _) = base.get(*index)?;
                (entry_key == key).then_some(*index)
            })
        }
        AChampNode::SparseNode(children) => {
            let slot = hash_slot(hash, level);
            let child = children
                .iter()
                .find_map(|(child_slot, child)| (*child_slot == slot).then_some(child))?;
            lookup_achamp_index(base, child, key, hash, level + 1)
        }
        AChampNode::DenseNode(children) => {
            let slot = hash_slot(hash, level);
            let child = children.get(slot).and_then(Option::as_ref)?;
            lookup_achamp_index(base, child, key, hash, level + 1)
        }
        AChampNode::CompressedPathNode { slots, child } => {
            for (offset, expected_slot) in slots.iter().enumerate() {
                if hash_slot(hash, level + offset) != *expected_slot {
                    return None;
                }
            }
            lookup_achamp_index(base, child, key, hash, level + slots.len())
        }
    }
}

fn insert_achamp_index<K, V>(
    node: &mut Arc<AChampNode>,
    base: &[(K, V)],
    entry: HashedIndex,
    level: usize,
) where
    K: Hash + PartialEq,
{
    let current = Arc::make_mut(node);
    match current {
        AChampNode::LeafBlock(indices) => {
            indices.push(entry.index);
            if indices.len() > super::map_layout::LEAF_BLOCK_LIMIT {
                *current = rebuild_node_from_indices(base, indices, level);
            }
        }
        AChampNode::CollisionNode(indices) => {
            let same_hash = indices
                .first()
                .and_then(|index| base.get(*index))
                .is_some_and(|(key, _)| hash_key(key) == entry.hash);
            indices.push(entry.index);
            if !same_hash {
                *current = rebuild_node_from_indices(base, indices, level);
            }
        }
        AChampNode::SparseNode(children) => {
            let slot = hash_slot(entry.hash, level);
            if let Some((_, child)) = children
                .iter_mut()
                .find(|(child_slot, _)| *child_slot == slot)
            {
                insert_achamp_index(child, base, entry, level + 1);
            } else {
                children.push((slot, Arc::new(AChampNode::LeafBlock(vec![entry.index]))));
                children.sort_by_key(|(child_slot, _)| *child_slot);
                if children.len() >= super::map_layout::DENSE_NODE_MIN_OCCUPIED_SLOTS {
                    let mut slots = vec![None; CHAMP_BRANCH_FACTOR];
                    for (child_slot, child) in children.drain(..) {
                        slots[child_slot] = Some(child);
                    }
                    *current = AChampNode::DenseNode(slots);
                }
            }
        }
        AChampNode::DenseNode(children) => {
            let slot = hash_slot(entry.hash, level);
            if let Some(Some(child)) = children.get_mut(slot) {
                insert_achamp_index(child, base, entry, level + 1);
            } else if let Some(child) = children.get_mut(slot) {
                *child = Some(Arc::new(AChampNode::LeafBlock(vec![entry.index])));
            }
        }
        AChampNode::CompressedPathNode { slots, child } => {
            let matches_prefix = slots
                .iter()
                .enumerate()
                .all(|(offset, slot)| hash_slot(entry.hash, level + offset) == *slot);
            if matches_prefix {
                insert_achamp_index(child, base, entry, level + slots.len());
            } else {
                let mut indices = collect_achamp_indices(current);
                indices.push(entry.index);
                *current = rebuild_node_from_indices(base, &indices, level);
            }
        }
    }
}

fn rebuild_node_from_indices<K, V>(base: &[(K, V)], indices: &[usize], level: usize) -> AChampNode
where
    K: Hash,
{
    let hashed = indices
        .iter()
        .filter_map(|index| {
            let (key, _) = base.get(*index)?;
            Some(HashedIndex {
                hash: hash_key(key),
                index: *index,
            })
        })
        .collect::<Vec<_>>();
    Arc::try_unwrap(build_achamp_node(&hashed, level)).unwrap_or_else(|node| (*node).clone())
}

fn collect_achamp_indices(node: &AChampNode) -> Vec<usize> {
    match node {
        AChampNode::LeafBlock(indices) | AChampNode::CollisionNode(indices) => indices.clone(),
        AChampNode::SparseNode(children) => children
            .iter()
            .flat_map(|(_, child)| collect_achamp_indices(child))
            .collect(),
        AChampNode::DenseNode(children) => children
            .iter()
            .filter_map(Option::as_ref)
            .flat_map(|child| collect_achamp_indices(child))
            .collect(),
        AChampNode::CompressedPathNode { child, .. } => collect_achamp_indices(child),
    }
}

fn group_by_slot(entries: &[HashedIndex], level: usize) -> BTreeMap<usize, Vec<HashedIndex>> {
    let mut groups = BTreeMap::<usize, Vec<HashedIndex>>::new();
    for entry in entries {
        groups
            .entry(hash_slot(entry.hash, level))
            .or_default()
            .push(*entry);
    }
    groups
}

fn shared_slot_prefix(entries: &[HashedIndex], level: usize) -> Vec<usize> {
    let mut slots = Vec::new();
    for current_level in level..MAX_HASH_LEVELS {
        let Some(first) = entries.first() else {
            break;
        };
        let slot = hash_slot(first.hash, current_level);
        if entries
            .iter()
            .all(|entry| hash_slot(entry.hash, current_level) == slot)
        {
            slots.push(slot);
        } else {
            break;
        }
    }
    slots
}

const MAX_HASH_LEVELS: usize = 13;

fn hash_slot(hash: u64, level: usize) -> usize {
    ((hash >> ((level % MAX_HASH_LEVELS) * 5)) & 0b1_1111) as usize
}

fn hash_key<K>(key: &K) -> u64
where
    K: Hash,
{
    let mut hasher = DefaultHasher::new();
    key.hash(&mut hasher);
    hasher.finish()
}

vm_map_profile_component! {
    /// Looks up one key in the current VM map entry layout.
    ///
    /// Inputs:
    /// - `entries`: insertion-ordered key/value pairs.
    /// - `key`: key to find.
    ///
    /// Output:
    /// - Borrowed value when the key exists.
    ///
    /// Transformation:
    /// - Preserves the current flat map semantics until the adaptive map storage
    ///   path replaces this representation.
    pub(crate) fn lookup<'a, K, V>(entries: &'a [(K, V)], key: &K) -> Option<&'a V>
    where
        K: PartialEq,
    {
        entries
            .iter()
            .find(|(entry_key, _)| entry_key == key)
            .map(|(_, value)| value)
    }
}

/// Inserts or replaces one VM map entry in place.
///
/// Inputs:
/// - `entries`: mutable insertion-ordered key/value pairs.
/// - `key`: key to insert or replace.
/// - `value`: replacement value.
///
/// Output:
/// - Unit after mutation.
///
/// Transformation:
/// - Replaces the first matching key while preserving entry order, otherwise
///   appends a new entry.
pub(crate) fn insert_or_replace<K, V>(entries: &mut Vec<(K, V)>, key: K, value: V)
where
    K: PartialEq,
{
    if let Some((_, existing)) = entries.iter_mut().find(|(entry_key, _)| *entry_key == key) {
        *existing = value;
    } else {
        entries.push((key, value));
    }
}

/// Returns a persistent-style map with one key inserted or replaced.
///
/// Inputs:
/// - `entries`: source map entries.
/// - `key`: key to insert or replace.
/// - `value`: replacement value.
///
/// Output:
/// - Cloned map entries with the requested update applied.
///
/// Transformation:
/// - Mirrors the current `Map.put` behavior, where updates return a new map
///   value rather than mutating the original one.
#[cfg(test)]
pub(crate) fn put_persistent<K, V>(entries: &[(K, V)], key: K, value: V) -> Vec<(K, V)>
where
    K: Clone + PartialEq,
    V: Clone,
{
    let mut updated = entries.to_vec();
    insert_or_replace(&mut updated, key, value);
    updated
}

#[cfg(test)]
#[path = "map_value_test.rs"]
#[cfg(test)]
mod map_value_test;
