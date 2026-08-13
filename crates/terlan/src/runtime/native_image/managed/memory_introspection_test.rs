use std::sync::Arc;

use super::*;

/// Creates one test actor identity.
fn actor(value: u64) -> ActorId {
    ActorId::new(value).expect("valid test actor")
}

/// Creates bounded heap limits for memory-introspection tests.
fn limits() -> HeapLimits {
    HeapLimits::new(4096, 16384).expect("valid test heap limits")
}

/// Creates one deterministic managed descriptor.
fn descriptor(name: &str, size: usize, references: Vec<usize>) -> Arc<ManagedTypeDescriptor> {
    Arc::new(
        ManagedTypeDescriptor::new(
            SemanticTypeId::from_canonical(name).expect("semantic identity"),
            size,
            8,
            references,
            AllocationClass::Young,
        )
        .expect("managed descriptor"),
    )
}

#[test]
fn shallow_size_excludes_children_while_retained_size_deduplicates_shared_children() {
    let mut heap = ActorHeap::new(actor(901), limits()).expect("actor heap");
    let child = heap
        .allocate::<u64>(
            descriptor("memory.Child", 8, vec![]),
            &7_u64.to_le_bytes(),
            &[],
        )
        .expect("child");
    let parent = heap
        .allocate::<u64>(
            descriptor("memory.Parent", 16, vec![0, 8]),
            &[0; 16],
            &[(0, child.erase()), (8, child.erase())],
        )
        .expect("parent");

    assert_eq!(heap.shallow_size(parent), Ok(16));
    assert_eq!(heap.retained_size(parent), Ok(24));
    assert_eq!(heap.shallow_size(child), Ok(8));
    assert_eq!(heap.retained_size(child), Ok(8));
}

#[test]
fn string_sizes_include_directly_owned_external_utf8_storage() {
    let mut heap = ActorHeap::new(actor(902), limits()).expect("actor heap");
    let inline = heap.allocate_string("terlan").expect("inline string");
    let external_text = "x".repeat(2048);
    let external = heap
        .allocate_string(&external_text)
        .expect("external string");

    assert_eq!(
        heap.shallow_size(inline),
        Ok(MANAGED_SEQUENCE_HEADER_BYTES + "terlan".len())
    );
    assert_eq!(heap.retained_size(inline), heap.shallow_size(inline));
    assert_eq!(
        heap.shallow_size(external),
        Ok(MANAGED_SEQUENCE_HEADER_BYTES + external_text.len())
    );
    assert_eq!(heap.retained_size(external), heap.shallow_size(external));
}

#[test]
fn memory_sizes_reject_foreign_and_stale_references() {
    let mut owner = ActorHeap::new(actor(903), limits()).expect("owner heap");
    let foreign = ActorHeap::new(actor(904), limits()).expect("foreign heap");
    let reference = owner
        .allocate::<u64>(
            descriptor("memory.Value", 8, vec![]),
            &0_u64.to_le_bytes(),
            &[],
        )
        .expect("value");

    assert_eq!(
        foreign.shallow_size(reference),
        Err(ManagedMemoryError::CrossActorReference)
    );
    let mut roots = vec![ManagedRoot::new(
        actor(903),
        RootLocation::NativeStack {
            function_id: 1,
            slot: 0,
        },
        reference.erase(),
    )];
    owner.collect(&mut roots, 4096).expect("collection");
    assert_eq!(
        owner.retained_size(reference),
        Err(ManagedMemoryError::StaleReference)
    );
}
