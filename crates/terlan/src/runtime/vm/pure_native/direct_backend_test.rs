//! Descriptor-directed public value conversion tests for direct execution.

use std::sync::Arc;

use crate::runtime::native_image::managed::{
    encode_aggregate_layout, encode_collection_layout, ManagedAggregateDescriptor,
    ManagedCollectionDescriptor, ManagedFieldType, SemanticTypeId,
};
use crate::runtime::native_image::{TvmManagedCollectionDescriptor, TvmManagedLayoutDescriptor};
use crate::runtime::vm::pure_native::{PureNativeExecutionImage, PureNativeExecutionRuntime};

use super::*;

/// Requires shard-migratable parked state at compile time.
fn assert_thread_neutral<T: Send + Sync + 'static>() {}

/// Prevents shard execution state or shared code from acquiring thread identity.
#[test]
fn direct_backend_parked_state_is_send_sync_and_static() {
    assert_thread_neutral::<PureNativeExecutionRuntime>();
    assert_thread_neutral::<PureNativeExecutionImage>();
    assert_thread_neutral::<DirectNativeBackend>();
}

/// Verifies independent actors can park and resume without sharing continuation state.
#[test]
fn execution_runtime_interleaves_owner_scoped_continuations() {
    let mut runtime = PureNativeExecutionRuntime::runtime_default().expect("execution runtime");
    assert_eq!(runtime.allocate_request_id(), Ok(1));
    assert_eq!(runtime.allocate_request_id(), Ok(2));
    let mut fork = runtime.fork_empty();
    assert_eq!(fork.allocate_request_id(), Ok(1));
    runtime
        .park_continuation(19, 17, 23, None, None)
        .expect("first actor park");
    runtime
        .park_continuation(29, 31, 37, Some(TvmBoundaryType::Int), None)
        .expect("second actor park");
    assert_eq!(runtime.pending_continuation_count(), 2);
    assert!(runtime
        .ensure_idle()
        .expect_err("parked actors prevent graceful shutdown")
        .contains("2 parked continuation"));

    assert!(runtime
        .claim_continuation(19, 31, 37)
        .expect_err("cross-request resume must fail")
        .contains("does not own"));
    assert_eq!(runtime.pending_continuation_count(), 2);
    let claim = runtime
        .claim_continuation(29, 31, 37)
        .expect("second actor resume");
    assert_eq!(claim.owner_id(), 29);
    assert_eq!(claim.request_id(), 31);
    assert_eq!(claim.continuation_id(), 37);
    let (injected, managed) = claim.into_resume_state();
    assert_eq!(injected, Some(TvmBoundaryType::Int));
    assert!(managed.is_none());
    assert_eq!(runtime.pending_continuation_count(), 1);
    assert!(runtime
        .claim_continuation(29, 31, 37)
        .expect_err("consumed authority cannot resume twice")
        .contains("is not parked"));
    runtime.release_owner(19);
    assert_eq!(runtime.pending_continuation_count(), 0);
    runtime.ensure_idle().expect("all actors released");
}

#[test]
fn typed_transition_statuses_preserve_operation_metadata_and_capture_words() {
    let send =
        frame_from_status(1, 2, 22, 41, vec![7, 5, 0, 0, 101, 103]).expect("typed send frame");
    assert_eq!(
        send,
        TvmControlFrame::Transition {
            request_id: 1,
            owner_id: 2,
            continuation_id: 41,
            operation: TvmTransitionOperation::Send,
            arguments: vec![7, 5, 0, 0, 101],
            values: vec![103],
        }
    );
    let receive = frame_from_status(3, 4, 23, 43, vec![5, 0, 0, 107]).expect("typed receive frame");
    assert_eq!(
        receive,
        TvmControlFrame::Transition {
            request_id: 3,
            owner_id: 4,
            continuation_id: 43,
            operation: TvmTransitionOperation::Receive,
            arguments: vec![5, 0, 0],
            values: vec![107],
        }
    );
}

