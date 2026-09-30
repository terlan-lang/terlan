//! Closure snapshots preserve captures without retaining an actor heap or code pointer.

use crate::runtime::native_image::managed::{
    encode_collection_layout, ManagedClosureDescriptor, ManagedCollectionDescriptor,
    ManagedFieldType,
};
use crate::runtime::native_image::TvmManagedCollectionDescriptor;
use crate::runtime::vm::{memory::logical_value_bytes, NativeClosureValue};

use super::*;

fn managed_runtime(captures: Vec<TvmBoundaryType>) -> ManagedExecutionRuntime {
    ManagedExecutionRuntime::with_executable_image_metadata(
        &[],
        &[],
        &["ready".to_string()],
        [7; 32],
        &[TvmCallableDescriptor {
            id: 17,
            parameters: vec![TvmBoundaryType::Int],
            results: vec![TvmBoundaryType::Int],
            captures,
        }],
    )
    .unwrap()
}

fn closure(runtime: &ManagedExecutionRuntime, captures: Vec<ReplValue>) -> ReplValue {
    ReplValue::Closure(Arc::new(NativeClosureValue {
        descriptor: Arc::new(
            runtime
                .closure_dispatch()
                .unwrap()
                .closure_descriptor(17)
                .unwrap(),
        ),
        captures: captures.into_boxed_slice(),
    }))
}

fn boundary() -> TvmBoundaryType {
    TvmBoundaryType::Managed(
        ManagedClosureDescriptor::semantic_id_for_signature(
            &[TvmBoundaryType::Int],
            &[TvmBoundaryType::Int],
        )
        .unwrap()
        .bytes(),
    )
}

#[test]
fn owned_closure_round_trips_all_scalar_and_sequence_capture_kinds_between_heaps() {
    let mut source = managed_runtime(vec![
        TvmBoundaryType::Unit,
        TvmBoundaryType::Bool,
        TvmBoundaryType::Int,
        TvmBoundaryType::Float,
        TvmBoundaryType::Atom,
        TvmBoundaryType::String,
        TvmBoundaryType::Bytes,
        TvmBoundaryType::Binary,
    ]);
    let value = closure(
        &source,
        vec![
            ReplValue::Unit,
            ReplValue::Bool(true),
            ReplValue::Int(i64::MIN),
            ReplValue::Float("1.25".into()),
            ReplValue::Atom("ready".into()),
            ReplValue::String("captured".into()),
            ReplValue::Bytes(Arc::from([0, 255])),
            ReplValue::BitString(VmBitString::from_bytes(vec![0xa0], 3).unwrap()),
        ],
    );
    let word = encode_public_argument(&mut source, 1, &boundary(), &value).unwrap();
    let owned = decode_public_result(&source, 1, &boundary(), word).unwrap();
    assert_eq!(owned, value);
    source.reset_owner(1);
    assert!(decode_public_result(&source, 1, &boundary(), word).is_err());
    let mut destination = source.fork_empty();
    let word = encode_public_argument(&mut destination, 2, &boundary(), &owned).unwrap();
    assert_eq!(
        decode_public_result(&destination, 2, &boundary(), word).unwrap(),
        value
    );
    assert!(decode_public_result(&destination, 1, &boundary(), word).is_err());
    assert_eq!(value.render(), "<function>");
    assert!(logical_value_bytes(&value).unwrap() > 200);
    assert!(value.stable_hash().is_err());
    assert!(crate::runtime::vm::term_format::encode_tetf(&value, &[])
        .unwrap_err()
        .contains("tetf_function"));
    assert!(crate::runtime::vm::native_value::to_native(&value).is_err());
}

#[test]
fn closure_admission_rejects_forged_shapes_and_rolls_back_partial_capture_allocation() {
    let mut runtime = managed_runtime(vec![TvmBoundaryType::String, TvmBoundaryType::Int]);
    let valid = closure(
        &runtime,
        vec![ReplValue::String("retained".into()), ReplValue::Int(42)],
    );
    let ReplValue::Closure(original) = &valid else {
        panic!("closure")
    };
    let mut cases = vec![
        (ReplValue::Tuple(vec![ReplValue::Int(17)]), "Function"),
        (closure(&runtime, vec![]), "capture"),
        (
            closure(
                &runtime,
                vec![
                    ReplValue::String("allocated before failure".into()),
                    ReplValue::Bool(true),
                ],
            ),
            "managed_field",
        ),
    ];
    for (generation, id, parameters, captures, expected) in [
        (
            [8; 32],
            17,
            vec![TvmBoundaryType::Int],
            vec![TvmBoundaryType::String, TvmBoundaryType::Int],
            "generation",
        ),
        (
            [7; 32],
            19,
            vec![TvmBoundaryType::Int],
            vec![TvmBoundaryType::String, TvmBoundaryType::Int],
            "callable",
        ),
        (
            [7; 32],
            17,
            vec![TvmBoundaryType::Bool],
            vec![TvmBoundaryType::String, TvmBoundaryType::Int],
            "signature",
        ),
        (
            [7; 32],
            17,
            vec![TvmBoundaryType::Int],
            vec![TvmBoundaryType::String, TvmBoundaryType::Bool],
            "capture",
        ),
    ] {
        let descriptor = ManagedClosureDescriptor::new(
            crate::runtime::native_image::managed::ManagedClosureImageGeneration::new(generation)
                .unwrap(),
            id,
            parameters,
            vec![TvmBoundaryType::Int],
            captures,
        )
        .unwrap();
        cases.push((
            ReplValue::Closure(Arc::new(NativeClosureValue {
                descriptor: Arc::new(descriptor),
                captures: original.captures.clone(),
            })),
            expected,
        ));
    }
    for (value, expected) in cases {
        let error = encode_public_argument(&mut runtime, 1, &boundary(), &value).unwrap_err();
        assert!(error.contains(expected), "{error}");
        assert_eq!(
            runtime.heap_usage(1),
            Some((0, 0)),
            "failed admission retained captures"
        );
    }
    let word = encode_public_argument(&mut runtime, 1, &boundary(), &valid).unwrap();
    assert_eq!(
        decode_public_result(&runtime, 1, &boundary(), word).unwrap(),
        valid
    );
    let mut no_generation = ManagedExecutionRuntime::runtime_default().unwrap();
    assert!(encode_public_argument(&mut no_generation, 2, &boundary(), &valid).is_err());
    let mut resource = managed_runtime(vec![TvmBoundaryType::NativeResource(7)]);
    let value = closure(&resource, vec![ReplValue::Int(7)]);
    assert!(encode_public_argument(&mut resource, 1, &boundary(), &value).is_err());
    assert_eq!(resource.heap_usage(1), Some((0, 0)));
}

