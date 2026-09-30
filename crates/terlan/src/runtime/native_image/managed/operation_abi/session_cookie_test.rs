//! Session ingress accepts only a checked optional cookie, never a request layout.

use super::*;
use crate::runtime::native_image::managed::{
    encode_aggregate_layout, managed_string_semantic_id, ActorId, HeapLimits,
    ManagedAggregateDescriptor, ManagedFieldType,
};
use crate::runtime::native_image::TvmManagedLayoutDescriptor;
use crate::runtime::vm::http_session::VmHttpSessionRuntime;

fn fixture() -> (
    ActorHeap,
    ManagedLayoutRegistry,
    SemanticTypeId,
    SemanticTypeId,
) {
    let heap = ActorHeap::new(
        ActorId::new(121).unwrap(),
        HeapLimits::new(1024 * 1024, 16 * 1024 * 1024).unwrap(),
    )
    .unwrap();
    let option = SemanticTypeId::from_canonical("Apply(Option;String)").unwrap();
    let session = SemanticTypeId::from_canonical("Tuple(String,Bool)").unwrap();
    let string = ManagedFieldType::Reference(managed_string_semantic_id());
    let descriptors = [
        ManagedAggregateDescriptor::constructor("Apply(Option;String)", "None", 0, 2, vec![])
            .unwrap(),
        ManagedAggregateDescriptor::constructor(
            "Apply(Option;String)",
            "Some",
            1,
            2,
            vec![(Some("value".into()), string)],
        )
        .unwrap(),
        ManagedAggregateDescriptor::tuple(
            "Tuple(String,Bool)",
            vec![string, ManagedFieldType::Bool],
        )
        .unwrap(),
    ];
    let encoded = descriptors
        .iter()
        .map(|descriptor| TvmManagedLayoutDescriptor {
            semantic_id: descriptor.managed().semantic_id().bytes(),
            encoded_layout: encode_aggregate_layout(descriptor).unwrap(),
        })
        .collect::<Vec<_>>();
    let layouts = ManagedLayoutRegistry::from_image(&encoded, &[], &[]).unwrap();
    (heap, layouts, option, session)
}

#[test]
fn retired_option_predicate_rejects_payloads_without_heap_or_session_changes() {
    let (mut heap, layouts, option, _) = fixture();
    let service =
        VmHttpSessionService::new(VmHttpSessionRuntime::new("retired-option", 100).unwrap());
    let some =
        allocate_option_string(&mut heap, &layouts, option, Some("preserved")).unwrap() as i64;
    let none = allocate_option_string(&mut heap, &layouts, option, None).unwrap() as i64;
    let before = heap.allocated_bytes();
    let mut retired = header(8);
    push_semantics(&mut retired, &[option]);
    let mut trailing = retired.clone();
    trailing.push(0);
    for encoded in (0..=retired.len())
        .map(|length| &retired[..length])
        .chain([trailing.as_slice()])
    {
        for words in [
            vec![],
            vec![0],
            vec![-1],
            vec![some],
            vec![none],
            vec![some, none],
        ] {
            assert_eq!(
                execute_session_operation(&mut heap, &layouts, Some(&service), encoded, &words),
                Err(ManagedMemoryError::InvalidManagedOperation)
            );
            assert_eq!(heap.allocated_bytes(), before);
        }
    }
    assert_eq!(
        read_option_string(&heap, &layouts, option, some).unwrap(),
        Some("preserved".into())
    );
    assert_eq!(
        read_option_string(&heap, &layouts, option, none).unwrap(),
        None
    );
    assert!(service
        .with_runtime(|state| state.snapshots().is_empty())
        .unwrap());
}