#[test]
fn capability_status_preserves_typed_rpc_arguments_and_separates_captures() {
    let mut expected_arguments = vec![2];
    expected_arguments.extend(TvmBoundaryType::Bool.transition_words());
    expected_arguments.push(101);
    let mut transport = expected_arguments.clone();
    transport.push(103);
    let frame = frame_from_status(7, 11, 24, 47, transport).expect("capability frame");
    assert_eq!(
        frame,
        TvmControlFrame::Transition {
            request_id: 7,
            owner_id: 11,
            continuation_id: 47,
            operation: TvmTransitionOperation::Capability,
            arguments: expected_arguments,
            values: vec![103],
        }
    );
}

#[test]
fn typed_receive_continuation_requires_its_exact_managed_result_type() {
    let continuation = TvmContinuationDescriptor {
        id: 109,
        parameters: vec![TvmBoundaryType::String, TvmBoundaryType::Int],
        results: vec![TvmBoundaryType::String],
    };
    let injected = transition_injected_type(
        &TvmTransitionOperation::Receive,
        &TvmBoundaryType::String.transition_words(),
    )
    .expect("typed receive metadata");
    let (result, captures) = split_continuation_types(injected.as_ref(), &continuation)
        .expect("typed continuation split");
    assert_eq!(result, [TvmBoundaryType::String]);
    assert_eq!(captures, [TvmBoundaryType::Int]);

    let wrong = TvmContinuationDescriptor {
        parameters: vec![TvmBoundaryType::Bytes],
        ..continuation
    };
    assert!(split_continuation_types(injected.as_ref(), &wrong)
        .expect_err("incompatible managed receive must fail")
        .contains("expected String"));
}

/// Builds one image-table row from a checked aggregate descriptor.
fn admitted_layout(descriptor: ManagedAggregateDescriptor) -> TvmManagedLayoutDescriptor {
    TvmManagedLayoutDescriptor {
        semantic_id: descriptor.managed().semantic_id().bytes(),
        encoded_layout: encode_aggregate_layout(&descriptor).expect("encode aggregate layout"),
    }
}

/// Derives one checked semantic identity for an aggregate test shape.
fn semantic(canonical: &str) -> SemanticTypeId {
    SemanticTypeId::from_canonical(canonical).expect("semantic identity")
}

/// Builds admitted tuple, array, record, and constructor layouts with nested references.
fn aggregate_layouts() -> Vec<TvmManagedLayoutDescriptor> {
    let scores = ManagedAggregateDescriptor::fixed_array("app.Scores", ManagedFieldType::Int, 3)
        .expect("scores layout");
    let user = ManagedAggregateDescriptor::record(
        "app.User",
        vec![
            (
                "name".to_string(),
                ManagedFieldType::Reference(semantic("std.core.String")),
            ),
            (
                "scores".to_string(),
                ManagedFieldType::Reference(semantic("app.Scores")),
            ),
        ],
    )
    .expect("user layout");
    let pair = ManagedAggregateDescriptor::tuple(
        "app.Pair",
        vec![ManagedFieldType::Int, ManagedFieldType::Bool],
    )
    .expect("pair layout");
    let ok = ManagedAggregateDescriptor::constructor(
        "app.Result",
        "Ok",
        0,
        2,
        vec![(
            Some("value".to_string()),
            ManagedFieldType::Reference(semantic("app.User")),
        )],
    )
    .expect("Ok layout");
    let error = ManagedAggregateDescriptor::constructor(
        "app.Result",
        "Error",
        1,
        2,
        vec![(
            Some("reason".to_string()),
            ManagedFieldType::Reference(semantic("std.core.String")),
        )],
    )
    .expect("Error layout");
    let mut layouts = vec![pair, scores, user, ok, error]
        .into_iter()
        .map(admitted_layout)
        .collect::<Vec<_>>();
    layouts.sort_by(|left, right| {
        left.semantic_id
            .cmp(&right.semantic_id)
            .then_with(|| left.encoded_layout.cmp(&right.encoded_layout))
    });
    layouts
}

