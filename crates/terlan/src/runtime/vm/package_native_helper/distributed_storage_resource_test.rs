//! Resource names resolve through pinned supervisor authority and real reply observations.

use super::*;

fn setup() -> (
    VmDistributedStorageRuntime,
    VmDistributedStateRuntime,
    ReplValue,
) {
    let mut runtime = VmDistributedStorageRuntime::default();
    runtime.bind(
        [
            ("primary".into(), Some([1; 32])),
            ("secondary".into(), Some([2; 32])),
            ("unpinned".into(), None),
        ]
        .into_iter(),
    );
    let mut state = VmDistributedStateRuntime::default();
    let policy = call(&mut runtime, &mut state, "force_local", vec![]);
    let adapter = call(&mut runtime, &mut state, "adapter", vec![policy]);
    call(&mut runtime, &mut state, "open", vec![adapter.clone()]);
    (runtime, state, adapter)
}

fn validation(adapter: &ReplValue, names: &[&str]) -> PureNativeCapabilityRequest {
    request(
        "std.vm.distributed_storage.validate_resource_handles",
        vec![
            adapter.clone(),
            ReplValue::List(
                names
                    .iter()
                    .map(|name| ReplValue::String((*name).into()))
                    .collect(),
            ),
        ],
    )
}

fn registration(adapter: &ReplValue, name: &str) -> PureNativeCapabilityRequest {
    request(
        "std.vm.distributed_storage.register_resource_handle",
        vec![adapter.clone(), ReplValue::String(name.into())],
    )
}

fn completed(step: Step) -> ReplValue {
    match step {
        Step::Complete(value) => value,
        Step::Continue(_) => panic!("unexpected continuation"),
    }
}

fn count(
    runtime: &mut VmDistributedStorageRuntime,
    state: &mut VmDistributedStateRuntime,
    adapter: &ReplValue,
) -> i64 {
    let proof = call(
        runtime,
        state,
        "resource_handle_validation_proof",
        vec![adapter.clone()],
    );
    let ReplValue::Int(count) = call(runtime, state, "resource_handle_count", vec![proof]) else {
        panic!("count")
    };
    count
}

#[test]
fn resources_require_supervisor_pins_checked_replies_and_atomic_proof_updates() {
    let (mut runtime, mut state, adapter) = setup();
    for name in ["db.unknown", "primary", "db.unpinned", ""] {
        let request = registration(&adapter, name);
        assert!(runtime.prepare(1, &request).unwrap().is_none());
        let outcome = runtime.call(1, &request, &[], &mut state).unwrap();
        assert_eq!(
            call(&mut runtime, &mut state, "kind", vec![outcome.clone()]),
            ReplValue::String("resource_handle_validation_failed".into())
        );
        assert_eq!(
            call(
                &mut runtime,
                &mut state,
                "missing_resource_handle",
                vec![outcome]
            ),
            ReplValue::String(name.into())
        );
    }
    assert_eq!(count(&mut runtime, &mut state, &adapter), 0);
    for (name, identity) in [("db.primary", 1), ("db.secondary", 2)] {
        let request = registration(&adapter, name);
        assert!(
            runtime.call(1, &request, &[], &mut state).is_err(),
            "no synchronous shortcut"
        );
        let prepared = runtime.prepare(1, &request).unwrap().unwrap();
        assert_eq!(prepared.backend, name.strip_prefix("db.").unwrap());
        let outcome = completed(
            runtime
                .complete_step(1, prepared.pending, open_reply(identity, 7, 1, 0))
                .unwrap(),
        );
        assert_eq!(
            call(&mut runtime, &mut state, "kind", vec![outcome]),
            ReplValue::String("resource_handles_validated".into())
        );
    }
    assert_eq!(count(&mut runtime, &mut state, &adapter), 2);
    let proof = call(
        &mut runtime,
        &mut state,
        "resource_handle_validation_proof",
        vec![adapter.clone()],
    );
    let request = validation(&adapter, &["db.primary", "db.secondary"]);
    let first = runtime.prepare(1, &request).unwrap().unwrap();
    let Step::Continue(second) = runtime
        .complete_step(1, first.pending, open_reply(1, 7, 1, 0))
        .unwrap()
    else {
        panic!("second observation required")
    };
    assert_eq!(second.backend, "secondary");
    assert_eq!(count(&mut runtime, &mut state, &adapter), 2);
    let rejected = completed(
        runtime
            .complete_step(1, second.pending, open_reply(3, 7, 1, 0))
            .unwrap(),
    );
    assert_eq!(
        call(
            &mut runtime,
            &mut state,
            "missing_resource_handle",
            vec![rejected]
        ),
        ReplValue::String("db.secondary".into())
    );
    assert_eq!(count(&mut runtime, &mut state, &adapter), 2);
    let first = runtime.prepare(1, &request).unwrap().unwrap();
    let Step::Continue(second) = runtime
        .complete_step(1, first.pending, open_reply(1, 8, 1, 0))
        .unwrap()
    else {
        panic!("second probe")
    };
    let validated = completed(
        runtime
            .complete_step(1, second.pending, open_reply(2, 99, 1, 0))
            .unwrap(),
    );
    assert_eq!(
        call(
            &mut runtime,
            &mut state,
            "validated_resource_count",
            vec![validated]
        ),
        ReplValue::Int(2)
    );
    let empty = validation(&adapter, &[]);
    assert!(runtime.prepare(1, &empty).unwrap().is_none());
    runtime.call(1, &empty, &[], &mut state).unwrap();
    assert_eq!(count(&mut runtime, &mut state, &adapter), 0);
    assert_eq!(
        call(
            &mut runtime,
            &mut state,
            "resource_handle_count",
            vec![proof]
        ),
        ReplValue::Int(2),
        "retained proof is immutable"
    );
}

