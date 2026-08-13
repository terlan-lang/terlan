use super::{
    VmResourceDescriptor, VmResourceEvent, VmResourceId, VmResourceTable, VmResourceTransferPolicy,
};
use crate::compiler::accelerator::{
    AcceleratorAddressSpace, AcceleratorResourceClass, AcceleratorResourceHandle,
    AcceleratorResourceId, AcceleratorResourcePrincipal, AcceleratorResourceRole,
};
use crate::runtime::vm::process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessTable};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

fn register_resource(
    processes: &mut VmProcessTable,
    resources: &mut VmResourceTable,
    owner: VmProcessId,
    descriptor: VmResourceDescriptor,
    transfer_policy: VmResourceTransferPolicy,
) -> VmResourceId {
    let event = resources
        .register(processes, owner, descriptor, transfer_policy)
        .expect("resource registration should succeed");
    let id = resources
        .snapshots()
        .last()
        .expect("registered resource snapshot should exist")
        .id;
    assert_eq!(event, VmResourceEvent::Registered { id, owner });
    id
}

#[test]
fn resource_table_registers_resource_and_exposes_inspection_snapshot() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("main"));
    let mut resources = VmResourceTable::default();

    let id = register_resource(
        &mut processes,
        &mut resources,
        owner,
        VmResourceDescriptor::new("postgres.connection", "primary"),
        VmResourceTransferPolicy::OwnerOnly,
    );
    assert_eq!(
        processes
            .get(owner)
            .expect("owner should exist")
            .resource_handles,
        vec![format!("resource:{}", id.as_u64())]
    );

    let snapshots = resources.snapshots();
    assert_eq!(snapshots.len(), 1);
    assert_eq!(snapshots[0].id, id);
    assert_eq!(snapshots[0].owner, owner);
    assert_eq!(snapshots[0].kind, "postgres.connection");
    assert_eq!(snapshots[0].label, "primary");
    assert_eq!(
        snapshots[0].transfer_policy,
        VmResourceTransferPolicy::OwnerOnly
    );
}

#[test]
fn resource_table_owns_canonical_accelerator_handles_until_actor_exit() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("accelerator_owner"));
    let mut resources = VmResourceTable::default();
    let handle = AcceleratorResourceHandle {
        id: AcceleratorResourceId {
            slot: 4,
            generation: 2,
        },
        class: AcceleratorResourceClass::Stream,
        address_space: AcceleratorAddressSpace::External {
            provider: "cuda".to_string(),
            space: "stream".to_string(),
        },
        role: AcceleratorResourceRole::Owned {
            principal: AcceleratorResourcePrincipal::new("actor.owner").expect("principal"),
        },
    };

    resources
        .register_accelerator(&mut processes, owner, handle.clone())
        .expect("accelerator registration");
    let snapshot = resources
        .snapshots()
        .into_iter()
        .next()
        .expect("resource snapshot");
    assert_eq!(snapshot.kind, "accelerator");
    assert_eq!(snapshot.label, "stream:4:2");
    assert_eq!(snapshot.accelerator_handle, Some(handle));
    assert_eq!(
        resources.cleanup_owner_handles(&mut processes, owner).len(),
        1
    );
    assert!(resources.snapshots().is_empty());
}

#[test]
fn resource_table_rejects_borrowed_accelerator_handles_at_actor_boundary() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("accelerator_owner"));
    let mut resources = VmResourceTable::default();
    let borrowed = AcceleratorResourceHandle {
        id: AcceleratorResourceId {
            slot: 1,
            generation: 1,
        },
        class: AcceleratorResourceClass::Allocation,
        address_space: AcceleratorAddressSpace::Host,
        role: AcceleratorResourceRole::Borrowed {
            principal: AcceleratorResourcePrincipal::new("actor.borrower").expect("principal"),
            scope: 9,
        },
    };
    assert!(resources
        .register_accelerator(&mut processes, owner, borrowed)
        .expect_err("borrow escape")
        .contains("borrowed handle escaped"));
    assert!(resources.snapshots().is_empty());
}