/// Builds one image-table row from a checked collection descriptor.
fn admitted_collection(descriptor: ManagedCollectionDescriptor) -> TvmManagedCollectionDescriptor {
    TvmManagedCollectionDescriptor {
        semantic_id: descriptor.semantic_id().bytes(),
        encoded_layout: encode_collection_layout(&descriptor).expect("encode collection schema"),
    }
}

/// Builds nested List, Map, and Set schemas used by the public boundary.
fn collection_layouts() -> Vec<TvmManagedCollectionDescriptor> {
    let list =
        ManagedCollectionDescriptor::list("List(Int)", ManagedFieldType::Int).expect("list schema");
    let map = ManagedCollectionDescriptor::map(
        "Apply(Map;String,List(Int))",
        ManagedFieldType::Reference(semantic("std.core.String")),
        ManagedFieldType::Reference(semantic("List(Int)")),
    )
    .expect("map schema");
    let set = ManagedCollectionDescriptor::set(
        "Apply(Set;String)",
        ManagedFieldType::Reference(semantic("std.core.String")),
    )
    .expect("set schema");
    let mut layouts = vec![list, map, set]
        .into_iter()
        .map(admitted_collection)
        .collect::<Vec<_>>();
    layouts.sort_by(|left, right| {
        left.semantic_id
            .cmp(&right.semantic_id)
            .then_with(|| left.encoded_layout.cmp(&right.encoded_layout))
    });
    layouts
}

/// Round-trips public UTF-8 through one owner-local managed word.
#[test]
fn public_string_round_trip_preserves_content_and_owner() {
    let mut managed = ManagedExecutionRuntime::runtime_default().expect("managed runtime");
    let input = ReplValue::String("Terlan \u{03bb}".to_string());
    let word = encode_public_argument(&mut managed, 71, &TvmBoundaryType::String, &input)
        .expect("encode String");

    assert_eq!(
        decode_public_result(&managed, 71, &TvmBoundaryType::String, word).expect("decode String"),
        input
    );
    let foreign = decode_public_result(&managed, 72, &TvmBoundaryType::String, word)
        .expect_err("foreign owner");
    assert!(foreign.contains("owner 72 has no managed heap"));
    let mistyped = decode_public_result(&managed, 71, &TvmBoundaryType::Bytes, word)
        .expect_err("wrong managed identity");
    assert!(mistyped.contains("wrong semantic type"));
}

/// Round-trips byte sequences and non-byte-aligned bitstrings without exposing references.
#[test]
fn public_bytes_and_binary_round_trip_into_owned_values() {
    let mut managed = ManagedExecutionRuntime::runtime_default().expect("managed runtime");
    let bytes = ReplValue::Bytes(Arc::from(&b"a\0b"[..]));
    let bytes_word = encode_public_argument(&mut managed, 73, &TvmBoundaryType::Bytes, &bytes)
        .expect("encode Bytes");
    assert_eq!(
        decode_public_result(&managed, 73, &TvmBoundaryType::Bytes, bytes_word)
            .expect("decode Bytes"),
        bytes
    );

    let binary =
        ReplValue::BitString(VmBitString::from_bytes([0b1011_0000], 4).expect("four-bit value"));
    let binary_word = encode_public_argument(&mut managed, 73, &TvmBoundaryType::Binary, &binary)
        .expect("encode Binary");
    assert_eq!(
        decode_public_result(&managed, 73, &TvmBoundaryType::Binary, binary_word)
            .expect("decode Binary"),
        binary
    );
}

/// Rejects public values that do not match the exact descriptor identity.
#[test]
fn public_conversion_rejects_mismatched_and_opaque_managed_values() {
    let mut managed = ManagedExecutionRuntime::runtime_default().expect("managed runtime");
    let mismatch = encode_public_argument(
        &mut managed,
        74,
        &TvmBoundaryType::String,
        &ReplValue::Int(4),
    )
    .expect_err("mismatched public argument");
    assert!(mismatch.contains("does not match `String`"));
    assert_eq!(managed.actor_count(), 0);

    let opaque = decode_public_result(&managed, 74, &TvmBoundaryType::Managed([7; 16]), 1)
        .expect_err("aggregate metadata required");
    assert!(opaque.contains("owner 74 has no managed heap"));
}

