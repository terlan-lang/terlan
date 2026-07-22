use super::*;
use crate::runtime::native_image::managed::{ActorId, HeapLimits};

/// Creates one bounded actor heap for allocation ABI tests.
fn heap(owner: u64) -> ActorHeap {
    ActorHeap::new(
        ActorId::new(owner).expect("actor"),
        HeapLimits::new(256, 1024 * 1024).expect("limits"),
    )
    .expect("heap")
}

/// Returns representative descriptors for every fixed aggregate family.
fn descriptors() -> Vec<ManagedAggregateDescriptor> {
    vec![
        ManagedAggregateDescriptor::tuple(
            "Tuple[Int,Bool]",
            vec![ManagedFieldType::Int, ManagedFieldType::Bool],
        )
        .expect("tuple"),
        ManagedAggregateDescriptor::fixed_array("Array[Float,2]", ManagedFieldType::Float, 2)
            .expect("array"),
        ManagedAggregateDescriptor::record(
            "app.User",
            vec![
                ("id".to_owned(), ManagedFieldType::Int),
                ("active".to_owned(), ManagedFieldType::Bool),
            ],
        )
        .expect("record"),
        ManagedAggregateDescriptor::constructor(
            "app.Result",
            "Ok",
            1,
            2,
            vec![(Some("value".to_owned()), ManagedFieldType::Int)],
        )
        .expect("constructor"),
    ]
}

#[test]
fn aggregate_abi_round_trips_every_shape_without_layout_drift() {
    for descriptor in descriptors() {
        let first = encode_aggregate_layout(&descriptor).expect("encode");
        let decoded = decode_aggregate_layout(&first).expect("decode");
        let second = encode_aggregate_layout(&decoded).expect("re-encode");

        assert_eq!(decoded, descriptor);
        assert_eq!(second, first);
        assert_eq!(
            decoded.managed().fingerprint(),
            descriptor.managed().fingerprint()
        );
        assert!(first.len() <= MAX_MANAGED_AGGREGATE_ABI_BYTES);
    }
}

#[test]
fn aggregate_abi_allocation_is_actor_owned_and_precisely_typed() {
    let mut owner = heap(91);
    let mut foreign = heap(92);
    let bytes_semantic = SemanticTypeId::from_canonical("std.binary.Bytes").expect("semantic");
    let descriptor = ManagedAggregateDescriptor::tuple(
        "Tuple[Bytes,Int]",
        vec![
            ManagedFieldType::Reference(bytes_semantic),
            ManagedFieldType::Int,
        ],
    )
    .expect("descriptor");
    let encoded = encode_aggregate_layout(&descriptor).expect("encode");
    let bytes = owner.allocate_bytes(b"owned").expect("bytes");
    let (value, decoded) = owner
        .allocate_aggregate_abi(
            &encoded,
            &[
                ManagedFieldValue::Reference(bytes.erase()),
                ManagedFieldValue::Int(7),
            ],
        )
        .expect("ABI allocation");
    let view = owner
        .read_aggregate(value, &decoded)
        .expect("aggregate view");

    assert_eq!(
        view.field(0),
        Ok(ManagedFieldValue::Reference(bytes.erase()))
    );
    assert_eq!(view.field(1), Ok(ManagedFieldValue::Int(7)));
    let foreign_bytes = foreign.allocate_bytes(b"foreign").expect("foreign bytes");
    assert_eq!(
        owner.allocate_aggregate_abi(
            &encoded,
            &[
                ManagedFieldValue::Reference(foreign_bytes.erase()),
                ManagedFieldValue::Int(8),
            ],
        ),
        Err(ManagedMemoryError::CrossActorReference)
    );
    assert_eq!(owner.object_count(), 2);
}

