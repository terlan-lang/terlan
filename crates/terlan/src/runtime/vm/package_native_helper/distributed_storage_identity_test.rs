//! An existing adapter cannot silently adopt a different durable store on reopen.

use super::*;
use crate::terlan_native_boundary::term::{
    NativeBoundaryReplyTerm as Reply, NativeBoundaryTerm as Term,
};

fn adapter(
    runtime: &mut VmDistributedStorageRuntime,
    state: &mut VmDistributedStateRuntime,
    expected_identity: Option<[u8; 32]>,
) -> ReplValue {
    runtime.bind([("primary".to_owned(), expected_identity)].into_iter());
    let mode = call(runtime, state, "durable", vec![]);
    let policy = call(
        runtime,
        state,
        "policy",
        vec![
            ReplValue::String("primary".into()),
            mode,
            ReplValue::Bool(true),
        ],
    );
    call(runtime, state, "adapter", vec![policy])
}

#[test]
fn reopening_a_changed_binding_fences_the_old_adapter_without_repinning() {
    let mut runtime = VmDistributedStorageRuntime::default();
    let mut state = VmDistributedStateRuntime::default();
    let adapter = adapter(&mut runtime, &mut state, None);
    let open = request("std.vm.distributed_storage.open", vec![adapter.clone()]);
    let pending = runtime.prepare(1, &open).unwrap().unwrap().pending;
    runtime
        .complete(1, pending, open_reply(1, 7, 2, 5))
        .unwrap();
    for _ in 0..2 {
        let pending = runtime.prepare(1, &open).unwrap().unwrap().pending;
        let mismatch = runtime
            .complete(1, pending, open_reply(2, 0, 1, 0))
            .unwrap();
        assert_eq!(
            call(&mut runtime, &mut state, "kind", vec![mismatch.clone()]),
            ReplValue::String("storage_identity_mismatch".into())
        );
        assert_eq!(
            call(
                &mut runtime,
                &mut state,
                "is_failure",
                vec![mismatch.clone()]
            ),
            ReplValue::Bool(true)
        );
        assert_eq!(
            call(&mut runtime, &mut state, "sequence", vec![mismatch.clone()]),
            ReplValue::Int(7)
        );
        assert_eq!(
            call(&mut runtime, &mut state, "recovery_action", vec![mismatch]),
            ReplValue::String("rebind_storage".into())
        );
        assert!(runtime
            .prepare(
                1,
                &request("std.vm.distributed_storage.flush", vec![adapter.clone()])
            )
            .unwrap()
            .is_none());
        assert!(runtime
            .prepare(
                1,
                &request(
                    "std.vm.distributed_storage.atomic_append_proof",
                    vec![adapter.clone()]
                )
            )
            .is_err());
    }
    let pending = runtime.prepare(1, &open).unwrap().unwrap().pending;
    let reopened = runtime
        .complete(1, pending, open_reply(1, 7, 2, 5))
        .unwrap();
    assert_eq!(
        call(&mut runtime, &mut state, "kind", vec![reopened]),
        ReplValue::String("opened".into())
    );
}

#[test]
fn malformed_open_observations_do_not_pin_an_identity() {
    let mut runtime = VmDistributedStorageRuntime::default();
    let mut state = VmDistributedStateRuntime::default();
    let adapter = adapter(&mut runtime, &mut state, None);
    let open = request("std.vm.distributed_storage.open", vec![adapter]);
    let short = Reply::Ok(Term::Tuple(vec![
        Term::Bytes(vec![1; 31]),
        Term::Tuple(vec![Term::Int(0), Term::Int(1), Term::Int(0)]),
    ]));
    for reply in [
        short,
        open_reply(0, 0, 1, 0),
        open_reply(1, -1, 1, 0),
        open_reply(1, 0, 0, 0),
    ] {
        let pending = runtime.prepare(1, &open).unwrap().unwrap().pending;
        assert!(runtime.complete(1, pending, reply).is_err());
    }
    let pending = runtime.prepare(1, &open).unwrap().unwrap().pending;
    let opened = runtime
        .complete(1, pending, open_reply(2, 0, 1, 0))
        .unwrap();
    assert_eq!(
        call(&mut runtime, &mut state, "kind", vec![opened]),
        ReplValue::String("opened".into())
    );
}

#[test]
fn supervisor_identity_pin_survives_fresh_runtime_and_adapter_creation() {
    for _ in 0..2 {
        let mut runtime = VmDistributedStorageRuntime::default();
        let mut state = VmDistributedStateRuntime::default();
        for _ in 0..2 {
            let adapter = adapter(&mut runtime, &mut state, Some([1; 32]));
            let open = request("std.vm.distributed_storage.open", vec![adapter.clone()]);
            for _ in 0..2 {
                let pending = runtime.prepare(1, &open).unwrap().unwrap().pending;
                let rejected = runtime
                    .complete(1, pending, open_reply(2, 99, 7, 90))
                    .unwrap();
                assert_eq!(
                    call(&mut runtime, &mut state, "kind", vec![rejected.clone()]),
                    ReplValue::String("storage_identity_mismatch".into())
                );
                assert_eq!(
                    call(&mut runtime, &mut state, "sequence", vec![rejected]),
                    ReplValue::Int(0)
                );
                assert!(runtime
                    .prepare(
                        1,
                        &request(
                            "std.vm.distributed_storage.atomic_append_proof",
                            vec![adapter.clone()]
                        )
                    )
                    .is_err());
            }
            let pending = runtime.prepare(1, &open).unwrap().unwrap().pending;
            let accepted = runtime
                .complete(1, pending, open_reply(1, 7, 2, 5))
                .unwrap();
            assert_eq!(
                call(&mut runtime, &mut state, "kind", vec![accepted.clone()]),
                ReplValue::String("opened".into())
            );
            assert_eq!(
                call(&mut runtime, &mut state, "sequence", vec![accepted]),
                ReplValue::Int(7)
            );
        }
    }
}