/// Round-trips every fixed aggregate family through descriptor-owned heap storage.
#[test]
fn public_fixed_aggregates_round_trip_through_admitted_layouts() {
    let mut managed = ManagedExecutionRuntime::with_image_layouts(&aggregate_layouts())
        .expect("managed aggregate runtime");
    let cases = [
        (
            "app.Pair",
            ReplValue::Tuple(vec![ReplValue::Int(7), ReplValue::Bool(true)]),
        ),
        (
            "app.Scores",
            ReplValue::List(vec![
                ReplValue::Int(1),
                ReplValue::Int(2),
                ReplValue::Int(3),
            ]),
        ),
        (
            "app.User",
            ReplValue::Record {
                name: "User".to_string(),
                fields: vec![
                    ("name".to_string(), ReplValue::String("Ada".to_string())),
                    (
                        "scores".to_string(),
                        ReplValue::List(vec![
                            ReplValue::Int(5),
                            ReplValue::Int(8),
                            ReplValue::Int(13),
                        ]),
                    ),
                ],
            },
        ),
        (
            "app.Result",
            ReplValue::Record {
                name: "Error".to_string(),
                fields: vec![(
                    "reason".to_string(),
                    ReplValue::String("denied".to_string()),
                )],
            },
        ),
    ];
    for (index, (canonical, expected)) in cases.into_iter().enumerate() {
        let boundary = TvmBoundaryType::Managed(semantic(canonical).bytes());
        let owner = 80 + index as u64;
        let word = encode_public_argument(&mut managed, owner, &boundary, &expected)
            .expect("encode fixed aggregate");
        assert_eq!(
            decode_public_result(&managed, owner, &boundary, word)
                .expect("materialize fixed aggregate"),
            expected
        );
    }
}

/// Resolves a nested constructor variant and preserves all named field identities.
#[test]
fn public_constructor_round_trip_selects_active_variant() {
    let mut managed = ManagedExecutionRuntime::with_image_layouts(&aggregate_layouts())
        .expect("managed aggregate runtime");
    let expected = ReplValue::Record {
        name: "Ok".to_string(),
        fields: vec![(
            "value".to_string(),
            ReplValue::Record {
                name: "User".to_string(),
                fields: vec![
                    ("name".to_string(), ReplValue::String("Grace".to_string())),
                    (
                        "scores".to_string(),
                        ReplValue::List(vec![
                            ReplValue::Int(2),
                            ReplValue::Int(3),
                            ReplValue::Int(5),
                        ]),
                    ),
                ],
            },
        )],
    };
    let boundary = TvmBoundaryType::Managed(semantic("app.Result").bytes());
    let word = encode_public_argument(&mut managed, 91, &boundary, &expected)
        .expect("encode nested constructor");

    assert_eq!(
        decode_public_result(&managed, 91, &boundary, word).expect("decode nested constructor"),
        expected
    );
    assert!(decode_public_result(&managed, 92, &boundary, word)
        .expect_err("foreign aggregate owner")
        .contains("owner 92 has no managed heap"));
}

