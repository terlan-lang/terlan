use super::*;

#[test]
/// Round-trips every closed collection family without byte drift.
fn collection_abi_round_trips_every_family_canonically() {
    let string = SemanticTypeId::from_canonical("std.core.String").expect("string identity");
    let descriptors = [
        ManagedCollectionDescriptor::list("List(Int)", ManagedFieldType::Int)
            .expect("list descriptor"),
        ManagedCollectionDescriptor::map(
            "Apply(Map;String,Int)",
            ManagedFieldType::Reference(string),
            ManagedFieldType::Int,
        )
        .expect("map descriptor"),
        ManagedCollectionDescriptor::set("Apply(Set;String)", ManagedFieldType::Reference(string))
            .expect("set descriptor"),
    ];

    for descriptor in descriptors {
        let encoded = encode_collection_layout(&descriptor).expect("encode collection");
        let decoded = decode_collection_layout(&encoded).expect("decode collection");
        assert_eq!(decoded, descriptor);
        assert_eq!(
            encode_collection_layout(&decoded).expect("canonical re-encode"),
            encoded
        );
    }
}

#[test]
/// Rejects every truncated prefix plus corrupted fixed-header fields.
fn collection_abi_rejects_truncation_and_header_corruption() {
    let descriptor =
        ManagedCollectionDescriptor::list("List(Int)", ManagedFieldType::Int).expect("list");
    let encoded = encode_collection_layout(&descriptor).expect("encoded list");
    for end in 0..encoded.len() {
        assert_eq!(
            decode_collection_layout(&encoded[..end]),
            Err(ManagedMemoryError::InvalidAggregateAbi)
        );
    }
    for offset in [0, 4, 6, 7] {
        let mut corrupted = encoded.clone();
        corrupted[offset] ^= 0xff;
        assert_eq!(
            decode_collection_layout(&corrupted),
            Err(ManagedMemoryError::InvalidAggregateAbi)
        );
    }
    let mut trailing = encoded;
    trailing.push(0);
    assert_eq!(
        decode_collection_layout(&trailing),
        Err(ManagedMemoryError::InvalidAggregateAbi)
    );
}

#[test]
/// Rejects empty identities, invalid family arity, and oversized metadata.
fn collection_abi_rejects_wrong_shape_and_bounds() {
    assert_eq!(
        ManagedCollectionDescriptor::list("", ManagedFieldType::Int),
        Err(ManagedMemoryError::InvalidAggregateShape)
    );
    let descriptor = ManagedCollectionDescriptor::map(
        "Apply(Map;Int,Bool)",
        ManagedFieldType::Int,
        ManagedFieldType::Bool,
    )
    .expect("map descriptor");
    let mut encoded = encode_collection_layout(&descriptor).expect("encoded map");
    let canonical_length = u32::from_le_bytes(encoded[8..12].try_into().expect("length")) as usize;
    encoded[12 + canonical_length] = 1;
    assert_eq!(
        decode_collection_layout(&encoded),
        Err(ManagedMemoryError::InvalidAggregateAbi)
    );
    assert_eq!(
        decode_collection_layout(&vec![0; MAX_MANAGED_COLLECTION_ABI_BYTES + 1]),
        Err(ManagedMemoryError::InvalidAggregateAbi)
    );
}
