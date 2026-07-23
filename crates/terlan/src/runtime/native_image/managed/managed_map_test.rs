use super::*;
use crate::runtime::native_image::managed::{
    ActorId, HeapLimits, ManagedBytes, ManagedRoot, RootLocation,
};

fn heap() -> ActorHeap {
    ActorHeap::new(
        ActorId::new(121).expect("actor"),
        HeapLimits::new(4096, 16 * 1024 * 1024).expect("limits"),
    )
    .expect("heap")
}

#[derive(Debug, Default)]
struct CollisionSemantics;

impl ManagedKeySemantics for CollisionSemantics {
    fn equivalent(
        &mut self,
        _heap: &ActorHeap,
        left: ManagedFieldValue,
        right: ManagedFieldValue,
    ) -> Result<bool, ManagedMemoryError> {
        Ok(left == right)
    }

    fn hash(
        &mut self,
        _heap: &ActorHeap,
        _value: ManagedFieldValue,
    ) -> Result<u64, ManagedMemoryError> {
        Ok(7)
    }
}

#[derive(Debug, Default)]
struct BytesSemantics;

impl ManagedKeySemantics for BytesSemantics {
    fn equivalent(
        &mut self,
        heap: &ActorHeap,
        left: ManagedFieldValue,
        right: ManagedFieldValue,
    ) -> Result<bool, ManagedMemoryError> {
        Ok(bytes(heap, left)? == bytes(heap, right)?)
    }

    fn hash(
        &mut self,
        heap: &ActorHeap,
        value: ManagedFieldValue,
    ) -> Result<u64, ManagedMemoryError> {
        Ok(bytes(heap, value)?
            .iter()
            .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
                (hash ^ u64::from(*byte)).wrapping_mul(0x1000_0000_01b3)
            }))
    }
}

fn bytes<'a>(
    heap: &'a ActorHeap,
    value: ManagedFieldValue,
) -> Result<&'a [u8], ManagedMemoryError> {
    let ManagedFieldValue::Reference(reference) = value else {
        return Err(ManagedMemoryError::InvalidAggregateField);
    };
    heap.read_bytes(reference.cast::<ManagedBytes>())
}

fn int_entries(count: usize) -> Vec<(ManagedFieldValue, ManagedFieldValue)> {
    (0..count)
        .map(|value| {
            (
                ManagedFieldValue::Int(value as i64),
                ManagedFieldValue::Int((value * 10) as i64),
            )
        })
        .collect()
}

#[test]
fn map_preserves_order_replaces_duplicates_and_persists_updates() {
    let descriptor = ManagedMapDescriptor::new(
        "Map[Int, Int]",
        ManagedFieldType::Int,
        ManagedFieldType::Int,
    )
    .expect("descriptor");
    let mut heap = heap();
    let mut semantics = ManagedScalarKeySemantics;
    let original = heap
        .map_from_entries(
            &descriptor,
            &[
                (ManagedFieldValue::Int(2), ManagedFieldValue::Int(20)),
                (ManagedFieldValue::Int(1), ManagedFieldValue::Int(10)),
                (ManagedFieldValue::Int(2), ManagedFieldValue::Int(22)),
            ],
            &mut semantics,
        )
        .expect("map");

    assert_eq!(heap.map_length(&descriptor, original), Ok(2));
    assert_eq!(
        heap.map_profile(&descriptor, original),
        Ok(ManagedMapProfile::Flat)
    );
    assert_eq!(
        heap.map_entries(&descriptor, original),
        Ok(vec![
            (ManagedFieldValue::Int(2), ManagedFieldValue::Int(22)),
            (ManagedFieldValue::Int(1), ManagedFieldValue::Int(10)),
        ])
    );
    assert_eq!(
        heap.map_get(
            &descriptor,
            original,
            ManagedFieldValue::Int(2),
            &mut semantics,
        ),
        Ok(Some(ManagedFieldValue::Int(22)))
    );

    let replaced = heap
        .map_put(
            &descriptor,
            original,
            ManagedFieldValue::Int(1),
            ManagedFieldValue::Int(11),
            &mut semantics,
        )
        .expect("replace");
    let appended = heap
        .map_put(
            &descriptor,
            replaced,
            ManagedFieldValue::Int(3),
            ManagedFieldValue::Int(30),
            &mut semantics,
        )
        .expect("append");

    assert_eq!(heap.map_length(&descriptor, original), Ok(2));
    assert_eq!(
        heap.map_entries(&descriptor, appended),
        Ok(vec![
            (ManagedFieldValue::Int(2), ManagedFieldValue::Int(22)),
            (ManagedFieldValue::Int(1), ManagedFieldValue::Int(11)),
            (ManagedFieldValue::Int(3), ManagedFieldValue::Int(30)),
        ])
    );
}