#[test]
fn aggregate_abi_rejects_truncation_corruption_and_ambiguity() {
    let descriptor = ManagedAggregateDescriptor::record(
        "app.Pair",
        vec![
            ("aa".to_owned(), ManagedFieldType::Int),
            ("bb".to_owned(), ManagedFieldType::Bool),
        ],
    )
    .expect("record");
    let encoded = encode_aggregate_layout(&descriptor).expect("encode");
    for end in 0..encoded.len() {
        assert_eq!(
            decode_aggregate_layout(&encoded[..end]),
            Err(ManagedMemoryError::InvalidAggregateAbi),
            "truncated at {end}"
        );
    }

    for (index, value) in [(0, b'X'), (4, 2), (6, 9), (7, 1)] {
        let mut corrupted = encoded.clone();
        corrupted[index] = value;
        assert_eq!(
            decode_aggregate_layout(&corrupted),
            Err(ManagedMemoryError::InvalidAggregateAbi)
        );
    }
    let mut trailing = encoded.clone();
    trailing.push(0);
    assert_eq!(
        decode_aggregate_layout(&trailing),
        Err(ManagedMemoryError::InvalidAggregateAbi)
    );
    let mut invalid_utf8 = encoded.clone();
    invalid_utf8[12] = 0xff;
    assert_eq!(
        decode_aggregate_layout(&invalid_utf8),
        Err(ManagedMemoryError::InvalidAggregateAbi)
    );
    let mut duplicate_names = encoded.clone();
    let second = duplicate_names
        .windows(2)
        .rposition(|window| window == b"bb")
        .expect("second field name");
    duplicate_names[second..second + 2].copy_from_slice(b"aa");
    assert_eq!(
        decode_aggregate_layout(&duplicate_names),
        Err(ManagedMemoryError::InvalidAggregateAbi)
    );
    assert_eq!(
        decode_aggregate_layout(&vec![0; MAX_MANAGED_AGGREGATE_ABI_BYTES + 1]),
        Err(ManagedMemoryError::InvalidAggregateAbi)
    );
}

#[test]
fn aggregate_abi_rejects_shape_specific_field_and_variant_corruption() {
    let array = ManagedAggregateDescriptor::fixed_array("Array[Int,2]", ManagedFieldType::Int, 2)
        .expect("array");
    let mut inconsistent = encode_aggregate_layout(&array).expect("array encoding");
    *inconsistent.last_mut().expect("last field tag") = 1;
    assert_eq!(
        decode_aggregate_layout(&inconsistent),
        Err(ManagedMemoryError::InvalidAggregateAbi)
    );

    let constructor = ManagedAggregateDescriptor::constructor(
        "Option[Int]",
        "Some",
        1,
        2,
        vec![(None, ManagedFieldType::Int)],
    )
    .expect("constructor");
    let mut invalid_variant = encode_aggregate_layout(&constructor).expect("constructor encoding");
    let discriminant_offset = HEADER_BYTES
        + 4
        + constructor.canonical_type().len()
        + 4
        + constructor.variant_name().expect("variant").len();
    invalid_variant[discriminant_offset..discriminant_offset + 4]
        .copy_from_slice(&2_u32.to_le_bytes());
    assert_eq!(
        decode_aggregate_layout(&invalid_variant),
        Err(ManagedMemoryError::InvalidAggregateAbi)
    );
}

#[test]
fn aggregate_abi_failure_never_publishes_a_partial_object() {
    let mut heap = heap(93);
    let descriptor = ManagedAggregateDescriptor::tuple("Tuple[Int]", vec![ManagedFieldType::Int])
        .expect("tuple");
    let encoded = encode_aggregate_layout(&descriptor).expect("encode");

    assert_eq!(
        heap.allocate_aggregate_abi(&encoded, &[ManagedFieldValue::Bool(true)]),
        Err(ManagedMemoryError::InvalidAggregateField)
    );
    assert_eq!(
        heap.allocate_aggregate_abi(&encoded[..encoded.len() - 1], &[ManagedFieldValue::Int(1)]),
        Err(ManagedMemoryError::InvalidAggregateAbi)
    );
    assert_eq!(heap.object_count(), 0);
}

