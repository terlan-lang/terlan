use super::super::*;
use crate::runtime::vm::memory::VmSharedAllocationKind;
use crate::runtime::vm::process_alias::VmProcessAliasOptions;
use crate::runtime::vm::resource::{
    VmResourceDescriptor, VmResourceEvent, VmResourceTransferPolicy,
};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Transfer", name, 0)
}

#[test]
fn parked_actor_runtime_moves_and_resumes_under_destination_owner() {
    let mut source_runtime = VmActorRuntime::default();
    let owner = source_runtime.spawn_root(source("run"));
    source_runtime
        .park_native_continuation(owner.as_u64(), 11, 13)
        .expect("park continuation");
    let transfer = source_runtime
        .detach_actor_runtime(owner)
        .expect("detach actor runtime");
    assert_eq!(transfer.owner(), owner);
    assert_eq!(transfer.native_continuation(), (11, 13));
    assert!(!source_runtime.is_alive(owner));

    let mut destination = VmActorRuntime::default();
    destination
        .import_actor_runtime(transfer)
        .expect("import actor runtime");
    assert!(destination.is_alive(owner));
    destination
        .resume_native_continuation(owner.as_u64(), 11, 13)
        .expect("resume imported continuation");
    assert_eq!(destination.pending_native_continuation_count(), 0);
}

#[test]
fn actor_runtime_collision_returns_complete_state_for_source_rollback() {
    let mut source_runtime = VmActorRuntime::default();
    let owner = source_runtime.spawn_root(source("source"));
    source_runtime
        .park_native_continuation(owner.as_u64(), 17, 19)
        .expect("park source");
    let transfer = source_runtime
        .detach_actor_runtime(owner)
        .expect("detach source");

    let mut destination = VmActorRuntime::default();
    assert_eq!(destination.spawn_root(source("collision")), owner);
    let failure = destination
        .import_actor_runtime(transfer)
        .expect_err("destination identity collision");
    assert!(failure.reason().contains("already contains"));
    source_runtime
        .import_actor_runtime(failure.into_transfer())
        .expect("restore source actor");
    assert_eq!(source_runtime.pending_native_continuation_count(), 1);
    source_runtime
        .resume_native_continuation(owner.as_u64(), 17, 19)
        .expect("resume restored continuation");
}

#[test]
fn actor_runtime_transfer_is_send_and_requires_a_published_safepoint() {
    fn assert_send<T: Send>() {}
    assert_send::<VmActorRuntimeTransfer>();

    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("running"));
    assert!(runtime
        .detach_actor_runtime(owner)
        .expect_err("runnable actor cannot migrate")
        .contains("not parked"));
}

#[test]
fn actor_runtime_transfer_moves_aliases_resources_and_process_handles_together() {
    let mut source_runtime = VmActorRuntime::default();
    let owner = source_runtime.spawn_root(source("owned"));
    let alias = source_runtime
        .aliases
        .create_with_options(
            &source_runtime.processes,
            owner,
            VmProcessAliasOptions::default().priority().reply(),
        )
        .expect("create owner alias");
    let resource = match source_runtime
        .memory
        .register_resource(
            &mut source_runtime.processes,
            &mut source_runtime.resources,
            owner,
            VmResourceDescriptor::new("socket", "session"),
            VmResourceTransferPolicy::OwnerOnly,
            24,
        )
        .expect("register accounted owner resource")
        .event
        .expect("resource registration")
    {
        VmResourceEvent::Registered { id, .. } => id,
        event => panic!("expected registration, found {event:?}"),
    };
    let shared = source_runtime
        .memory
        .register_shared_allocation(
            &mut source_runtime.processes,
            owner,
            VmSharedAllocationKind::ProtocolBuffer,
            16,
        )
        .expect("register actor-local shared allocation")
        .allocation_id
        .expect("shared allocation identity");
    source_runtime
        .park_native_continuation(owner.as_u64(), 31, 37)
        .expect("park owner");

    let transfer = source_runtime
        .detach_actor_runtime(owner)
        .expect("detach complete actor runtime");
    assert_eq!(source_runtime.aliases.len(), 0);
    assert!(source_runtime.resource_snapshots().is_empty());
    assert!(source_runtime.memory.process_metrics(owner).is_none());
    assert!(source_runtime
        .memory
        .shared_allocation_kind(shared)
        .is_none());
    let mut destination = VmActorRuntime::default();
    destination
        .import_actor_runtime(transfer)
        .expect("import complete actor runtime");

    let alias_route = destination.aliases.route(alias).expect("imported alias");
    assert_eq!(alias_route.owner, owner);
    assert!(alias_route.priority);
    assert!(alias_route.reply);
    let resource_record = destination
        .resources
        .get_for_owner(owner, resource)
        .expect("imported resource");
    assert_eq!(resource_record.descriptor.kind, "socket");
    assert_eq!(
        destination
            .memory
            .resource_ownership(resource)
            .expect("imported resource accounting")
            .logical_bytes,
        24
    );
    assert_eq!(
        destination.memory.shared_allocation_kind(shared),
        Some(VmSharedAllocationKind::ProtocolBuffer)
    );
    assert_eq!(
        destination
            .memory_metrics(owner)
            .expect("imported memory metrics")
            .current_bytes,
        40
    );
    assert_eq!(
        destination
            .process_info_snapshot(owner)
            .expect("imported process")
            .resource_handles,
        vec![format!("resource:{}", resource.as_u64())]
    );
    destination
        .resume_native_continuation(owner.as_u64(), 31, 37)
        .expect("resume actor with imported owner tables");
}

#[test]
fn actor_runtime_transfer_moves_delayed_message_with_exact_timer_deadline() {
    let mut source_runtime = VmActorRuntime::default();
    let owner = source_runtime.spawn_root(source("timer"));
    let timer = source_runtime
        .send_after(owner, owner, ReplValue::Int(89), 0, 10)
        .expect("schedule delayed message");
    source_runtime
        .park_native_continuation(owner.as_u64(), 41, 43)
        .expect("park timer owner");

    let transfer = source_runtime
        .detach_actor_runtime(owner)
        .expect("detach actor with timer");
    assert!(source_runtime.timer_snapshots().is_empty());
    assert_eq!(source_runtime.delayed_send_count(), 0);
    let mut destination = VmActorRuntime::default();
    destination
        .import_actor_runtime(transfer)
        .expect("import actor with timer");
    assert_eq!(destination.delayed_send_count(), 1);
    assert_eq!(destination.timer_snapshots()[0].id, timer);
    assert_eq!(destination.timer_snapshots()[0].deadline_tick, 10);

    let advance = destination.advance_actor_timers(10);
    assert_eq!(advance.deliveries.len(), 1);
    assert_eq!(destination.delayed_send_count(), 0);
    assert_eq!(
        destination
            .process_info_snapshot(owner)
            .expect("timer owner")
            .mailbox_messages,
        1
    );
    destination
        .resume_native_continuation(owner.as_u64(), 41, 43)
        .expect("resume timer owner");
}