#[test]
fn cookie_option_ingress_validates_variants_semantics_and_legacy_payloads() {
    let (mut heap, layouts, option, session) = fixture();
    for value in [None, Some(""), Some("s1"), Some("malformed; cookie")] {
        let word = allocate_option_string(&mut heap, &layouts, option, value).unwrap() as i64;
        assert_eq!(
            read_option_string(&heap, &layouts, option, word).unwrap(),
            value.map(str::to_owned)
        );
        assert!(read_option_string(&heap, &layouts, session, word).is_err());
    }
    for (variant, fields) in [
        ("Other", vec![]),
        ("None", vec![ManagedFieldType::Int]),
        ("Some", vec![]),
        ("Some", vec![ManagedFieldType::Int]),
    ] {
        let descriptor = ManagedAggregateDescriptor::constructor(
            "Apply(Option;String)",
            variant,
            0,
            1,
            fields.iter().map(|ty| (None, *ty)).collect(),
        )
        .unwrap();
        let malformed = ManagedLayoutRegistry::from_image(
            &[TvmManagedLayoutDescriptor {
                semantic_id: option.bytes(),
                encoded_layout: encode_aggregate_layout(&descriptor).unwrap(),
            }],
            &[],
            &[],
        )
        .unwrap();
        let reference = heap
            .allocate_aggregate_ref(
                &descriptor,
                &fields
                    .iter()
                    .map(|_| ManagedFieldValue::Int(0))
                    .collect::<Vec<_>>(),
            )
            .unwrap();
        assert!(read_option_string(
            &heap,
            &malformed,
            option,
            reference.encoded_abi_word() as i64
        )
        .is_err());
    }
    let service = VmHttpSessionService::new(VmHttpSessionRuntime::new("cookie-test", 100).unwrap());
    let none = allocate_option_string(&mut heap, &layouts, option, None).unwrap() as i64;
    let operation = encode_session_current_operation(option, session);
    let mut legacy = operation.clone();
    legacy.extend_from_slice(&[0; SEMANTIC_BYTES + 4]);
    assert!(
        execute_session_operation(&mut heap, &layouts, Some(&service), &legacy, &[none]).is_err()
    );
    for words in [vec![], vec![none, none], vec![0], vec![-1]] {
        assert!(
            execute_session_operation(&mut heap, &layouts, Some(&service), &operation, &words)
                .is_err()
        );
    }
    assert!(service
        .with_runtime(|state| state.snapshots().is_empty())
        .unwrap());
    assert!(execute_session_operation(&mut heap, &layouts, None, &operation, &[none]).is_err());
    execute_session_operation(&mut heap, &layouts, Some(&service), &operation, &[none]).unwrap();
    assert_eq!(
        service
            .with_runtime(|state| state.snapshots().len())
            .unwrap(),
        1
    );
}

#[test]
fn string_identity_liveness_rejects_source_records_and_retired_policy_opcodes() {
    let (mut heap, layouts, option, _) = fixture();
    let mut state = VmHttpSessionRuntime::new("response-cookie-test", 100).unwrap();
    let (handle, _) = http_session::current(&mut state, None)
        .unwrap()
        .into_managed_parts();
    assert!(state.is_live(&handle));
    let service = VmHttpSessionService::new(state);
    let operation = encode_session_is_live_operation();
    assert!(!session_operation_result_is_reference(&operation));
    let word = heap
        .allocate_string(handle.managed_id())
        .unwrap()
        .encoded_abi_word() as i64;
    for _ in 0..2 {
        assert_eq!(
            execute_session_operation(&mut heap, &layouts, Some(&service), &operation, &[word]),
            Ok(1)
        );
    }
    service
        .with_runtime(|state| http_session::expire(state, &handle))
        .unwrap()
        .unwrap();
    assert!(!service
        .with_runtime(|state| state.is_live(&handle))
        .unwrap());
    let result =
        execute_session_operation(&mut heap, &layouts, Some(&service), &operation, &[word])
            .unwrap();
    assert_eq!(result, 0);
    let wrong_semantic = allocate_option_string(&mut heap, &layouts, option, None).unwrap() as i64;
    for words in [
        vec![],
        vec![word, word],
        vec![wrong_semantic],
        vec![0],
        vec![-1],
    ] {
        assert!(
            execute_session_operation(&mut heap, &layouts, Some(&service), &operation, &words)
                .is_err()
        );
    }
    assert!(execute_session_operation(&mut heap, &layouts, None, &operation, &[word]).is_err());
    for (opcode, length) in [
        (7, HEADER_BYTES + SEMANTIC_BYTES * 4 + 4),
        (9, HEADER_BYTES + SEMANTIC_BYTES * 2),
        (1, CURRENT_BYTES),
        (5, HEADER_BYTES + SEMANTIC_BYTES),
        (10, HEADER_BYTES + SEMANTIC_BYTES * 2),
        (13, HEADER_BYTES + SEMANTIC_BYTES * 2),
    ] {
        let mut retired = header(opcode);
        push_semantics(&mut retired, &[option, option]);
        retired.resize(length, 0);
        assert!(!session_operation_result_is_reference(&retired));
        let before = heap.allocated_bytes();
        for words in [vec![], vec![0], vec![-1], vec![word], vec![word, word]] {
            assert_eq!(
                execute_session_operation(&mut heap, &layouts, Some(&service), &retired, &words),
                Err(ManagedMemoryError::InvalidManagedOperation)
            );
            assert_eq!(heap.allocated_bytes(), before);
        }
    }
    assert!(service
        .with_runtime(|state| state.snapshots().is_empty())
        .unwrap());
}