#[test]
fn equal_map_shapes_reuse_immutable_root_descriptors() {
    let descriptor = ManagedMapDescriptor::new(
        "Map[Int, Int]",
        ManagedFieldType::Int,
        ManagedFieldType::Int,
    )
    .expect("descriptor");
    let mut heap = heap();
    let mut semantics = ManagedScalarKeySemantics;
    let first = heap
        .map_from_entries(&descriptor, &int_entries(3), &mut semantics)
        .expect("first map");
    let first_descriptor = heap.descriptor(first).expect("first descriptor") as *const _;
    let second = heap
        .map_from_entries(
            &descriptor,
            &[
                (ManagedFieldValue::Int(4), ManagedFieldValue::Int(40)),
                (ManagedFieldValue::Int(5), ManagedFieldValue::Int(50)),
                (ManagedFieldValue::Int(6), ManagedFieldValue::Int(60)),
            ],
            &mut semantics,
        )
        .expect("second map");

    assert_eq!(
        first_descriptor,
        heap.descriptor(second).expect("second descriptor") as *const _
    );

    let different_shape = heap
        .map_from_entries(&descriptor, &int_entries(2), &mut semantics)
        .expect("different map shape");
    assert_ne!(
        first_descriptor,
        heap.descriptor(different_shape)
            .expect("different descriptor") as *const _
    );
}

#[test]
fn indexed_map_path_copies_updates_and_survives_precise_relocation() {
    let descriptor = ManagedMapDescriptor::new(
        "Map[Int, Int]",
        ManagedFieldType::Int,
        ManagedFieldType::Int,
    )
    .expect("descriptor");
    let mut heap = heap();
    let mut semantics = ManagedScalarKeySemantics;
    let original = heap
        .map_from_entries(&descriptor, &int_entries(128), &mut semantics)
        .expect("indexed map");
    assert_eq!(
        heap.map_profile(&descriptor, original),
        Ok(ManagedMapProfile::Indexed)
    );

    let before_update = heap.object_count();
    let updated = heap
        .map_put(
            &descriptor,
            original,
            ManagedFieldValue::Int(64),
            ManagedFieldValue::Int(999),
            &mut semantics,
        )
        .expect("path-copy update");
    assert!(heap.object_count() - before_update <= 10);
    assert_eq!(
        heap.map_get(
            &descriptor,
            original,
            ManagedFieldValue::Int(64),
            &mut semantics,
        ),
        Ok(Some(ManagedFieldValue::Int(640)))
    );
    assert_eq!(
        heap.map_get(
            &descriptor,
            updated,
            ManagedFieldValue::Int(64),
            &mut semantics,
        ),
        Ok(Some(ManagedFieldValue::Int(999)))
    );

    let before_insert = heap.object_count();
    let appended = heap
        .map_put(
            &descriptor,
            updated,
            ManagedFieldValue::Int(128),
            ManagedFieldValue::Int(1280),
            &mut semantics,
        )
        .expect("path-copy insert");
    assert!(heap.object_count() - before_insert <= 12);
    assert_eq!(heap.map_length(&descriptor, appended), Ok(129));
    assert_eq!(
        heap.map_entries(&descriptor, appended)
            .expect("ordered entries")
            .last(),
        Some(&(ManagedFieldValue::Int(128), ManagedFieldValue::Int(1280)))
    );

    let old_root = appended;
    let mut roots = [ManagedRoot::new(
        heap.owner(),
        RootLocation::ActorState { slot: 0 },
        appended.erase(),
    )];
    heap.collect(&mut roots, 1_000_000)
        .expect("collect index graph");
    let relocated = roots[0].reference().cast();
    assert_eq!(
        heap.map_get(
            &descriptor,
            relocated,
            ManagedFieldValue::Int(128),
            &mut semantics,
        ),
        Ok(Some(ManagedFieldValue::Int(1280)))
    );
    assert_eq!(heap.read(old_root), Err(ManagedMemoryError::StaleReference));
}

#[test]
fn indexed_map_handles_full_hash_collisions_and_demotes_after_remove() {
    let descriptor = ManagedMapDescriptor::new(
        "Map[Int, Int]",
        ManagedFieldType::Int,
        ManagedFieldType::Int,
    )
    .expect("descriptor");
    let mut heap = heap();
    let mut semantics = CollisionSemantics;
    let original = heap
        .map_from_entries(&descriptor, &int_entries(128), &mut semantics)
        .expect("collision map");

    assert_eq!(
        heap.map_get(
            &descriptor,
            original,
            ManagedFieldValue::Int(127),
            &mut semantics,
        ),
        Ok(Some(ManagedFieldValue::Int(1270)))
    );
    let (taken, remainder) = heap
        .map_take(
            &descriptor,
            original,
            ManagedFieldValue::Int(64),
            &mut semantics,
        )
        .expect("collision take");
    assert_eq!(taken, Some(ManagedFieldValue::Int(640)));
    assert_eq!(heap.map_length(&descriptor, remainder), Ok(127));
    assert_eq!(
        heap.map_profile(&descriptor, remainder),
        Ok(ManagedMapProfile::Flat)
    );
    assert_eq!(
        heap.map_get(
            &descriptor,
            original,
            ManagedFieldValue::Int(64),
            &mut semantics,
        ),
        Ok(Some(ManagedFieldValue::Int(640)))
    );
}