#[test]
fn resource_table_transfers_transferable_resource_between_live_processes() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let recipient = processes.spawn_root(source("recipient"));
    let mut resources = VmResourceTable::default();
    let id = register_resource(
        &mut processes,
        &mut resources,
        owner,
        VmResourceDescriptor::new("file.handle", "/tmp/report"),
        VmResourceTransferPolicy::Transferable,
    );

    let event = resources
        .transfer(&mut processes, id, owner, recipient)
        .expect("transferable resource should move");

    assert_eq!(
        event,
        VmResourceEvent::Transferred {
            id,
            from: owner,
            to: recipient
        }
    );
    assert!(processes
        .get(owner)
        .expect("owner should exist")
        .resource_handles
        .is_empty());
    assert_eq!(
        processes
            .get(recipient)
            .expect("recipient should exist")
            .resource_handles,
        vec![format!("resource:{}", id.as_u64())]
    );
    assert_eq!(
        resources
            .get_for_owner(recipient, id)
            .expect("recipient should own resource")
            .owner,
        recipient
    );
}

#[test]
fn resource_table_releases_transferred_resource_from_new_owner() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let recipient = processes.spawn_root(source("recipient"));
    let mut resources = VmResourceTable::default();
    let id = register_resource(
        &mut processes,
        &mut resources,
        owner,
        VmResourceDescriptor::new("file.handle", "/tmp/report"),
        VmResourceTransferPolicy::Transferable,
    );

    resources
        .transfer(&mut processes, id, owner, recipient)
        .expect("transferable resource should move");
    assert_eq!(
        resources
            .release(&mut processes, owner, id)
            .expect_err("previous owner should not release transferred resource"),
        format!(
            "resource {} is owned by process {}, not {}",
            id.as_u64(),
            recipient.as_u64(),
            owner.as_u64()
        )
    );

    assert_eq!(
        resources
            .release(&mut processes, recipient, id)
            .expect("recipient should release transferred resource"),
        VmResourceEvent::Released {
            id,
            owner: recipient
        }
    );
    assert!(processes
        .get(recipient)
        .expect("recipient should exist")
        .resource_handles
        .is_empty());
    assert!(resources.snapshots().is_empty());
}

#[test]
fn resource_table_rejects_owner_only_transfer() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let recipient = processes.spawn_root(source("recipient"));
    let mut resources = VmResourceTable::default();
    let id = register_resource(
        &mut processes,
        &mut resources,
        owner,
        VmResourceDescriptor::new("socket", "control"),
        VmResourceTransferPolicy::OwnerOnly,
    );

    assert_eq!(
        resources
            .transfer(&mut processes, id, owner, recipient)
            .expect_err("owner-only resource should not transfer"),
        format!("resource {} cannot be transferred", id.as_u64())
    );
    assert_eq!(
        resources
            .get_for_owner(owner, id)
            .expect("owner should still own resource")
            .owner,
        owner
    );
}

#[test]
fn resource_table_reports_stale_handle_after_release() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let mut resources = VmResourceTable::default();
    let id = register_resource(
        &mut processes,
        &mut resources,
        owner,
        VmResourceDescriptor::new("json.decoder", "scratch"),
        VmResourceTransferPolicy::OwnerOnly,
    );

    assert_eq!(
        resources
            .release(&mut processes, owner, id)
            .expect("owner should release resource"),
        VmResourceEvent::Released { id, owner }
    );
    assert_eq!(
        resources
            .get_for_owner(owner, id)
            .expect_err("released resource should be stale"),
        format!("stale native resource handle {}", id.as_u64())
    );
    assert!(processes
        .get(owner)
        .expect("owner should exist")
        .resource_handles
        .is_empty());
}

