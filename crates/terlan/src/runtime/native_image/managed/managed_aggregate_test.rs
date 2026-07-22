use std::sync::Arc;

use super::*;
use crate::runtime::native_image::managed::{
    ActorId, AtomTable, HeapLimits, ManagedRoot, RootLocation,
};

fn heap() -> ActorHeap {
    ActorHeap::new(
        ActorId::new(81).expect("actor"),
        HeapLimits::new(256, 1024 * 1024).expect("limits"),
    )
    .expect("heap")
}

#[test]
fn tuple_array_and_record_layouts_preserve_source_order_and_alignment() {
    let tuple = ManagedAggregateDescriptor::tuple(
        "Tuple[Bool,Int,Atom]",
        vec![
            ManagedFieldType::Bool,
            ManagedFieldType::Int,
            ManagedFieldType::Atom,
        ],
    )
    .expect("tuple");
    let array = ManagedAggregateDescriptor::fixed_array("Array[Int,3]", ManagedFieldType::Int, 3)
        .expect("array");
    let record = ManagedAggregateDescriptor::record(
        "app.User",
        vec![
            ("active".to_owned(), ManagedFieldType::Bool),
            ("id".to_owned(), ManagedFieldType::Int),
        ],
    )
    .expect("record");

    assert_eq!(
        tuple
            .fields()
            .iter()
            .map(ManagedFieldDescriptor::offset)
            .collect::<Vec<_>>(),
        vec![0, 8, 16]
    );
    assert_eq!(array.managed().size(), 24);
    assert_eq!(record.fields()[0].name(), Some("active"));
    assert_eq!(record.fields()[1].name(), Some("id"));
    assert_eq!(record.fields()[1].field_type(), ManagedFieldType::Int);
    assert_ne!(
        tuple.managed().fingerprint(),
        record.managed().fingerprint()
    );
}

#[test]
fn aggregate_values_round_trip_all_fixed_field_categories() {
    let mut heap = heap();
    let atoms = AtomTable::new(["ready"]).expect("atoms");
    let bytes = heap.allocate_bytes(b"payload").expect("bytes");
    let descriptor = Arc::new(
        ManagedAggregateDescriptor::record(
            "app.Payload",
            vec![
                ("unit".to_owned(), ManagedFieldType::Unit),
                ("enabled".to_owned(), ManagedFieldType::Bool),
                ("count".to_owned(), ManagedFieldType::Int),
                ("ratio".to_owned(), ManagedFieldType::Float),
                ("state".to_owned(), ManagedFieldType::Atom),
                (
                    "bytes".to_owned(),
                    ManagedFieldType::Reference(
                        SemanticTypeId::from_canonical("std.binary.Bytes").expect("semantic"),
                    ),
                ),
            ],
        )
        .expect("descriptor"),
    );
    let aggregate = heap
        .allocate_aggregate(
            descriptor.clone(),
            &[
                ManagedFieldValue::Unit,
                ManagedFieldValue::Bool(true),
                ManagedFieldValue::Int(-7),
                ManagedFieldValue::Float(1.25),
                ManagedFieldValue::Atom(atoms.index("ready").expect("atom")),
                ManagedFieldValue::Reference(bytes.erase()),
            ],
        )
        .expect("aggregate");
    let view = heap
        .read_aggregate(aggregate, &descriptor)
        .expect("aggregate view");

    assert_eq!(view.field(0), Ok(ManagedFieldValue::Unit));
    assert_eq!(view.field(1), Ok(ManagedFieldValue::Bool(true)));
    assert_eq!(view.field(2), Ok(ManagedFieldValue::Int(-7)));
    assert_eq!(view.field(3), Ok(ManagedFieldValue::Float(1.25)));
    assert_eq!(
        view.field(4),
        Ok(ManagedFieldValue::Atom(atoms.index("ready").expect("atom")))
    );
    assert_eq!(
        view.field(5),
        Ok(ManagedFieldValue::Reference(bytes.erase()))
    );
    assert_eq!(
        view.field(6),
        Err(ManagedMemoryError::InvalidAggregateField)
    );
}