#[test]
fn map_take_remove_clear_and_empty_reuse_have_persistent_semantics() {
    let descriptor = ManagedMapDescriptor::new(
        "Map[Int, Bool]",
        ManagedFieldType::Int,
        ManagedFieldType::Bool,
    )
    .expect("descriptor");
    let mut heap = heap();
    let mut semantics = ManagedScalarKeySemantics;
    let empty = heap.map_empty(&descriptor).expect("empty");
    assert_eq!(
        heap.map_profile(&descriptor, empty),
        Ok(ManagedMapProfile::Empty)
    );
    assert_eq!(heap.map_clear(&descriptor, empty), Ok(empty));

    let map = heap
        .map_put(
            &descriptor,
            empty,
            ManagedFieldValue::Int(7),
            ManagedFieldValue::Bool(true),
            &mut semantics,
        )
        .expect("put");
    let before_missing = heap.object_count();
    let (missing, unchanged) = heap
        .map_take(&descriptor, map, ManagedFieldValue::Int(8), &mut semantics)
        .expect("missing take");
    assert_eq!(missing, None);
    assert_eq!(unchanged, map);
    assert_eq!(heap.object_count(), before_missing);

    let (taken, remainder) = heap
        .map_take(&descriptor, map, ManagedFieldValue::Int(7), &mut semantics)
        .expect("take");
    assert_eq!(taken, Some(ManagedFieldValue::Bool(true)));
    assert_eq!(heap.map_is_empty(&descriptor, remainder), Ok(true));
    assert_eq!(heap.map_length(&descriptor, map), Ok(1));
}

#[test]
fn managed_reference_keys_use_content_semantics_and_relocate_precisely() {
    let bytes_type = SemanticTypeId::from_canonical("std.binary.Bytes").expect("bytes type");
    let descriptor = ManagedMapDescriptor::new(
        "Map[Bytes, Bytes]",
        ManagedFieldType::Reference(bytes_type),
        ManagedFieldType::Reference(bytes_type),
    )
    .expect("descriptor");
    let mut heap = heap();
    let first_key = heap.allocate_bytes(b"key").expect("first key");
    let equal_key = heap.allocate_bytes(b"key").expect("equal key");
    let first_value = heap.allocate_bytes(b"first").expect("first value");
    let replacement = heap.allocate_bytes(b"replacement").expect("replacement");
    let mut semantics = BytesSemantics;
    let map = heap
        .map_from_entries(
            &descriptor,
            &[
                (
                    ManagedFieldValue::Reference(first_key.erase()),
                    ManagedFieldValue::Reference(first_value.erase()),
                ),
                (
                    ManagedFieldValue::Reference(equal_key.erase()),
                    ManagedFieldValue::Reference(replacement.erase()),
                ),
            ],
            &mut semantics,
        )
        .expect("map");
    assert_eq!(heap.map_length(&descriptor, map), Ok(1));
    let entries = heap.map_entries(&descriptor, map).expect("entries");
    assert_eq!(bytes(&heap, entries[0].0), Ok(&b"key"[..]));
    assert_eq!(bytes(&heap, entries[0].1), Ok(&b"replacement"[..]));

    let old_map = map;
    let mut roots = [ManagedRoot::new(
        heap.owner(),
        RootLocation::ActorState { slot: 0 },
        map.erase(),
    )];
    heap.collect(&mut roots, 4096).expect("collect");
    let relocated = roots[0].reference().cast();
    let relocated_entries = heap.map_entries(&descriptor, relocated).expect("entries");
    assert_eq!(bytes(&heap, relocated_entries[0].0), Ok(&b"key"[..]));
    assert_eq!(
        bytes(&heap, relocated_entries[0].1),
        Ok(&b"replacement"[..])
    );
    assert_eq!(heap.read(old_map), Err(ManagedMemoryError::StaleReference));
}

#[test]
fn map_rejects_wrong_types_before_allocating() {
    let descriptor = ManagedMapDescriptor::new(
        "Map[Int, Bool]",
        ManagedFieldType::Int,
        ManagedFieldType::Bool,
    )
    .expect("descriptor");
    let mut heap = heap();
    let mut semantics = ManagedScalarKeySemantics;
    let before = heap.object_count();
    assert_eq!(
        heap.map_from_entries(
            &descriptor,
            &[(ManagedFieldValue::Bool(true), ManagedFieldValue::Bool(true))],
            &mut semantics,
        ),
        Err(ManagedMemoryError::InvalidAggregateField)
    );
    assert_eq!(heap.object_count(), before);
}