#[test]
fn resource_table_cleans_up_owner_resources_on_process_exit() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let other = processes.spawn_root(source("other"));
    let mut resources = VmResourceTable::default();
    let owned_id = register_resource(
        &mut processes,
        &mut resources,
        owner,
        VmResourceDescriptor::new("postgres.connection", "primary"),
        VmResourceTransferPolicy::OwnerOnly,
    );
    let other_id = register_resource(
        &mut processes,
        &mut resources,
        other,
        VmResourceDescriptor::new("postgres.connection", "analytics"),
        VmResourceTransferPolicy::OwnerOnly,
    );

    let cleaned_handles = processes
        .exit_process(owner, VmExitReason::Normal)
        .expect("owner exit should succeed");
    let cleanup_events = resources.cleanup_owner(owner);

    assert_eq!(
        cleaned_handles,
        vec![format!("resource:{}", owned_id.as_u64())]
    );
    assert_eq!(
        cleanup_events,
        vec![VmResourceEvent::CleanedUpOnExit {
            id: owned_id,
            owner
        }]
    );
    assert_eq!(
        resources
            .get_for_owner(owner, owned_id)
            .expect_err("cleaned resource should be stale"),
        format!("stale native resource handle {}", owned_id.as_u64())
    );
    assert_eq!(resources.snapshots().len(), 1);
    assert_eq!(resources.snapshots()[0].id, other_id);
}

#[test]
fn resource_table_cleanup_owner_handles_removes_live_process_handle_rows() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let other = processes.spawn_root(source("other"));
    let mut resources = VmResourceTable::default();
    let first = register_resource(
        &mut processes,
        &mut resources,
        owner,
        VmResourceDescriptor::new("file.handle", "/tmp/a"),
        VmResourceTransferPolicy::OwnerOnly,
    );
    let second = register_resource(
        &mut processes,
        &mut resources,
        owner,
        VmResourceDescriptor::new("file.handle", "/tmp/b"),
        VmResourceTransferPolicy::OwnerOnly,
    );
    let other_id = register_resource(
        &mut processes,
        &mut resources,
        other,
        VmResourceDescriptor::new("file.handle", "/tmp/c"),
        VmResourceTransferPolicy::OwnerOnly,
    );

    let cleanup_events = resources.cleanup_owner_handles(&mut processes, owner);

    assert_eq!(
        cleanup_events,
        vec![
            VmResourceEvent::CleanedUpOnExit { id: first, owner },
            VmResourceEvent::CleanedUpOnExit { id: second, owner },
        ]
    );
    assert!(processes
        .get(owner)
        .expect("owner should still exist")
        .resource_handles
        .is_empty());
    assert_eq!(
        processes
            .get(other)
            .expect("other should still exist")
            .resource_handles,
        vec![format!("resource:{}", other_id.as_u64())]
    );
    assert_eq!(resources.snapshots().len(), 1);
    assert_eq!(resources.snapshots()[0].id, other_id);

    assert!(resources
        .cleanup_owner_handles(&mut processes, VmProcessId::from_raw_for_test(999))
        .is_empty());

    let detached_owner = processes.spawn_root(source("detached"));
    resources
        .register(
            &mut processes,
            detached_owner,
            VmResourceDescriptor::new("file.handle", "/tmp/detached"),
            VmResourceTransferPolicy::OwnerOnly,
        )
        .expect("detached resource should register");
    let mut detached_processes = VmProcessTable::default();
    assert_eq!(
        resources
            .cleanup_owner_handles(&mut detached_processes, detached_owner)
            .len(),
        1
    );
}

#[test]
fn resource_table_rejects_wrong_owner_access_transfer_and_release() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let other = processes.spawn_root(source("other"));
    let mut resources = VmResourceTable::default();
    let id = register_resource(
        &mut processes,
        &mut resources,
        owner,
        VmResourceDescriptor::new("file.handle", "/tmp/report"),
        VmResourceTransferPolicy::Transferable,
    );

    let expected = format!(
        "resource {} is owned by process {}, not {}",
        id.as_u64(),
        owner.as_u64(),
        other.as_u64()
    );

    assert_eq!(
        resources
            .get_for_owner(other, id)
            .expect_err("wrong owner read should fail"),
        expected
    );
    assert_eq!(
        resources
            .transfer(&mut processes, id, other, owner)
            .expect_err("wrong transfer source should fail"),
        expected
    );
    assert_eq!(
        resources
            .release(&mut processes, other, id)
            .expect_err("wrong release owner should fail"),
        expected
    );
}