#[test]
fn closure_capture_graph_obeys_shared_depth_and_work_budgets() {
    let TvmBoundaryType::Managed(signature) = boundary() else {
        unreachable!()
    };
    let empty = managed_runtime(Vec::new());
    let mut value = closure(&empty, vec![]);
    // A separate admitted leaf and recursive capture shape share the same signature.
    let mut leaf = TvmCallableDescriptor {
        id: 17,
        parameters: vec![TvmBoundaryType::Int],
        results: vec![TvmBoundaryType::Int],
        captures: vec![],
    };
    let mut recursive = leaf.clone();
    recursive.id = 18;
    recursive.captures.push(TvmBoundaryType::Managed(signature));
    let mut runtime = ManagedExecutionRuntime::with_executable_image_metadata(
        &[],
        &[],
        &[],
        [7; 32],
        &[leaf.clone(), recursive],
    )
    .unwrap();
    let descriptor = Arc::new(
        runtime
            .closure_dispatch()
            .unwrap()
            .closure_descriptor(18)
            .unwrap(),
    );
    for _ in 0..260 {
        value = ReplValue::Closure(Arc::new(NativeClosureValue {
            descriptor: descriptor.clone(),
            captures: vec![value].into_boxed_slice(),
        }));
    }
    assert!(encode_public_argument(&mut runtime, 1, &boundary(), &value)
        .unwrap_err()
        .contains("managed_budget"));
    assert_eq!(runtime.heap_usage(1), Some((0, 0)));

    let leaf_descriptor = runtime
        .closure_dispatch()
        .unwrap()
        .closure_descriptor(17)
        .unwrap();
    let word = runtime
        .with_public_allocation(3, |heap, _| {
            let mut reference = heap
                .allocate_closure(&leaf_descriptor, &[])
                .map_err(|error| error.to_string())?;
            for _ in 0..260 {
                reference = heap
                    .allocate_closure(&descriptor, &[reference.encoded_abi_word() as i64])
                    .map_err(|error| error.to_string())?;
            }
            Ok(reference.encoded_abi_word() as i64)
        })
        .unwrap();
    assert!(decode_public_result(&runtime, 3, &boundary(), word)
        .unwrap_err()
        .contains("managed_budget"));

    let list = ManagedCollectionDescriptor::list("List[Int]", ManagedFieldType::Int).unwrap();
    leaf.captures = vec![TvmBoundaryType::Managed(list.semantic_id().bytes())];
    let metadata = TvmManagedCollectionDescriptor {
        semantic_id: list.semantic_id().bytes(),
        encoded_layout: encode_collection_layout(&list).unwrap(),
    };
    let mut runtime = ManagedExecutionRuntime::with_executable_image_metadata(
        &[],
        &[metadata],
        &[],
        [7; 32],
        &[leaf],
    )
    .unwrap();
    let value = closure(
        &runtime,
        vec![ReplValue::List(vec![ReplValue::Int(1); 65_536])],
    );
    assert!(encode_public_argument(&mut runtime, 2, &boundary(), &value)
        .unwrap_err()
        .contains("managed_budget"));
    assert_eq!(runtime.heap_usage(2), Some((0, 0)));
}

#[test]
fn materialization_checks_heap_closures_against_executable_membership() {
    use crate::runtime::native_image::managed::ManagedClosureImageGeneration;

    let mut runtime = managed_runtime(vec![TvmBoundaryType::Int]);
    for (generation, id, capture, expected) in [
        ([8; 32], 17, TvmBoundaryType::Int, "generation"),
        ([7; 32], 19, TvmBoundaryType::Int, "callable"),
        ([7; 32], 17, TvmBoundaryType::Bool, "capture"),
    ] {
        let descriptor = ManagedClosureDescriptor::new(
            ManagedClosureImageGeneration::new(generation).unwrap(),
            id,
            vec![TvmBoundaryType::Int],
            vec![TvmBoundaryType::Int],
            vec![capture],
        )
        .unwrap();
        let word = runtime
            .with_public_allocation(1, |heap, _| {
                heap.allocate_closure(&descriptor, &[1])
                    .map(|reference| reference.encoded_abi_word() as i64)
                    .map_err(|error| error.to_string())
            })
            .unwrap();
        let error = decode_public_result(&runtime, 1, &boundary(), word).unwrap_err();
        assert!(error.contains(expected), "{error}");
        runtime.reset_owner(1);
    }
}