/// Rolls back nested allocations when a later aggregate field violates its layout.
#[test]
fn public_aggregate_failure_is_atomic_and_rejects_unknown_shapes() {
    let mut managed = ManagedExecutionRuntime::with_image_layouts(&aggregate_layouts())
        .expect("managed aggregate runtime");
    let boundary = TvmBoundaryType::Managed(semantic("app.User").bytes());
    let malformed = ReplValue::Record {
        name: "User".to_string(),
        fields: vec![
            (
                "name".to_string(),
                ReplValue::String("allocated first".to_string()),
            ),
            (
                "scores".to_string(),
                ReplValue::List(vec![
                    ReplValue::Int(1),
                    ReplValue::Bool(false),
                    ReplValue::Int(3),
                ]),
            ),
        ],
    };
    let error = encode_public_argument(&mut managed, 93, &boundary, &malformed)
        .expect_err("nested field mismatch");
    assert!(error.contains("managed_field"));
    assert_eq!(managed.heap_usage(93), Some((0, 0)));

    let absent = TvmBoundaryType::Managed(semantic("app.Missing").bytes());
    let error = encode_public_argument(
        &mut managed,
        94,
        &absent,
        &ReplValue::Tuple(vec![ReplValue::Int(1)]),
    )
    .expect_err("missing admitted layout");
    assert!(error.contains("no admitted fixed layout"));
    assert_eq!(managed.heap_usage(94), Some((0, 0)));
}

/// Rejects recursive public graphs beyond the conversion bound before publication.
#[test]
fn public_aggregate_depth_limit_and_atom_metadata_fail_closed() {
    let end = ManagedAggregateDescriptor::constructor("app.Node", "End", 0, 2, Vec::new())
        .expect("End layout");
    let next = ManagedAggregateDescriptor::constructor(
        "app.Node",
        "Next",
        1,
        2,
        vec![(
            Some("next".to_string()),
            ManagedFieldType::Reference(semantic("app.Node")),
        )],
    )
    .expect("Next layout");
    let atom = ManagedAggregateDescriptor::record(
        "app.Tagged",
        vec![("tag".to_string(), ManagedFieldType::Atom)],
    )
    .expect("atom layout");
    let mut layouts = vec![end, next, atom]
        .into_iter()
        .map(admitted_layout)
        .collect::<Vec<_>>();
    layouts.sort_by(|left, right| {
        left.semantic_id
            .cmp(&right.semantic_id)
            .then_with(|| left.encoded_layout.cmp(&right.encoded_layout))
    });
    let mut managed =
        ManagedExecutionRuntime::with_image_layouts(&layouts).expect("recursive runtime");
    let mut value = ReplValue::Record {
        name: "End".to_string(),
        fields: Vec::new(),
    };
    for _ in 0..258 {
        value = ReplValue::Record {
            name: "Next".to_string(),
            fields: vec![("next".to_string(), value)],
        };
    }
    let node = TvmBoundaryType::Managed(semantic("app.Node").bytes());
    assert!(encode_public_argument(&mut managed, 95, &node, &value)
        .expect_err("depth limit")
        .contains("managed_budget"));
    assert_eq!(managed.heap_usage(95), Some((0, 0)));

    let tagged = ReplValue::Record {
        name: "Tagged".to_string(),
        fields: vec![("tag".to_string(), ReplValue::Atom("ready".to_string()))],
    };
    let tagged_boundary = TvmBoundaryType::Managed(semantic("app.Tagged").bytes());
    assert!(
        encode_public_argument(&mut managed, 96, &tagged_boundary, &tagged)
            .expect_err("missing admitted atom table")
            .contains("managed_atom")
    );
    assert_eq!(managed.heap_usage(96), Some((0, 0)));
}