#[test]
fn option_result_and_recursive_constructor_graphs_relocate_precisely() {
    let mut heap = heap();
    let node_semantic = SemanticTypeId::from_canonical("app.Node").expect("semantic");
    let none = Arc::new(
        ManagedAggregateDescriptor::constructor("Option[Int]", "None", 0, 2, vec![]).expect("none"),
    );
    let some = Arc::new(
        ManagedAggregateDescriptor::constructor(
            "Option[Int]",
            "Some",
            1,
            2,
            vec![(None, ManagedFieldType::Int)],
        )
        .expect("some"),
    );
    let ok = Arc::new(
        ManagedAggregateDescriptor::constructor(
            "Result[Int,app.Node]",
            "Ok",
            0,
            2,
            vec![(None, ManagedFieldType::Int)],
        )
        .expect("ok"),
    );
    let leaf_descriptor = Arc::new(
        ManagedAggregateDescriptor::constructor(
            "app.Node",
            "Leaf",
            0,
            2,
            vec![(Some("value".to_owned()), ManagedFieldType::Int)],
        )
        .expect("leaf"),
    );
    let branch_descriptor = Arc::new(
        ManagedAggregateDescriptor::constructor(
            "app.Node",
            "Branch",
            1,
            2,
            vec![
                (
                    Some("left".to_owned()),
                    ManagedFieldType::Reference(node_semantic),
                ),
                (
                    Some("right".to_owned()),
                    ManagedFieldType::Reference(node_semantic),
                ),
            ],
        )
        .expect("branch"),
    );

    let none_value = heap
        .allocate_aggregate(none.clone(), &[])
        .expect("none value");
    let some_value = heap
        .allocate_aggregate(some.clone(), &[ManagedFieldValue::Int(9)])
        .expect("some value");
    let ok_value = heap
        .allocate_aggregate(ok.clone(), &[ManagedFieldValue::Int(10)])
        .expect("ok value");
    let left = heap
        .allocate_aggregate(leaf_descriptor.clone(), &[ManagedFieldValue::Int(1)])
        .expect("left");
    let right = heap
        .allocate_aggregate(leaf_descriptor.clone(), &[ManagedFieldValue::Int(2)])
        .expect("right");
    let branch = heap
        .allocate_aggregate(
            branch_descriptor.clone(),
            &[
                ManagedFieldValue::Reference(left.erase()),
                ManagedFieldValue::Reference(right.erase()),
            ],
        )
        .expect("branch");

    assert_eq!(
        heap.read_aggregate(none_value, &none)
            .unwrap()
            .discriminant(),
        Some(0)
    );
    assert_eq!(
        heap.read_aggregate(some_value, &some).unwrap().field(0),
        Ok(ManagedFieldValue::Int(9))
    );
    assert_eq!(
        heap.read_aggregate(ok_value, &ok).unwrap().discriminant(),
        Some(0)
    );

    let mut roots = [ManagedRoot::new(
        heap.owner(),
        RootLocation::ActorState { slot: 0 },
        branch.erase(),
    )];
    let stats = heap.collect(&mut roots, 4096).expect("collect");
    let relocated: TvmRef<ManagedAggregate> = roots[0].reference().cast();
    let branch = heap
        .read_aggregate(relocated, &branch_descriptor)
        .expect("relocated branch");

    assert_eq!(stats.objects_after, 3);
    for (index, expected) in [(0, 1), (1, 2)] {
        let ManagedFieldValue::Reference(child) = branch.field(index).expect("child") else {
            panic!("expected child reference");
        };
        let child: TvmRef<ManagedAggregate> = child.cast();
        assert_eq!(
            heap.read_aggregate(child, &leaf_descriptor)
                .expect("leaf")
                .field(0),
            Ok(ManagedFieldValue::Int(expected))
        );
    }
}

#[test]
fn aggregate_construction_rejects_malformed_shapes_and_values_atomically() {
    assert_eq!(
        ManagedAggregateDescriptor::tuple("Tuple[]", vec![]),
        Err(ManagedMemoryError::InvalidAggregateShape)
    );
    assert_eq!(
        ManagedAggregateDescriptor::fixed_array("Array[Int,0]", ManagedFieldType::Int, 0),
        Err(ManagedMemoryError::InvalidAggregateShape)
    );
    assert_eq!(
        ManagedAggregateDescriptor::record(
            "app.Bad",
            vec![
                ("same".to_owned(), ManagedFieldType::Int),
                ("same".to_owned(), ManagedFieldType::Bool),
            ],
        ),
        Err(ManagedMemoryError::InvalidAggregateShape)
    );
    assert_eq!(
        ManagedAggregateDescriptor::constructor("Option[Int]", "Some", 2, 2, vec![]),
        Err(ManagedMemoryError::InvalidVariantDiscriminant)
    );

    let mut heap = heap();
    let descriptor = Arc::new(
        ManagedAggregateDescriptor::tuple(
            "Tuple[Float,Int]",
            vec![ManagedFieldType::Float, ManagedFieldType::Int],
        )
        .expect("descriptor"),
    );
    assert_eq!(
        heap.allocate_aggregate(descriptor.clone(), &[ManagedFieldValue::Int(1)]),
        Err(ManagedMemoryError::InvalidAggregateArity)
    );
    assert_eq!(
        heap.allocate_aggregate(
            descriptor.clone(),
            &[ManagedFieldValue::Int(1), ManagedFieldValue::Int(2)]
        ),
        Err(ManagedMemoryError::InvalidAggregateField)
    );
    assert_eq!(
        heap.allocate_aggregate(
            descriptor,
            &[
                ManagedFieldValue::Float(f64::NAN),
                ManagedFieldValue::Int(2)
            ]
        ),
        Err(ManagedMemoryError::InvalidManagedScalar)
    );
    assert_eq!(heap.object_count(), 0);
}

#[test]
fn same_sized_aggregate_shapes_have_distinct_fingerprints_and_views() {
    let mut heap = heap();
    let tuple = Arc::new(
        ManagedAggregateDescriptor::tuple("app.Pair", vec![ManagedFieldType::Int]).expect("tuple"),
    );
    let record = Arc::new(
        ManagedAggregateDescriptor::record(
            "app.Pair",
            vec![("value".to_owned(), ManagedFieldType::Int)],
        )
        .expect("record"),
    );
    assert_ne!(
        tuple.managed().fingerprint(),
        record.managed().fingerprint()
    );
    let value = heap
        .allocate_aggregate(tuple.clone(), &[ManagedFieldValue::Int(1)])
        .expect("value");
    assert!(matches!(
        heap.read_aggregate(value, &record),
        Err(ManagedMemoryError::ManagedTypeMismatch)
    ));
}

#[test]
fn aggregate_reference_fields_reject_wrong_semantic_types() {
    let mut heap = heap();
    let bytes = heap.allocate_bytes(b"bytes").expect("bytes");
    let expects_string = Arc::new(
        ManagedAggregateDescriptor::tuple(
            "Tuple[String]",
            vec![ManagedFieldType::Reference(
                SemanticTypeId::from_canonical("std.core.String").expect("semantic"),
            )],
        )
        .expect("descriptor"),
    );
    assert_eq!(
        heap.allocate_aggregate(
            expects_string,
            &[ManagedFieldValue::Reference(bytes.erase())]
        ),
        Err(ManagedMemoryError::InvalidAggregateField)
    );
    assert_eq!(heap.object_count(), 1);
}