#[test]
fn aggregate_abi_encoder_rejects_a_layout_above_the_boundary_limit() {
    let descriptor = ManagedAggregateDescriptor::record(
        "app.Oversized",
        vec![(
            "x".repeat(MAX_MANAGED_AGGREGATE_ABI_BYTES),
            ManagedFieldType::Int,
        )],
    )
    .expect("runtime descriptor remains independently valid");

    assert_eq!(
        encode_aggregate_layout(&descriptor),
        Err(ManagedMemoryError::InvalidAggregateAbi)
    );
}

#[test]
fn aggregate_word_abi_decodes_every_field_kind_into_one_owner_heap() {
    let mut heap = heap(94);
    let bytes = heap.allocate_bytes(b"child").expect("managed child");
    let semantic = SemanticTypeId::from_canonical("std.binary.Bytes").expect("semantic");
    let descriptor = ManagedAggregateDescriptor::tuple(
        "Tuple[Unit,Bool,Int,Float,Atom,Bytes]",
        vec![
            ManagedFieldType::Unit,
            ManagedFieldType::Bool,
            ManagedFieldType::Int,
            ManagedFieldType::Float,
            ManagedFieldType::Atom,
            ManagedFieldType::Reference(semantic),
        ],
    )
    .expect("descriptor");
    let encoded = encode_aggregate_layout(&descriptor).expect("encoded descriptor");
    let words = [
        0,
        1,
        -19,
        2.5_f64.to_bits() as i64,
        7,
        bytes.encoded().get() as i64,
    ];
    let (reference_word, decoded) = heap
        .allocate_aggregate_words_abi(&encoded, &words)
        .expect("word ABI allocation");
    let reference = TvmRef::from_encoded(
        NonZeroUsize::new(reference_word as usize).expect("opaque reference word"),
    );
    let view = heap
        .read_aggregate(reference, &decoded)
        .expect("aggregate view");

    assert_eq!(view.field(0), Ok(ManagedFieldValue::Unit));
    assert_eq!(view.field(1), Ok(ManagedFieldValue::Bool(true)));
    assert_eq!(view.field(2), Ok(ManagedFieldValue::Int(-19)));
    assert_eq!(view.field(3), Ok(ManagedFieldValue::Float(2.5)));
    assert_eq!(
        view.field(4),
        Ok(ManagedFieldValue::Atom(AtomIndex::from_runtime(7)))
    );
    assert_eq!(
        view.field(5),
        Ok(ManagedFieldValue::Reference(bytes.erase()))
    );
}

#[test]
fn aggregate_word_abi_rejects_bad_scalars_references_and_arity_atomically() {
    let mut owner = heap(95);
    let mut foreign = heap(96);
    let semantic = SemanticTypeId::from_canonical("std.binary.Bytes").expect("semantic");
    let descriptor = ManagedAggregateDescriptor::tuple(
        "Tuple[Unit,Bool,Float,Atom,Bytes]",
        vec![
            ManagedFieldType::Unit,
            ManagedFieldType::Bool,
            ManagedFieldType::Float,
            ManagedFieldType::Atom,
            ManagedFieldType::Reference(semantic),
        ],
    )
    .expect("descriptor");
    let encoded = encode_aggregate_layout(&descriptor).expect("encoded descriptor");
    let foreign_bytes = foreign.allocate_bytes(b"foreign").expect("foreign child");
    let base = [
        0,
        1,
        1.5_f64.to_bits() as i64,
        1,
        foreign_bytes.encoded().get() as i64,
    ];

    assert_eq!(
        owner.allocate_aggregate_words_abi(&encoded, &base),
        Err(ManagedMemoryError::CrossActorReference)
    );
    for (index, invalid) in [
        (0, 1),
        (1, 2),
        (2, f64::NAN.to_bits() as i64),
        (3, -1),
        (4, 0),
    ] {
        let mut words = base;
        words[index] = invalid;
        assert!(owner
            .allocate_aggregate_words_abi(&encoded, &words)
            .is_err());
    }
    assert_eq!(
        owner.allocate_aggregate_words_abi(&encoded, &base[..4]),
        Err(ManagedMemoryError::InvalidAggregateArity)
    );
    assert_eq!(owner.object_count(), 0);
}
