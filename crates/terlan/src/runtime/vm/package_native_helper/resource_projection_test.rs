use super::*;
use crate::runtime::native_image::TvmBoundaryType;

fn request(method: &str, args: Vec<ReplValue>) -> PureNativeCapabilityRequest {
    PureNativeCapabilityRequest {
        capability: "package-native".into(),
        operation: format!("std.data.json.{method}"),
        arguments: vec![],
        package_arguments: Some(args),
        result_type: TvmBoundaryType::Unit,
    }
}

fn value(reply: Reply) -> ReplValue {
    let Reply::Ok(term) = reply else {
        panic!("expected success");
    };
    managed_capability_term(term).unwrap()
}

fn allocate(adapter: &mut Adapter, owner: u64) -> ReplValue {
    let (_, projection) = adapter.prepare(owner, &request("array", vec![])).unwrap();
    value(
        adapter
            .complete(
                owner,
                projection,
                Reply::Ok(Term::Handle {
                    id: 41,
                    generation: 9,
                }),
            )
            .unwrap(),
    )
}

#[test]
fn registered_handles_roundtrip_without_exposing_worker_identity_and_mutation_preserves_identity() {
    let mut adapter = Adapter::default();
    let handle = allocate(&mut adapter, 7);
    let (args, projection) = adapter
        .prepare(
            7,
            &request("array_push", vec![handle.clone(), handle.clone()]),
        )
        .unwrap();
    assert_eq!(
        args,
        vec![
            Term::Handle {
                id: 41,
                generation: 9
            };
            2
        ]
    );
    assert_eq!(
        value(
            adapter
                .complete(
                    7,
                    projection,
                    Reply::Ok(Term::Handle {
                        id: 41,
                        generation: 9
                    })
                )
                .unwrap()
        ),
        handle
    );
    let ReplValue::Record { fields, .. } = &handle else {
        panic!("handle");
    };
    assert_eq!(
        direct_std::native_handle(fields).unwrap().unwrap().0,
        Handle {
            id: 1,
            generation: 1
        }
    );
    adapter.close_owner(7);
    assert!(adapter
        .prepare(7, &request("length", vec![handle]))
        .unwrap_err()
        .to_string()
        .contains("stale_handle"));
}

#[test]
fn foreign_forged_and_wrong_kind_source_handles_never_reach_worker() {
    let mut adapter = Adapter::default();
    let handle = allocate(&mut adapter, 7);
    assert!(adapter
        .prepare(8, &request("length", vec![handle.clone()]))
        .is_err());
    for (field, replacement) in [
        ("$native_owner", ReplValue::String("8".into())),
        ("$native_id", ReplValue::Int(42)),
        ("$native_generation", ReplValue::Int(9)),
        ("$native_type", ReplValue::String("std.io.Path.Path".into())),
    ] {
        let mut forged = handle.clone();
        let ReplValue::Record { fields, .. } = &mut forged else {
            panic!("handle");
        };
        *fields.iter_mut().find(|(name, _)| name == field).unwrap() = (field.into(), replacement);
        assert!(adapter
            .prepare(7, &request("length", vec![forged.clone()]))
            .is_err());
        assert!(adapter
            .prepare(8, &request("length", vec![forged]))
            .is_err());
    }
    let mut wrong_capability = request("array", vec![]);
    wrong_capability.capability = "filesystem".into();
    assert!(adapter.prepare(7, &wrong_capability).is_err());
    assert!(adapter
        .prepare(7, &request("array", vec![ReplValue::Unit]))
        .is_err());
    assert!(adapter.prepare(7, &request("array_extra", vec![])).is_err());
    for nested in [
        ReplValue::List(vec![handle.clone()]),
        ReplValue::Tuple(vec![handle.clone()]),
        ReplValue::Map(vec![(ReplValue::String("key".into()), handle)]),
    ] {
        assert!(adapter
            .prepare(
                7,
                &request("string_object_rows", vec![ReplValue::List(vec![]), nested])
            )
            .is_err());
    }
}

#[test]
fn incompatible_and_nested_worker_handles_are_rejected() {
    let mut adapter = Adapter::default();
    let handle = allocate(&mut adapter, 7);
    for term in [
        Term::Handle {
            id: 99,
            generation: 9,
        },
        Term::Bool(true),
    ] {
        let (_, projection) = adapter
            .prepare(
                7,
                &request("array_push", vec![handle.clone(), handle.clone()]),
            )
            .unwrap();
        assert!(adapter.complete(7, projection, Reply::Ok(term)).is_err());
    }
    for term in [
        Term::Handle {
            id: 1,
            generation: 1,
        },
        Term::List(vec![Term::Handle {
            id: 1,
            generation: 1,
        }]),
    ] {
        let (_, projection) = adapter
            .prepare(7, &request("length", vec![handle.clone()]))
            .unwrap();
        assert!(adapter.complete(7, projection, Reply::Ok(term)).is_err());
    }
    for (id, generation) in [(0, 1), (1, 0)] {
        let (_, projection) = adapter.prepare(7, &request("array", vec![])).unwrap();
        assert!(adapter
            .complete(7, projection, Reply::Ok(Term::Handle { id, generation }))
            .is_err());
    }
}

#[test]
fn package_errors_remain_typed_but_worker_loss_revokes_resources() {
    let mut adapter = Adapter::default();
    let handle = allocate(&mut adapter, 7);
    for code in [
        "capability_worker.operation_denied",
        "native_boundary.capability_denied",
        "resource.owner",
        "dispatch.contract",
        "dispatch.unknown_operation",
    ] {
        let (_, projection) = adapter
            .prepare(7, &request("length", vec![handle.clone()]))
            .unwrap();
        let denied = Reply::Error {
            code: code.into(),
            message: "denied".into(),
            offset: 0,
        };
        assert_eq!(
            adapter.complete(7, projection, denied.clone()).unwrap(),
            denied
        );
    }
    let (_, projection) = adapter
        .prepare(7, &request("parse", vec![ReplValue::String("{".into())]))
        .unwrap();
    let error = value(
        adapter
            .complete(
                7,
                projection,
                Reply::Error {
                    code: "json.parse".into(),
                    message: "invalid JSON".into(),
                    offset: 3,
                },
            )
            .unwrap(),
    );
    assert_eq!(
        error,
        direct_std::typed_result_error(
            NativeAdapterError::new("json.parse", "invalid JSON", 3),
            "JsonError"
        )
    );
    let (_, projection) = adapter
        .prepare(7, &request("length", vec![handle.clone()]))
        .unwrap();
    let lost = Reply::Error {
        code: "resource_worker.lost".into(),
        message: "lost".into(),
        offset: 0,
    };
    assert_eq!(adapter.complete(7, projection, lost.clone()).unwrap(), lost);
    assert!(adapter
        .prepare(7, &request("length", vec![handle]))
        .is_err());
}