/// Round-trips standalone, aggregate, and collection atoms through one image table.
#[test]
fn public_atoms_round_trip_through_canonical_image_identity() {
    let tagged = ManagedAggregateDescriptor::record(
        "app.Tagged",
        vec![("tag".to_string(), ManagedFieldType::Atom)],
    )
    .expect("atom aggregate");
    let collections = [
        ManagedCollectionDescriptor::list("List(Atom)", ManagedFieldType::Atom).expect("atom list"),
        ManagedCollectionDescriptor::map(
            "Apply(Map;Atom,Int)",
            ManagedFieldType::Atom,
            ManagedFieldType::Int,
        )
        .expect("atom map"),
        ManagedCollectionDescriptor::set("Apply(Set;Atom)", ManagedFieldType::Atom)
            .expect("atom set"),
    ]
    .into_iter()
    .map(admitted_collection)
    .collect::<Vec<_>>();
    let atoms = vec!["error".to_owned(), "pending".to_owned(), "ready".to_owned()];
    let mut managed = ManagedExecutionRuntime::with_image_metadata(
        &[admitted_layout(tagged)],
        &collections,
        &atoms,
    )
    .expect("atom-aware runtime");

    let ready = ReplValue::Atom("ready".to_owned());
    let word = encode_public_argument(&mut managed, 97, &TvmBoundaryType::Atom, &ready)
        .expect("encode standalone atom");
    assert_eq!(
        decode_public_result(&managed, 999, &TvmBoundaryType::Atom, word)
            .expect("decode image-local atom"),
        ready
    );

    let values = [
        (
            "app.Tagged",
            ReplValue::Record {
                name: "Tagged".to_owned(),
                fields: vec![("tag".to_owned(), ready.clone())],
            },
        ),
        (
            "List(Atom)",
            ReplValue::List(vec![ready.clone(), ReplValue::Atom("error".to_owned())]),
        ),
        (
            "Apply(Map;Atom,Int)",
            ReplValue::Map(vec![
                (ready.clone(), ReplValue::Int(1)),
                (ReplValue::Atom("error".to_owned()), ReplValue::Int(2)),
            ]),
        ),
        (
            "Apply(Set;Atom)",
            ReplValue::Set(vec![
                ready.clone(),
                ReplValue::Atom("pending".to_owned()),
                ready.clone(),
            ]),
        ),
    ];
    for (offset, (canonical, value)) in values.into_iter().enumerate() {
        let owner = 98 + offset as u64;
        let boundary = TvmBoundaryType::Managed(semantic(canonical).bytes());
        let word = encode_public_argument(&mut managed, owner, &boundary, &value)
            .expect("encode atom-bearing managed value");
        let decoded = decode_public_result(&managed, owner, &boundary, word)
            .expect("decode atom-bearing managed value");
        if canonical == "Apply(Set;Atom)" {
            assert_eq!(
                decoded,
                ReplValue::Set(vec![ready.clone(), ReplValue::Atom("pending".to_owned())])
            );
        } else {
            assert_eq!(decoded, value);
        }
    }

    assert!(encode_public_argument(
        &mut managed,
        110,
        &TvmBoundaryType::Atom,
        &ReplValue::Atom("unknown".to_owned()),
    )
    .expect_err("unknown atom")
    .contains("tvm.atom.unknown"));
    assert!(
        decode_public_result(&managed, 110, &TvmBoundaryType::Atom, i64::MAX)
            .expect_err("invalid atom word")
            .contains("invalid atom index")
    );
}

