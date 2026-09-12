use super::PureNativeSuspension;

/// Requires scheduler-migratable state at compile time.
fn assert_thread_neutral<T: Send + Sync + 'static>() {}

/// Prevents parked scheduler state from acquiring thread-bound fields.
#[test]
fn parked_native_continuation_is_send_sync_and_static() {
    assert_thread_neutral::<PureNativeSuspension>();
}

/// Actual entry, fork, suspension and thread handoff retain the admitted table.
#[test]
fn parked_native_continuation_reuses_the_image_metadata_storage() {
    use crate::runtime::vm::actor::VmActorRuntime;
    use crate::runtime::vm::process::VmProcessSource;
    use crate::runtime::vm::pure_native::{
        pure_native_transport_test::typed_mailbox_boundary, PureNativeExecution,
        PureNativeExecutionContext, PureNativeExecutionRuntime,
    };

    let mut actors = VmActorRuntime::default();
    let owner = actors.spawn_root(VmProcessSource::new("typed.Mailbox", "round_trip", 0));
    let mut boundary = typed_mailbox_boundary();
    let table = boundary.artifact.as_ref().unwrap().continuations.clone();
    let fork = boundary.fork_empty().expect("fork image metadata");
    assert!(std::ptr::eq(
        table.get(901).unwrap(),
        fork.artifact
            .as_ref()
            .unwrap()
            .continuations
            .get(901)
            .unwrap(),
    ));
    let mut execution = PureNativeExecutionRuntime::runtime_default().unwrap();
    let mut context = PureNativeExecutionContext::new(owner, &mut execution);
    let PureNativeExecution::Suspended(suspension) = boundary
        .begin_call_for_actor(&mut actors, &mut context, "round_trip", &[])
        .expect("first real transition")
    else {
        panic!("mailbox call must suspend");
    };
    assert!(std::ptr::eq(
        table.get(901).unwrap(),
        boundary
            .call_cache
            .as_ref()
            .unwrap()
            .continuations
            .get(901)
            .unwrap(),
    ));
    let resumed = std::thread::spawn(move || suspension.into_resume_state())
        .join()
        .expect("handoff and consume parked state");
    assert!(std::ptr::eq(
        table.get(901).unwrap(),
        resumed.continuations.get(901).unwrap(),
    ));
    assert_eq!(resumed.owner_id, owner.as_u64());
}
