use super::super::map_layout::AChampNodeLayout;
use super::{
    hash_key, hash_slot, insert_or_replace, lookup, put_persistent, should_use_indexed, VmMapValue,
};
use std::fmt;
use std::hash::{Hash, Hasher};

/// Verifies insertion preserves order and replacement updates in place.
///
/// Inputs:
/// - Empty and then populated entry vectors.
///
/// Output:
/// - Test passes when insert, lookup, and replacement match VM map semantics.
///
/// Transformation:
/// - Locks the current flat map behavior before benchmark code uses the same
///   helper path.
#[test]
fn flat_map_insert_lookup_and_replace_are_stable() {
    let mut entries = Vec::new();
    insert_or_replace(&mut entries, 1, "one");
    insert_or_replace(&mut entries, 2, "two");
    insert_or_replace(&mut entries, 1, "uno");

    assert_eq!(entries, vec![(1, "uno"), (2, "two")]);
    assert_eq!(lookup(&entries, &1), Some(&"uno"));
    assert_eq!(lookup(&entries, &3), None);
}

/// Verifies persistent put does not mutate the source entries.
///
/// Inputs:
/// - A one-entry source map.
///
/// Output:
/// - Test passes when the returned entries contain the update and the original
///   entries remain unchanged.
///
/// Transformation:
/// - Keeps `Map.put` benchmark semantics aligned with the runtime intrinsic.
#[test]
fn flat_map_put_persistent_clones_before_update() {
    let entries = vec![(1, 1)];
    let updated = put_persistent(&entries, 1, 2);

    assert_eq!(entries, vec![(1, 1)]);
    assert_eq!(updated, vec![(1, 2)]);
}

/// Verifies large-map promotion starts at the benchmarked inflection point.
///
/// Inputs:
/// - The 128-entry inflection point observed in VM-vs-OTP benchmarks.
///
/// Output:
/// - Test passes when 127 stays flat and 128 selects the indexed backend.
///
/// Transformation:
/// - Prevents the adaptive map threshold from drifting back to Erlang's
///   thirty-three-entry threshold, where Terlan VM flat maps still win.
#[test]
fn adaptive_map_switches_after_benchmarked_inflection_point() {
    assert!(!should_use_indexed(127));
    assert!(should_use_indexed(128));
}

/// Verifies indexed maps preserve insertion order and replace values.
///
/// Inputs:
/// - A 128-entry map that crosses the active large-map threshold.
///
/// Output:
/// - Test passes when lookup, replacement, and entry order remain stable.
///
/// Transformation:
/// - Locks user-visible map semantics while the internal representation changes
///   away from the flat vector path.
#[test]
fn indexed_map_lookup_replace_and_order_are_stable() {
    let entries = (1..=128).map(|value| (value, value)).collect::<Vec<_>>();
    let map = VmMapValue::from_entries(entries);

    assert_eq!(map.lookup(&128), Some(&128));

    let updated = map.put_persistent(128, 129);
    assert_eq!(updated.lookup(&128), Some(&129));
    let updated_entries = updated.to_entries();
    assert_eq!(updated_entries.first(), Some(&(1, 1)));
    assert_eq!(updated_entries.last(), Some(&(128, 129)));
}

#[test]
fn indexed_map_owned_private_update_reuses_unique_base_storage() {
    let map = VmMapValue::from_entries((1..=128).map(|value| (value, value)).collect::<Vec<_>>());

    let updated = map.put_persistent_owned(128, 512);

    assert_eq!(updated.lookup(&128), Some(&512));
    assert_eq!(updated.patch_depth_for_test(), 0);
    assert_eq!(updated.to_entries().last(), Some(&(128, 512)));
}

#[test]
fn indexed_map_mutable_insert_extends_unique_base_without_patch_chain() {
    let mut map = VmMapValue::from_entries((1..=128).map(|value| (value, value)).collect());

    map.insert_or_replace(129, 129);

    assert_eq!(map.lookup(&129), Some(&129));
    assert_eq!(map.len(), 129);
    assert_eq!(map.patch_depth_for_test(), 0);
    assert_eq!(map.to_entries().last(), Some(&(129, 129)));
}

#[test]
fn indexed_map_insert_and_remove_keep_bucket_lookup_coherent() {
    let mut map = VmMapValue::from_entries(
        (0..140)
            .map(|value| (format!("key-{value}"), value))
            .collect::<Vec<_>>(),
    );

    assert_eq!(map.lookup(&"key-99".to_string()), Some(&99));

    map.insert_or_replace("key-99".to_string(), 900);
    map.insert_or_replace("key-141".to_string(), 141);

    assert_eq!(map.lookup(&"key-99".to_string()), Some(&900));
    assert_eq!(map.lookup(&"key-141".to_string()), Some(&141));
    assert_eq!(map.len(), 141);

    map.remove(&"key-99".to_string());

    assert_eq!(map.lookup(&"key-99".to_string()), None);
    assert_eq!(map.len(), 140);
}

#[test]
fn indexed_map_repeated_remove_does_not_decrement_len_twice() {
    let source = VmMapValue::from_entries((1..=128).map(|value| (value, value)).collect());
    let mut map = source.clone();

    map.remove(&64);
    map.remove(&64);

    assert_eq!(map.lookup(&64), None);
    assert_eq!(map.len(), 127);
    assert_eq!(source.lookup(&64), Some(&64));
    assert_eq!(source.len(), 128);
}