/// Round-trips nested public collections using their canonical managed profiles.
#[test]
fn public_collections_round_trip_nested_values_and_structural_keys() {
    let mut managed = ManagedExecutionRuntime::with_image_metadata(
        &aggregate_layouts(),
        &collection_layouts(),
        &[],
    )
    .expect("managed collection runtime");
    let list = ReplValue::List((0..97).map(ReplValue::Int).collect());
    let list_boundary = TvmBoundaryType::Managed(semantic("List(Int)").bytes());
    let list_word =
        encode_public_argument(&mut managed, 101, &list_boundary, &list).expect("encode RRB list");
    assert_eq!(
        decode_public_result(&managed, 101, &list_boundary, list_word).expect("decode RRB list"),
        list
    );

    let map = ReplValue::Map(vec![
        (
            ReplValue::String("first".to_string()),
            ReplValue::List(vec![ReplValue::Int(1)]),
        ),
        (
            ReplValue::String("second".to_string()),
            ReplValue::List(vec![ReplValue::Int(2), ReplValue::Int(3)]),
        ),
        (
            ReplValue::String("first".to_string()),
            ReplValue::List(vec![ReplValue::Int(8)]),
        ),
    ]);
    let expected_map = ReplValue::Map(vec![
        (
            ReplValue::String("first".to_string()),
            ReplValue::List(vec![ReplValue::Int(8)]),
        ),
        (
            ReplValue::String("second".to_string()),
            ReplValue::List(vec![ReplValue::Int(2), ReplValue::Int(3)]),
        ),
    ]);
    let map_boundary = TvmBoundaryType::Managed(semantic("Apply(Map;String,List(Int))").bytes());
    let map_word =
        encode_public_argument(&mut managed, 102, &map_boundary, &map).expect("encode nested map");
    assert_eq!(
        decode_public_result(&managed, 102, &map_boundary, map_word).expect("decode nested map"),
        expected_map
    );

    let set = ReplValue::Set(vec![
        ReplValue::String("alpha".to_string()),
        ReplValue::String("beta".to_string()),
        ReplValue::String("alpha".to_string()),
    ]);
    let set_boundary = TvmBoundaryType::Managed(semantic("Apply(Set;String)").bytes());
    let set_word =
        encode_public_argument(&mut managed, 103, &set_boundary, &set).expect("encode managed set");
    assert_eq!(
        decode_public_result(&managed, 103, &set_boundary, set_word).expect("decode managed set"),
        ReplValue::Set(vec![
            ReplValue::String("alpha".to_string()),
            ReplValue::String("beta".to_string()),
        ])
    );
}

/// Exercises indexed collection promotion without exposing physical storage publicly.
#[test]
fn public_collections_cross_indexed_map_and_set_profiles() {
    let mut managed = ManagedExecutionRuntime::with_image_metadata(&[], &collection_layouts(), &[])
        .expect("indexed collection runtime");
    let map = ReplValue::Map(
        (0..140)
            .map(|index| {
                (
                    ReplValue::String(format!("key-{index:03}")),
                    ReplValue::List(vec![ReplValue::Int(index)]),
                )
            })
            .collect(),
    );
    let map_boundary = TvmBoundaryType::Managed(semantic("Apply(Map;String,List(Int))").bytes());
    let map_word =
        encode_public_argument(&mut managed, 104, &map_boundary, &map).expect("encode indexed map");
    assert_eq!(
        decode_public_result(&managed, 104, &map_boundary, map_word).expect("decode indexed map"),
        map
    );

    let set = ReplValue::Set(
        (0..140)
            .map(|index| ReplValue::String(format!("member-{index:03}")))
            .collect(),
    );
    let set_boundary = TvmBoundaryType::Managed(semantic("Apply(Set;String)").bytes());
    let set_word =
        encode_public_argument(&mut managed, 105, &set_boundary, &set).expect("encode indexed set");
    assert_eq!(
        decode_public_result(&managed, 105, &set_boundary, set_word).expect("decode indexed set"),
        set
    );
}

/// Rolls back all nested collection allocations after a late field mismatch.
#[test]
fn public_collection_failure_is_atomic_and_schema_directed() {
    let mut managed = ManagedExecutionRuntime::with_image_metadata(&[], &collection_layouts(), &[])
        .expect("managed collection runtime");
    let list_boundary = TvmBoundaryType::Managed(semantic("List(Int)").bytes());
    let malformed = ReplValue::List(vec![
        ReplValue::Int(1),
        ReplValue::Int(2),
        ReplValue::String("wrong".to_string()),
    ]);
    assert!(
        encode_public_argument(&mut managed, 106, &list_boundary, &malformed)
            .expect_err("late list field mismatch")
            .contains("managed_field")
    );
    assert_eq!(managed.heap_usage(106), Some((0, 0)));

    let map_boundary = TvmBoundaryType::Managed(semantic("Apply(Map;String,List(Int))").bytes());
    assert!(encode_public_argument(
        &mut managed,
        107,
        &map_boundary,
        &ReplValue::Tuple(Vec::new()),
    )
    .expect_err("wrong public collection shape")
    .contains("managed_collection"));
    assert_eq!(managed.heap_usage(107), Some((0, 0)));
}
