use std::collections::BTreeSet;

use super::{
    collection::VmMemoryCollection, VmAccountedResourceOwnership, VmMemoryAccountant,
    VmMemoryLimits, VmSharedAllocation, VmSharedAllocationKind,
};
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessTable},
    resource::{VmResourceDescriptor, VmResourceEvent, VmResourceTable, VmResourceTransferPolicy},
    ReplValue,
};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Memory", name, 0)
}

fn accountant() -> VmMemoryAccountant {
    VmMemoryAccountant::new(VmMemoryLimits::new(1_024, 2_048).expect("memory limits"))
}

#[test]
fn memory_collection_reclaims_untraced_heap_and_records_metrics() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source("collect"));
    let mut memory = accountant();
    memory
        .account_heap(&mut processes, pid, 100)
        .expect("account heap");

    assert_eq!(
        memory
            .collect_process_heap(&mut processes, pid, 24)
            .expect("collect heap"),
        VmMemoryCollection {
            pid,
            previous_bytes: 100,
            protected_bytes: 0,
            traced_value_bytes: 24,
            retained_bytes: 24,
            reclaimed_bytes: 76,
        }
    );
    assert_eq!(processes.snapshot(pid).expect("snapshot").heap_bytes, 24);
    let metrics = memory.process_metrics(pid).expect("memory metrics");
    assert_eq!(metrics.current_bytes, 24);
    assert_eq!(metrics.high_water_bytes, 100);
    assert_eq!(metrics.collection_events, 1);
    assert_eq!(metrics.released_bytes, 76);
}

#[test]
fn memory_collection_preserves_mailbox_resource_and_shared_roots() {
    let mut processes = VmProcessTable::default();
    let sender = processes.spawn_root(source("sender"));
    let owner = processes.spawn_root(source("owner"));
    let mut memory = accountant();
    let mut resources = VmResourceTable::default();
    memory
        .account_heap(&mut processes, owner, 40)
        .expect("account traced and garbage heap");
    memory
        .send_message(
            &mut processes,
            sender,
            owner,
            ReplValue::Atom("queued".to_string()),
            16,
        )
        .expect("account mailbox root");
    let registered = memory
        .register_resource(
            &mut processes,
            &mut resources,
            owner,
            VmResourceDescriptor::new("file.handle", "/tmp/collection-root"),
            VmResourceTransferPolicy::OwnerOnly,
            20,
        )
        .expect("account resource root")
        .event
        .expect("registered resource");
    let VmResourceEvent::Registered { id: resource, .. } = registered else {
        panic!("resource registration must return a registered event");
    };
    let shared = memory
        .register_shared_allocation(&mut processes, owner, VmSharedAllocationKind::Binary, 24)
        .expect("account shared root")
        .allocation_id
        .expect("shared allocation");

    let collected = memory
        .collect_process_heap(&mut processes, owner, 10)
        .expect("collect rooted heap");
    assert_eq!(collected.protected_bytes, 60);
    assert_eq!(collected.retained_bytes, 70);
    assert_eq!(collected.reclaimed_bytes, 30);

    memory
        .receive_message(&mut processes, owner)
        .expect("receive rooted message");
    memory
        .release_resource(&mut processes, &mut resources, owner, resource)
        .expect("release rooted resource");
    memory
        .release_shared_allocation(&mut processes, shared, owner)
        .expect("release rooted shared allocation");
    assert_eq!(processes.snapshot(owner).expect("snapshot").heap_bytes, 10);
}

#[test]
fn memory_collection_rejects_impossible_retained_size_without_mutation() {
    let mut processes = VmProcessTable::default();
    let pid = processes.spawn_root(source("oversized"));
    let mut memory = accountant();
    memory
        .account_heap(&mut processes, pid, 8)
        .expect("account heap");

    assert_eq!(
        memory
            .collect_process_heap(&mut processes, pid, 9)
            .expect_err("retained bytes exceed accounted heap"),
        "process 1 collected heap retains 9 bytes from 8 accounted bytes"
    );
    assert_eq!(processes.snapshot(pid).expect("snapshot").heap_bytes, 8);
    let metrics = memory.process_metrics(pid).expect("memory metrics");
    assert_eq!(metrics.collection_events, 0);
    assert_eq!(metrics.released_bytes, 0);
}

#[test]
fn memory_collection_rejects_overflow_missing_and_exited_processes() {
    let mut processes = VmProcessTable::default();
    let sender = processes.spawn_root(source("sender"));
    let owner = processes.spawn_root(source("owner"));
    let exited = processes.spawn_root(source("exited"));
    let mut memory = accountant();
    memory
        .send_message(&mut processes, sender, owner, ReplValue::Unit, 1)
        .expect("account mailbox root");
    processes
        .exit_process(exited, VmExitReason::Normal)
        .expect("exit process");

    assert_eq!(
        memory
            .collect_process_heap(&mut processes, owner, usize::MAX)
            .expect_err("retained byte overflow"),
        "process 2 collected heap retained byte overflow"
    );
    assert_eq!(processes.snapshot(owner).expect("snapshot").heap_bytes, 1);
    assert_eq!(
        memory
            .collect_process_heap(&mut processes, exited, 0)
            .expect_err("exited process"),
        "exited process 3 cannot own VM heap bytes"
    );
    let missing = VmProcessId::from_raw_for_test(99);
    assert_eq!(
        memory
            .collect_process_heap(&mut processes, missing, 0)
            .expect_err("missing process"),
        "missing process 99 for VM memory accounting"
    );
}

#[test]
fn memory_collection_rejects_resource_and_shared_root_sum_overflow() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let sender = processes.spawn_root(source("sender"));
    let mut memory = accountant();
    memory.resource_ownership.insert(
        1,
        VmAccountedResourceOwnership {
            resource_id: 1,
            owner: owner.as_u64(),
            logical_bytes: usize::MAX,
        },
    );
    processes
        .send_accounted(sender, owner, ReplValue::Unit, 1)
        .expect("account mailbox root");

    assert_eq!(
        memory
            .collect_process_heap(&mut processes, owner, 0)
            .expect_err("protected roots should overflow"),
        "process 1 protected heap root byte overflow"
    );
    processes
        .get_mut(owner)
        .expect("owner")
        .receive_next()
        .expect("mailbox root");

    memory.resource_ownership.insert(
        2,
        VmAccountedResourceOwnership {
            resource_id: 2,
            owner: owner.as_u64(),
            logical_bytes: 1,
        },
    );

    assert_eq!(
        memory
            .collect_process_heap(&mut processes, owner, 0)
            .expect_err("resource roots should overflow"),
        "process 1 resource ownership byte overflow"
    );

    memory.resource_ownership.clear();
    memory.shared_allocations.insert(
        1,
        VmSharedAllocation {
            id: 1,
            kind: VmSharedAllocationKind::Binary,
            logical_bytes: usize::MAX,
            owners: BTreeSet::from([owner.as_u64()]),
        },
    );
    memory.shared_allocations.insert(
        2,
        VmSharedAllocation {
            id: 2,
            kind: VmSharedAllocationKind::Binary,
            logical_bytes: 1,
            owners: BTreeSet::from([owner.as_u64()]),
        },
    );

    assert_eq!(
        memory
            .collect_process_heap(&mut processes, owner, 0)
            .expect_err("shared roots should overflow"),
        "process 1 shared allocation ownership byte overflow"
    );
    assert_eq!(processes.snapshot(owner).expect("snapshot").heap_bytes, 0);
}