#[test]
fn resource_observations_reject_stale_authority_owner_loss_and_malformed_replies() {
    let (mut runtime, mut state, adapter) = setup();
    let request = registration(&adapter, "db.primary");
    assert!(runtime.prepare(2, &request).is_err());
    let pending = runtime.prepare(1, &request).unwrap().unwrap();
    assert!(runtime
        .complete_step(2, pending.pending, open_reply(1, 0, 1, 0))
        .is_err());
    let pending = runtime.prepare(1, &request).unwrap().unwrap();
    assert!(runtime
        .complete_step(1, pending.pending, open_reply(0, 0, 1, 0))
        .is_err());
    let pending = runtime.prepare(1, &request).unwrap().unwrap();
    runtime.bindings.remove("primary");
    let rejected = completed(
        runtime
            .complete_step(1, pending.pending, open_reply(1, 0, 1, 0))
            .unwrap(),
    );
    assert_eq!(
        call(&mut runtime, &mut state, "is_failure", vec![rejected]),
        ReplValue::Bool(true)
    );
    assert_eq!(count(&mut runtime, &mut state, &adapter), 0);
    runtime.bind([("primary".into(), Some([1; 32]))].into_iter());
    let pending = runtime.prepare(1, &request).unwrap().unwrap();
    runtime.budget.charge(1, 64 * 1024 * 1024);
    assert!(runtime
        .complete_step(1, pending.pending, open_reply(1, 0, 1, 0))
        .is_err());
    runtime.budget.release(1);
    assert_eq!(count(&mut runtime, &mut state, &adapter), 0);
    assert!(runtime
        .prepare(1, &validation(&adapter, &["db.primary"]))
        .unwrap()
        .is_none());
    let pending = runtime.prepare(1, &request).unwrap().unwrap();
    runtime.close_owner(1);
    assert!(runtime
        .complete_step(1, pending.pending, open_reply(1, 0, 1, 0))
        .is_err());
}

#[test]
fn resource_lists_are_bounded_and_unconfigured_backends_never_issue_proofs() {
    let (mut runtime, mut state, adapter) = setup();
    for names in [vec!["db.primary"; 17], vec!["db.primary"; 2], vec!["a"; 1]] {
        let request = if names == ["a"] {
            registration(&adapter, &"a".repeat(68))
        } else {
            validation(&adapter, &names)
        };
        assert!(runtime.prepare(1, &request).unwrap().is_none());
        let outcome = runtime.call(1, &request, &[], &mut state).unwrap();
        assert_eq!(
            call(&mut runtime, &mut state, "kind", vec![outcome]),
            ReplValue::String("invalid_request".into())
        );
    }
    runtime.bindings.clear();
    let outcome = call(
        &mut runtime,
        &mut state,
        "require_resource_handle_validation",
        vec![adapter.clone()],
    );
    assert_eq!(
        call(&mut runtime, &mut state, "kind", vec![outcome]),
        ReplValue::String("unsupported".into())
    );
    let proof = request(
        "std.vm.distributed_storage.resource_handle_validation_proof",
        vec![adapter],
    );
    assert!(runtime.call(1, &proof, &[], &mut state).is_err());
}
