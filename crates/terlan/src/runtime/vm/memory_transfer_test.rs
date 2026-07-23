use super::transfer::VmMemoryTransfer;
use super::{VmMemoryAccountant, VmMemoryLimits, VmSharedAllocationKind};
use crate::runtime::vm::process::{VmProcessSource, VmProcessTable};
use crate::runtime::vm::resource::{
    VmResourceDescriptor, VmResourceTable, VmResourceTransferPolicy,
};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.MemoryTransfer", name, 0)
}

#[test]
fn memory_transfer_preserves_metrics_decisions_resources_and_shared_identity() {
    let limits = VmMemoryLimits::new(128, 256).expect("limits");
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let mut resources = VmResourceTable::default();
    let mut memory = VmMemoryAccountant::new(limits);
    memory
        .account_heap(&mut processes, owner, 10)
        .expect("ordinary heap");
    let resource = memory
        .register_resource(
            &mut processes,
            &mut resources,
            owner,
            VmResourceDescriptor::new("socket", "client"),
            VmResourceTransferPolicy::OwnerOnly,
            20,
        )
        .expect("resource")
        .event
        .expect("resource event");
    let shared = memory
        .register_shared_allocation(
            &mut processes,
            owner,
            VmSharedAllocationKind::ProtocolBuffer,
            25,
        )
        .expect("shared allocation")
        .allocation_id
        .expect("shared identity");
    let resource_id = match resource {
        crate::runtime::vm::resource::VmResourceEvent::Registered { id, .. } => id,
        event => panic!("expected registration, found {event:?}"),
    };
    let heap_bytes = processes.get(owner).expect("owner").heap_bytes;
    memory
        .validate_memory_detach(owner, heap_bytes, [resource_id.as_u64()])
        .expect("detachable graph");

    let transfer = memory.detach_owner_memory(owner);
    assert_eq!(transfer.owner(), owner);
    assert_eq!(transfer.current_bytes(), 55);
    assert!(memory.process_metrics(owner).is_none());
    assert!(memory.resource_ownership(resource_id).is_none());
    assert!(memory.shared_allocation(shared).is_none());

    let mut destination = VmMemoryAccountant::new(limits);
    destination
        .import_memory_transfer(transfer, heap_bytes)
        .expect("import memory");
    assert_eq!(
        destination
            .process_metrics(owner)
            .expect("imported metrics")
            .current_bytes,
        55
    );
    assert_eq!(
        destination
            .resource_ownership(resource_id)
            .expect("imported resource charge")
            .logical_bytes,
        20
    );
    assert_eq!(
        destination.shared_allocation_kind(shared),
        Some(VmSharedAllocationKind::ProtocolBuffer)
    );
    let next = destination
        .register_shared_allocation(&mut processes, owner, VmSharedAllocationKind::Binary, 1)
        .expect("next allocation")
        .allocation_id
        .expect("next identity");
    assert_eq!(next.as_u64(), shared.as_u64() + 1);
}

#[test]
fn memory_transfer_rejects_cross_actor_shared_roots_before_detach() {
    let limits = VmMemoryLimits::new(128, 256).expect("limits");
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let peer = processes.spawn_root(source("peer"));
    let mut memory = VmMemoryAccountant::new(limits);
    let shared = memory
        .register_shared_allocation(
            &mut processes,
            owner,
            VmSharedAllocationKind::ResponseBuffer,
            16,
        )
        .expect("shared allocation")
        .allocation_id
        .expect("shared identity");
    memory
        .retain_shared_allocation(&mut processes, shared, owner, peer)
        .expect("peer retain");

    assert!(memory
        .validate_memory_detach(owner, 16, [])
        .expect_err("cross-actor root must reject migration")
        .contains("cross-actor shared allocation"));
    assert!(memory.process_metrics(owner).is_some());
    assert!(memory.shared_allocation(shared).is_some());
}

#[test]
fn memory_import_collision_returns_complete_transfer_for_rollback() {
    let limits = VmMemoryLimits::new(128, 256).expect("limits");
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let mut source_memory = VmMemoryAccountant::new(limits);
    source_memory
        .account_heap(&mut processes, owner, 8)
        .expect("source accounting");
    let transfer = source_memory.detach_owner_memory(owner);

    let mut destination = VmMemoryAccountant::new(limits);
    destination
        .account_heap(&mut processes, owner, 1)
        .expect("collision metrics");
    let failure = destination
        .import_memory_transfer(transfer, 8)
        .expect_err("metrics collision");
    assert!(failure.reason().contains("already contains"));
    source_memory
        .import_memory_transfer(failure.into_transfer(), 8)
        .expect("source rollback");
    assert_eq!(
        source_memory
            .process_metrics(owner)
            .expect("restored metrics")
            .current_bytes,
        8
    );
}

#[test]
fn memory_transfer_is_send_even_when_empty() {
    fn assert_send<T: Send>() {}
    assert_send::<VmMemoryTransfer>();

    let limits = VmMemoryLimits::new(128, 256).expect("limits");
    let mut memory = VmMemoryAccountant::new(limits);
    let owner = crate::runtime::vm::process::VmProcessId::from_raw_for_test(9);
    let transfer = memory.detach_owner_memory(owner);
    assert_eq!(transfer.current_bytes(), 0);
}
