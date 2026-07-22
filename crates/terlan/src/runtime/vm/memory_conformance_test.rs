use super::{VmMemoryAccountant, VmMemoryLimits, VmMemoryPressureOutcome, VmSharedAllocationKind};
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessSource, VmProcessState, VmProcessTable},
    resource::{VmResourceDescriptor, VmResourceEvent, VmResourceTable, VmResourceTransferPolicy},
    ReplValue,
};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.MemoryConformance", name, 0)
}

#[test]
fn memory_pressure_rollback_preserves_every_owner_registry_and_allocation_identity() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let survivor = processes.spawn_root(source("survivor"));
    let mut resources = VmResourceTable::default();
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(80, 100).expect("limits"));

    memory
        .account_heap(&mut processes, owner, 10)
        .expect("ordinary owner heap");
    memory
        .send_message(&mut processes, owner, owner, ReplValue::Unit, 15)
        .expect("owner mailbox root");
    memory
        .register_resource(
            &mut processes,
            &mut resources,
            owner,
            VmResourceDescriptor::new("file.handle", "/tmp/owner"),
            VmResourceTransferPolicy::OwnerOnly,
            20,
        )
        .expect("owner resource");
    let first_shared = memory
        .register_shared_allocation(
            &mut processes,
            owner,
            VmSharedAllocationKind::ProtocolBuffer,
            25,
        )
        .expect("owner shared allocation")
        .allocation_id
        .expect("first shared id");
    memory
        .register_resource(
            &mut processes,
            &mut resources,
            survivor,
            VmResourceDescriptor::new("file.handle", "/tmp/survivor"),
            VmResourceTransferPolicy::OwnerOnly,
            12,
        )
        .expect("survivor resource");

    let owner_before = processes.snapshot(owner).expect("owner snapshot");
    let survivor_before = processes.snapshot(survivor).expect("survivor snapshot");
    let resources_before = resources.snapshots();
    let resource_ownership_before = memory.resource_ownership.clone();
    let shared_allocations_before = memory.shared_allocations.clone();
    let next_shared_id_before = memory.next_shared_allocation_id;

    let rejected = memory
        .register_shared_allocation(
            &mut processes,
            owner,
            VmSharedAllocationKind::ResponseBuffer,
            31,
        )
        .expect("hard pressure is a typed rollback");

    assert_eq!(rejected.allocation_id, None);
    assert_eq!(
        rejected.pressure.outcome,
        VmMemoryPressureOutcome::HardLimitRejected
    );
    assert_eq!(
        processes.snapshot(owner).expect("owner snapshot"),
        owner_before
    );
    assert_eq!(
        processes.snapshot(survivor).expect("survivor snapshot"),
        survivor_before
    );
    assert_eq!(resources.snapshots(), resources_before);
    assert_eq!(memory.resource_ownership, resource_ownership_before);
    assert_eq!(memory.shared_allocations, shared_allocations_before);
    assert_eq!(memory.next_shared_allocation_id, next_shared_id_before);

    let second_shared = memory
        .register_shared_allocation(
            &mut processes,
            owner,
            VmSharedAllocationKind::ResponseBuffer,
            30,
        )
        .expect("hard-limit boundary is admissible")
        .allocation_id
        .expect("second shared id");
    assert_eq!(first_shared.as_u64(), 1);
    assert_eq!(second_shared.as_u64(), 2);
}

#[test]
fn memory_collection_then_exit_preserves_surviving_owner_state() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let survivor = processes.spawn_root(source("survivor"));
    let mut resources = VmResourceTable::default();
    let mut memory = VmMemoryAccountant::new(VmMemoryLimits::new(128, 256).expect("limits"));

    memory
        .account_heap(&mut processes, owner, 40)
        .expect("owner traced and reclaimable heap");
    memory
        .send_message(&mut processes, owner, owner, ReplValue::Unit, 10)
        .expect("owner mailbox root");
    let owner_resource = memory
        .register_resource(
            &mut processes,
            &mut resources,
            owner,
            VmResourceDescriptor::new("file.handle", "/tmp/owner"),
            VmResourceTransferPolicy::OwnerOnly,
            20,
        )
        .expect("owner resource")
        .event
        .expect("owner resource event");
    let shared = memory
        .register_shared_allocation(
            &mut processes,
            owner,
            VmSharedAllocationKind::ResponseBuffer,
            25,
        )
        .expect("shared response buffer")
        .allocation_id
        .expect("shared allocation id");
    memory
        .retain_shared_allocation(&mut processes, shared, owner, survivor)
        .expect("survivor shared reference");
    let survivor_resource = memory
        .register_resource(
            &mut processes,
            &mut resources,
            survivor,
            VmResourceDescriptor::new("file.handle", "/tmp/survivor"),
            VmResourceTransferPolicy::OwnerOnly,
            15,
        )
        .expect("survivor resource")
        .event
        .expect("survivor resource event");
    let survivor_before = processes.snapshot(survivor).expect("survivor snapshot");

    let collected = memory
        .collect_process_heap(&mut processes, owner, 5)
        .expect("collect owner heap");
    assert_eq!(collected.previous_bytes, 95);
    assert_eq!(collected.protected_bytes, 55);
    assert_eq!(collected.retained_bytes, 60);
    assert_eq!(collected.reclaimed_bytes, 35);

    let exited = memory
        .exit_process_with_memory_cleanup(
            &mut processes,
            &mut resources,
            owner,
            VmExitReason::Killed,
        )
        .expect("exit owner after collection");
    let VmResourceEvent::Registered {
        id: owner_resource_id,
        ..
    } = owner_resource
    else {
        panic!("owner resource must be registered");
    };
    let VmResourceEvent::Registered {
        id: survivor_resource_id,
        ..
    } = survivor_resource
    else {
        panic!("survivor resource must be registered");
    };

    assert_eq!(
        exited.resource_events,
        vec![VmResourceEvent::CleanedUpOnExit {
            id: owner_resource_id,
            owner,
        }]
    );
    assert_eq!(exited.released_shared_allocations, vec![shared]);
    assert_eq!(
        processes.snapshot(owner).expect("owner snapshot").state,
        VmProcessState::Exited(VmExitReason::Killed)
    );
    assert_eq!(
        processes
            .snapshot(owner)
            .expect("owner snapshot")
            .heap_bytes,
        0
    );
    assert_eq!(
        processes.snapshot(survivor).expect("survivor snapshot"),
        survivor_before
    );
    assert_eq!(resources.snapshots_for_owner(owner), Vec::new());
    assert_eq!(
        resources.snapshots_for_owner(survivor)[0].id,
        survivor_resource_id
    );
    assert_eq!(
        memory
            .shared_allocation(shared)
            .expect("shared allocation survives")
            .owners,
        std::collections::BTreeSet::from([survivor.as_u64()])
    );
}
