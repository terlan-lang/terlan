use super::*;
use crate::runtime::native_image::managed::{ActorId, HeapLimits, ManagedRoot, RootLocation};

fn heap() -> ActorHeap {
    ActorHeap::new(
        ActorId::new(71).expect("actor"),
        HeapLimits::new(128, 1024 * 1024).expect("limits"),
    )
    .expect("heap")
}

#[test]
fn strings_and_bytes_preserve_empty_unicode_and_nul_payloads() {
    let mut heap = heap();
    let empty = heap.allocate_string("").expect("empty string");
    let unicode = heap.allocate_string("Terlan λ").expect("unicode string");
    let bytes = heap.allocate_bytes(&[0, 1, 255]).expect("bytes");

    assert_eq!(heap.read_string(empty), Ok(""));
    assert_eq!(heap.read_string(unicode), Ok("Terlan λ"));
    assert_eq!(heap.read_bytes(bytes), Ok(&[0, 1, 255][..]));
    assert_eq!(
        heap.allocate_string_bytes(&[0xff, 0xfe]),
        Err(ManagedMemoryError::InvalidUtf8)
    );
}

#[test]
fn binary_slices_enforce_bounds_and_bit_order() {
    let mut heap = heap();
    let storage = heap
        .allocate_bytes(&[0b1011_0001, 0b0100_0000])
        .expect("storage");
    let aligned = heap.allocate_binary(storage, 0, 8).expect("aligned");
    let unaligned = heap.allocate_binary(storage, 3, 7).expect("unaligned");

    let aligned = heap.read_binary(aligned).expect("aligned view");
    assert!(aligned.is_byte_aligned());
    assert_eq!(aligned.aligned_bytes(), Some(&[0b1011_0001][..]));
    assert_eq!(aligned.storage(), &[0b1011_0001, 0b0100_0000]);

    let unaligned = heap.read_binary(unaligned).expect("unaligned view");
    assert!(!unaligned.is_byte_aligned());
    assert_eq!(unaligned.aligned_bytes(), None);
    assert_eq!(unaligned.bit_offset(), 3);
    assert_eq!(unaligned.bit_length(), 7);
    assert_eq!(
        (0..7).map(|index| unaligned.bit(index)).collect::<Vec<_>>(),
        vec![
            Some(true),
            Some(false),
            Some(false),
            Some(false),
            Some(true),
            Some(false),
            Some(true)
        ]
    );
    assert_eq!(unaligned.bit(7), None);
    assert_eq!(
        heap.allocate_binary(storage, 15, 2),
        Err(ManagedMemoryError::InvalidBitRange)
    );
    assert_eq!(
        heap.allocate_binary(storage, usize::MAX, 1),
        Err(ManagedMemoryError::InvalidBitRange)
    );
}

#[test]
fn sequence_graph_survives_precise_relocation() {
    let mut heap = heap();
    let storage = heap.allocate_bytes(b"abcdef").expect("storage");
    let binary = heap.allocate_binary(storage, 8, 24).expect("binary");
    let old_storage = storage;
    let old_binary = binary;
    let mut roots = [ManagedRoot::new(
        heap.owner(),
        RootLocation::Mailbox {
            fragment: 1,
            slot: 0,
        },
        binary.erase(),
    )];

    let stats = heap.collect(&mut roots, 4096).expect("collect");
    let relocated: TvmRef<ManagedBinary> = roots[0].reference().cast();
    let view = heap.read_binary(relocated).expect("relocated binary");

    assert_eq!(view.aligned_bytes(), Some(&b"bcd"[..]));
    assert_eq!(stats.objects_after, 2);
    assert_eq!(
        heap.read(old_binary),
        Err(ManagedMemoryError::StaleReference)
    );
    assert_eq!(
        heap.read(old_storage),
        Err(ManagedMemoryError::StaleReference)
    );
}

#[test]
fn typed_sequence_access_rejects_wrong_and_foreign_references() {
    let mut first = heap();
    let second = ActorHeap::new(
        ActorId::new(72).expect("actor"),
        HeapLimits::new(128, 4096).expect("limits"),
    )
    .expect("heap");
    let bytes = first.allocate_bytes(b"typed").expect("bytes");
    let forged_string: TvmRef<ManagedString> = bytes.erase().cast();

    assert_eq!(
        first.read_string(forged_string),
        Err(ManagedMemoryError::ManagedTypeMismatch)
    );
    assert_eq!(
        second.read_bytes(bytes),
        Err(ManagedMemoryError::CrossActorReference)
    );
}
