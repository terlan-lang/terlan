use super::{
    select_achamp_node_layout, select_map_root_layout, AChampNodeLayout, AChampSubtreeHint,
    MapRootLayout, MapShapeHint, CHAMP_BRANCH_FACTOR,
};

#[test]
fn map_root_layout_keeps_small_maps_flat_through_boundary() {
    assert_eq!(
        select_map_root_layout(MapShapeHint {
            entry_count: 32,
            shared_literal_key_shape: false,
            insert_delete_heavy: true,
        }),
        MapRootLayout::FlatSmallMap
    );
    assert_eq!(
        select_map_root_layout(MapShapeHint {
            entry_count: 33,
            shared_literal_key_shape: false,
            insert_delete_heavy: true,
        }),
        MapRootLayout::AChampRoot
    );
}

#[test]
fn map_root_layout_extends_flat_storage_for_shared_literal_shapes() {
    assert_eq!(
        select_map_root_layout(MapShapeHint {
            entry_count: 128,
            shared_literal_key_shape: true,
            insert_delete_heavy: false,
        }),
        MapRootLayout::FlatSmallMap
    );
    assert_eq!(
        select_map_root_layout(MapShapeHint {
            entry_count: 129,
            shared_literal_key_shape: true,
            insert_delete_heavy: false,
        }),
        MapRootLayout::AChampRoot
    );
}

#[test]
fn map_root_layout_does_not_keep_dynamic_dictionaries_flat() {
    assert_eq!(
        select_map_root_layout(MapShapeHint {
            entry_count: 64,
            shared_literal_key_shape: true,
            insert_delete_heavy: true,
        }),
        MapRootLayout::AChampRoot
    );
}

#[test]
fn achamp_node_layout_uses_leaf_blocks_for_small_subtrees() {
    assert_eq!(
        select_achamp_node_layout(AChampSubtreeHint {
            entry_count: 8,
            occupied_slots: 8,
            shared_prefix_levels: 0,
            full_hash_collision: false,
        }),
        AChampNodeLayout::LeafBlock
    );
    assert_eq!(
        select_achamp_node_layout(AChampSubtreeHint {
            entry_count: 9,
            occupied_slots: 8,
            shared_prefix_levels: 0,
            full_hash_collision: false,
        }),
        AChampNodeLayout::SparseNode
    );
}

#[test]
fn achamp_node_layout_prioritizes_true_hash_collisions() {
    assert_eq!(
        select_achamp_node_layout(AChampSubtreeHint {
            entry_count: 4,
            occupied_slots: 1,
            shared_prefix_levels: 3,
            full_hash_collision: true,
        }),
        AChampNodeLayout::CollisionNode
    );
}

#[test]
fn achamp_node_layout_compresses_long_shared_prefixes() {
    assert_eq!(
        select_achamp_node_layout(AChampSubtreeHint {
            entry_count: 12,
            occupied_slots: 1,
            shared_prefix_levels: 2,
            full_hash_collision: false,
        }),
        AChampNodeLayout::CompressedPathNode
    );
}

#[test]
fn achamp_node_layout_splits_sparse_and_dense_nodes() {
    assert_eq!(CHAMP_BRANCH_FACTOR, 32);
    assert_eq!(
        select_achamp_node_layout(AChampSubtreeHint {
            entry_count: 24,
            occupied_slots: 16,
            shared_prefix_levels: 0,
            full_hash_collision: false,
        }),
        AChampNodeLayout::SparseNode
    );
    assert_eq!(
        select_achamp_node_layout(AChampSubtreeHint {
            entry_count: 24,
            occupied_slots: 17,
            shared_prefix_levels: 0,
            full_hash_collision: false,
        }),
        AChampNodeLayout::DenseNode
    );
}