#[test]
fn resource_table_reports_stale_handle_for_transfer() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let recipient = processes.spawn_root(source("recipient"));
    let mut resources = VmResourceTable::default();
    let id = register_resource(
        &mut processes,
        &mut resources,
        owner,
        VmResourceDescriptor::new("socket", "control"),
        VmResourceTransferPolicy::Transferable,
    );
    resources
        .release(&mut processes, owner, id)
        .expect("resource should release");

    assert_eq!(
        resources
            .transfer(&mut processes, id, owner, recipient)
            .expect_err("stale transfer should fail"),
        format!("stale native resource handle {}", id.as_u64())
    );
}

#[test]
fn resource_table_reports_stale_handle_for_release() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let mut resources = VmResourceTable::default();
    let id = register_resource(
        &mut processes,
        &mut resources,
        owner,
        VmResourceDescriptor::new("socket", "control"),
        VmResourceTransferPolicy::OwnerOnly,
    );
    resources
        .release(&mut processes, owner, id)
        .expect("resource should release");

    assert_eq!(
        resources
            .release(&mut processes, owner, id)
            .expect_err("stale release should fail"),
        format!("stale native resource handle {}", id.as_u64())
    );
}

#[test]
fn resource_table_rejects_missing_process_roles() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let target = processes.spawn_root(source("target"));
    let missing = VmProcessId::from_raw_for_test(99);
    let mut resources = VmResourceTable::default();
    let id = register_resource(
        &mut processes,
        &mut resources,
        owner,
        VmResourceDescriptor::new("socket", "control"),
        VmResourceTransferPolicy::Transferable,
    );

    assert_eq!(
        resources
            .register(
                &mut processes,
                missing,
                VmResourceDescriptor::new("file", "missing"),
                VmResourceTransferPolicy::OwnerOnly,
            )
            .expect_err("missing owner registration should fail"),
        "missing owner process 99"
    );
    assert_eq!(
        resources
            .transfer(&mut processes, id, missing, target)
            .expect_err("missing source transfer should fail"),
        "missing source process 99"
    );
    assert_eq!(
        resources
            .transfer(&mut processes, id, owner, missing)
            .expect_err("missing target transfer should fail"),
        "missing target process 99"
    );
    assert_eq!(
        resources
            .release(&mut processes, missing, id)
            .expect_err("missing owner release should fail"),
        "missing owner process 99"
    );
}

#[test]
fn resource_table_rejects_exited_process_roles() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let exited = processes.spawn_root(source("exited"));
    let live_target = processes.spawn_root(source("target"));
    let mut resources = VmResourceTable::default();
    let id = register_resource(
        &mut processes,
        &mut resources,
        owner,
        VmResourceDescriptor::new("socket", "control"),
        VmResourceTransferPolicy::Transferable,
    );
    processes
        .exit_process(exited, VmExitReason::Killed)
        .expect("process should exit");

    assert_eq!(
        resources
            .register(
                &mut processes,
                exited,
                VmResourceDescriptor::new("file", "exited"),
                VmResourceTransferPolicy::OwnerOnly,
            )
            .expect_err("exited owner registration should fail"),
        "owner process 2 has exited"
    );
    assert_eq!(
        resources
            .transfer(&mut processes, id, exited, live_target)
            .expect_err("exited source transfer should fail"),
        "source process 2 has exited"
    );
    assert_eq!(
        resources
            .transfer(&mut processes, id, owner, exited)
            .expect_err("exited target transfer should fail"),
        "target process 2 has exited"
    );
    assert_eq!(
        resources
            .release(&mut processes, exited, id)
            .expect_err("exited owner release should fail"),
        "owner process 2 has exited"
    );
}