#[test]
fn indexed_map_remove_then_reinsert_restores_length_and_value() {
    let source = VmMapValue::from_entries((1..=128).map(|value| (value, value)).collect());
    let mut map = source.clone();

    map.remove(&64);
    map.insert_or_replace(64, 640);

    assert_eq!(map.lookup(&64), Some(&640));
    assert_eq!(map.len(), 128);
    assert_eq!(source.lookup(&64), Some(&64));
}

#[test]
fn indexed_map_shared_persistent_updates_compact_patch_chain() {
    let source = VmMapValue::from_entries((1..=128).map(|value| (value, value)).collect());
    let mut current = source.clone();

    for offset in 0..=super::SHARED_PATCH_REBUILD_THRESHOLD {
        current = current.put_persistent(64, 1_000 + offset);
    }

    assert_eq!(
        current.lookup(&64),
        Some(&(1_000 + super::SHARED_PATCH_REBUILD_THRESHOLD))
    );
    assert_eq!(current.patch_depth_for_test(), 0);
    assert_eq!(current.len(), 128);
    assert_eq!(current.to_entries().get(63), Some(&(64, 1_008)));
    assert_eq!(source.lookup(&64), Some(&64));
    assert_eq!(source.to_entries().get(63), Some(&(64, 64)));
}

#[test]
fn indexed_map_shared_removes_compact_patch_chain() {
    let source = VmMapValue::from_entries((1..=128).map(|value| (value, value)).collect());
    let mut current = source.clone();

    for key in 1..=(super::SHARED_PATCH_REBUILD_THRESHOLD + 1) {
        current.remove(&key);
    }

    assert_eq!(current.patch_depth_for_test(), 0);
    assert_eq!(
        current.len(),
        128 - (super::SHARED_PATCH_REBUILD_THRESHOLD + 1)
    );
    assert_eq!(current.lookup(&1), None);
    assert_eq!(
        current.lookup(&(super::SHARED_PATCH_REBUILD_THRESHOLD + 2)),
        Some(&(super::SHARED_PATCH_REBUILD_THRESHOLD + 2))
    );
    assert_eq!(source.len(), 128);
    assert_eq!(source.lookup(&1), Some(&1));
}

#[test]
fn indexed_map_clear_drops_achamp_storage_and_resets_length() {
    let source = VmMapValue::from_entries((1..=128).map(|value| (value, value)).collect());
    let mut map = source.clone();

    map.clear();

    assert_eq!(map.len(), 0);
    assert_eq!(map.lookup(&64), None);
    assert_eq!(map.to_entries(), Vec::<(i32, i32)>::new());
    assert_eq!(map.root_node_layout_for_test(), None);
    assert_eq!(source.len(), 128);
    assert_eq!(source.lookup(&64), Some(&64));
}

#[test]
fn retained_entry_visitor_covers_flat_base_patch_and_tombstone_storage() {
    let flat = VmMapValue::from_entries(vec![(1, 10), (2, 20)]);
    let mut flat_entries = Vec::new();
    flat.visit_retained_entries(|key, value| flat_entries.push((*key, value.copied())));
    assert_eq!(flat_entries, vec![(1, Some(10)), (2, Some(20))]);

    let source = VmMapValue::from_entries((1..=128).map(|value| (value, value)).collect());
    let mut patched = source.clone();
    patched.insert_or_replace(64, 640);
    patched.remove(&65);

    let mut retained = Vec::new();
    patched.visit_retained_entries(|key, value| retained.push((*key, value.copied())));

    assert_eq!(retained.len(), 130);
    assert_eq!(retained.get(63), Some(&(64, Some(64))));
    assert_eq!(retained.get(64), Some(&(65, Some(65))));
    assert_eq!(retained.get(128), Some(&(65, None)));
    assert_eq!(retained.get(129), Some(&(64, Some(640))));
}

#[derive(Clone, PartialEq)]
struct CollidingDebugKey(usize);

impl fmt::Debug for CollidingDebugKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("same-debug-key")
    }
}

impl Hash for CollidingDebugKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        1_usize.hash(state);
    }
}

#[test]
fn achamp_indexed_map_uses_collision_node_for_equal_hashes() {
    let entries = (0..128)
        .map(|value| (CollidingDebugKey(value), value))
        .collect::<Vec<_>>();
    let map = VmMapValue::from_entries(entries);

    assert_eq!(
        map.root_node_layout_for_test(),
        Some(AChampNodeLayout::CollisionNode)
    );
    assert_eq!(map.lookup(&CollidingDebugKey(99)), Some(&99));
}

#[test]
fn achamp_indexed_map_compresses_long_shared_hash_prefixes() {
    let prefix = find_shared_slot_prefix_keys(2, 128);
    let entries = prefix
        .iter()
        .copied()
        .map(|value| (value, value))
        .collect::<Vec<_>>();
    let map = VmMapValue::from_entries(entries);

    assert_eq!(
        map.root_node_layout_for_test(),
        Some(AChampNodeLayout::CompressedPathNode)
    );
    assert_eq!(map.lookup(&prefix[97]), Some(&prefix[97]));
}

fn find_shared_slot_prefix_keys(prefix_len: usize, count: usize) -> Vec<i64> {
    let mut buckets: std::collections::BTreeMap<Vec<usize>, Vec<i64>> =
        std::collections::BTreeMap::new();
    for value in 0..500_000_i64 {
        let hash = hash_key(&value);
        let prefix = (0..prefix_len)
            .map(|level| hash_slot(hash, level))
            .collect::<Vec<_>>();
        let values = buckets.entry(prefix).or_default();
        values.push(value);
        if values.len() == count {
            return values.clone();
        }
    }
    panic!("could not find {count} keys with {prefix_len} shared CHAMP slots");
}