#[test]
fn identity_operations_reject_legacy_versions_and_invalid_arguments_without_mutation() {
    let (mut heap, layouts, option, lookup) = fixture();
    let mut state = VmHttpSessionRuntime::new("identity-boundary", 100).unwrap();
    let (session, _) = http_session::current(&mut state, None)
        .unwrap()
        .into_managed_parts();
    http_session::set(&mut state, &session, "key", "original").unwrap();
    let service = VmHttpSessionService::new(state);
    let id = heap
        .allocate_string(session.managed_id())
        .unwrap()
        .encoded_abi_word() as i64;
    let key = heap.allocate_string("key").unwrap().encoded_abi_word() as i64;
    let value = heap
        .allocate_string("replacement")
        .unwrap()
        .encoded_abi_word() as i64;
    let none = allocate_option_string(&mut heap, &layouts, option, None).unwrap() as i64;
    let operations = [
        (encode_session_current_operation(option, lookup), vec![none]),
        (encode_session_get_operation(option), vec![id, key]),
        (
            encode_session_mutation_operation(ManagedSessionMutation::Set),
            vec![id, key, value],
        ),
        (
            encode_session_mutation_operation(ManagedSessionMutation::Delete),
            vec![id, key],
        ),
        (encode_session_rotate_operation(), vec![id]),
        (encode_session_expire_operation(), vec![id]),
        (encode_session_is_live_operation(), vec![id]),
    ];
    let before = heap.allocated_bytes();
    for (encoded, words) in operations {
        let mut old = encoded.clone();
        old[4..6].copy_from_slice(&1_u16.to_le_bytes());
        let mut trailing = encoded.clone();
        trailing.push(0);
        let mut reserved = encoded.clone();
        reserved[7] = 1;
        for invalid in [old, trailing, reserved] {
            assert_eq!(
                execute_session_operation(&mut heap, &layouts, Some(&service), &invalid, &words),
                Err(ManagedMemoryError::InvalidManagedOperation)
            );
        }
        for length in 0..encoded.len() {
            assert!(execute_session_operation(
                &mut heap,
                &layouts,
                Some(&service),
                &encoded[..length],
                &words
            )
            .is_err());
        }
        assert!(execute_session_operation(
            &mut heap,
            &layouts,
            Some(&service),
            &encoded,
            &words[..words.len() - 1]
        )
        .is_err());
        let mut extra = words.clone();
        extra.push(id);
        assert!(
            execute_session_operation(&mut heap, &layouts, Some(&service), &encoded, &extra)
                .is_err()
        );
        for index in 0..words.len() {
            for invalid in [0, -1, if encoded[6] == CURRENT { id } else { none }] {
                let mut args = words.clone();
                args[index] = invalid;
                assert!(execute_session_operation(
                    &mut heap,
                    &layouts,
                    Some(&service),
                    &encoded,
                    &args
                )
                .is_err());
            }
        }
        assert_eq!(heap.allocated_bytes(), before);
        service
            .with_runtime(|state| {
                assert_eq!(state.snapshots().len(), 1);
                assert!(state.is_live(&session));
                assert_eq!(
                    http_session::get(state, &session, "key").unwrap(),
                    Some("original".into())
                );
            })
            .unwrap();
    }
}

#[test]
fn lookup_rejects_invalid_result_layout_before_creating_session_state() {
    let (mut heap, _, option, lookup) = fixture();
    let string = ManagedFieldType::Reference(managed_string_semantic_id());
    let service =
        VmHttpSessionService::new(VmHttpSessionRuntime::new("layout-admission", 100).unwrap());
    for result in [
        ManagedAggregateDescriptor::tuple("Tuple(String,Bool)", vec![string; 3]).unwrap(),
        ManagedAggregateDescriptor::tuple("Tuple(String,Bool)", vec![string, string]).unwrap(),
        ManagedAggregateDescriptor::tuple(
            "Tuple(String,Bool)",
            vec![ManagedFieldType::Bool, string],
        )
        .unwrap(),
        ManagedAggregateDescriptor::record(
            "Tuple(String,Bool)",
            vec![
                ("identity".into(), string),
                ("created".into(), ManagedFieldType::Bool),
            ],
        )
        .unwrap(),
    ] {
        let descriptors = [
            ManagedAggregateDescriptor::constructor("Apply(Option;String)", "None", 0, 2, vec![])
                .unwrap(),
            result,
        ];
        let encoded = descriptors
            .iter()
            .map(|descriptor| TvmManagedLayoutDescriptor {
                semantic_id: descriptor.managed().semantic_id().bytes(),
                encoded_layout: encode_aggregate_layout(descriptor).unwrap(),
            })
            .collect::<Vec<_>>();
        let layouts = ManagedLayoutRegistry::from_image(&encoded, &[], &[]).unwrap();
        let none = allocate_option_string(&mut heap, &layouts, option, None).unwrap() as i64;
        let before = heap.allocated_bytes();
        assert!(execute_session_operation(
            &mut heap,
            &layouts,
            Some(&service),
            &encode_session_current_operation(option, lookup),
            &[none]
        )
        .is_err());
        assert_eq!(heap.allocated_bytes(), before);
        assert!(service
            .with_runtime(|state| state.snapshots().is_empty())
            .unwrap());
    }
}
