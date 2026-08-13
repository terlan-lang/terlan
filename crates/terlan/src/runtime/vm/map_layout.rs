/// Maximum size for normal inline map storage.
pub(crate) const FLAT_SMALL_MAP_LIMIT: usize = 32;

/// Maximum size for repeated-shape literal maps kept flat past the normal cliff.
pub(crate) const SHARED_SHAPE_FLAT_MAP_LIMIT: usize = 128;

/// Maximum subtree size stored as a compact A-CHAMP leaf block.
pub(crate) const LEAF_BLOCK_LIMIT: usize = 8;

/// Slot count in one CHAMP hash-fragment level.
pub(crate) const CHAMP_BRANCH_FACTOR: usize = 32;

/// Occupancy at or above this value switches from bitmap indexing to direct slots.
pub(crate) const DENSE_NODE_MIN_OCCUPIED_SLOTS: usize = 17;

/// Minimum skipped hash-fragment levels that justify path compression.
pub(crate) const COMPRESSED_PATH_MIN_SKIPPED_LEVELS: usize = 2;

/// First dynamic-map size where indexed storage beats the flat runtime path.
pub(crate) const ACTIVE_INDEXED_MAP_MIN: usize = 128;

/// Root representation selected for one VM-owned portable map.
///
/// Inputs:
/// - Produced from map construction/update shape metadata.
///
/// Output:
/// - Storage family used by the VM map implementation.
///
/// Transformation:
/// - Separates source-facing `std.collections.Map` semantics from the adaptive
///   internal representation selected by the VM.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MapRootLayout {
    FlatSmallMap,
    AChampRoot,
}

/// Internal A-CHAMP node shape selected for a subtree.
///
/// Inputs:
/// - Produced from subtree cardinality, hash occupancy, and collision shape.
///
/// Output:
/// - Node family for the future persistent map implementation.
///
/// Transformation:
/// - Encodes the A-CHAMP adaptation rules without committing the current REPL
///   value storage to a concrete node allocation strategy yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AChampNodeLayout {
    SparseNode,
    DenseNode,
    LeafBlock,
    CollisionNode,
    CompressedPathNode,
}

/// Describes source/runtime shape hints for one map root.
///
/// Inputs:
/// - `entry_count`: number of key/value pairs.
/// - `shared_literal_key_shape`: whether many maps reuse the same literal-key shape.
/// - `insert_delete_heavy`: whether the map behaves like a dynamic dictionary.
///
/// Output:
/// - Value passed to `select_map_root_layout`.
///
/// Transformation:
/// - Captures the first compiler/VM hints needed to avoid a fixed
///   thirty-three-entry cliff for record-like maps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MapShapeHint {
    pub(crate) entry_count: usize,
    pub(crate) shared_literal_key_shape: bool,
    pub(crate) insert_delete_heavy: bool,
}

/// Describes one A-CHAMP subtree.
///
/// Inputs:
/// - `entry_count`: entries contained in the subtree.
/// - `occupied_slots`: occupied hash-fragment slots at this level.
/// - `shared_prefix_levels`: consecutive low-information hash-fragment levels.
/// - `full_hash_collision`: whether all entries collided on the full hash.
///
/// Output:
/// - Value passed to `select_achamp_node_layout`.
///
/// Transformation:
/// - Keeps the adaptive node-choice criteria explicit and testable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AChampSubtreeHint {
    pub(crate) entry_count: usize,
    pub(crate) occupied_slots: usize,
    pub(crate) shared_prefix_levels: usize,
    pub(crate) full_hash_collision: bool,
}

/// Selects the root storage family for a portable Terlan map.
///
/// Inputs:
/// - `shape`: entry count and source-shape hints.
///
/// Output:
/// - `FlatSmallMap` for compact small or repeated-shape maps.
/// - `AChampRoot` for dynamic dictionaries and larger maps.
///
/// Transformation:
/// - Keeps Terlan's map representation adaptive without exposing Erlang or
///   Rust collection semantics at the language level.
pub(crate) fn select_map_root_layout(shape: MapShapeHint) -> MapRootLayout {
    if shape.entry_count <= FLAT_SMALL_MAP_LIMIT {
        return MapRootLayout::FlatSmallMap;
    }
    if shape.shared_literal_key_shape
        && !shape.insert_delete_heavy
        && shape.entry_count <= SHARED_SHAPE_FLAT_MAP_LIMIT
    {
        return MapRootLayout::FlatSmallMap;
    }
    MapRootLayout::AChampRoot
}

/// Selects the internal A-CHAMP node layout for one subtree.
///
/// Inputs:
/// - `shape`: cardinality, occupancy, prefix, and collision hints.
///
/// Output:
/// - Concrete adaptive node family.
///
/// Transformation:
/// - Applies collision, leaf, path-compression, and density decisions in a
///   deterministic order so future storage changes cannot silently change map
///   semantics or performance targets.
pub(crate) fn select_achamp_node_layout(shape: AChampSubtreeHint) -> AChampNodeLayout {
    if shape.full_hash_collision {
        return AChampNodeLayout::CollisionNode;
    }
    if shape.entry_count <= LEAF_BLOCK_LIMIT {
        return AChampNodeLayout::LeafBlock;
    }
    if shape.shared_prefix_levels >= COMPRESSED_PATH_MIN_SKIPPED_LEVELS {
        return AChampNodeLayout::CompressedPathNode;
    }
    if shape.occupied_slots >= DENSE_NODE_MIN_OCCUPIED_SLOTS {
        return AChampNodeLayout::DenseNode;
    }
    AChampNodeLayout::SparseNode
}

/// Reports whether a dynamic map should activate its indexed A-CHAMP profile.
pub(crate) fn should_use_indexed_map(entry_count: usize) -> bool {
    entry_count >= ACTIVE_INDEXED_MAP_MIN
        && select_map_root_layout(MapShapeHint {
            entry_count,
            shared_literal_key_shape: false,
            insert_delete_heavy: true,
        }) == MapRootLayout::AChampRoot
}

#[cfg(test)]
#[path = "map_layout_test.rs"]
#[cfg(test)]
mod map_layout_test;
