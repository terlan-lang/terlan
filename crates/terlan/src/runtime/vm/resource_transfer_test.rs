use super::*;
use crate::runtime::vm::process::{VmProcessSource, VmProcessTable};

fn owner_process() -> (VmProcessTable, VmProcessId) {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("app.Resource", "run", 0));
    (processes, owner)
}

#[test]
fn resource_transfer_preserves_exact_records_and_allocation_watermark() {
    let (mut processes, owner) = owner_process();
    let mut source = VmResourceTable::default();
    source
        .register(
            &mut processes,
            owner,
            VmResourceDescriptor::new("socket", "primary"),
            VmResourceTransferPolicy::OwnerOnly,
        )
        .expect("register owner-only resource");
    source
        .register(
            &mut processes,
            owner,
            VmResourceDescriptor::new("file", "log"),
            VmResourceTransferPolicy::Transferable,
        )
        .expect("register transferable resource");
    let transfer = source.detach_owner_resources(owner);
    assert_eq!(transfer.owner(), owner);
    assert_eq!(transfer.len(), 2);
    assert!(source.snapshots().is_empty());

    let mut destination = VmResourceTable::default();
    destination
        .import_resource_transfer(transfer)
        .expect("import resources");
    let snapshots = destination.snapshots_for_owner(owner);
    assert_eq!(snapshots.len(), 2);
    assert_eq!(snapshots[0].kind, "socket");
    assert_eq!(
        snapshots[0].transfer_policy,
        VmResourceTransferPolicy::OwnerOnly
    );
    assert_eq!(snapshots[1].kind, "file");
}

#[test]
fn resource_collision_returns_complete_state_for_rollback() {
    let (mut processes, owner) = owner_process();
    let mut source = VmResourceTable::default();
    source
        .register(
            &mut processes,
            owner,
            VmResourceDescriptor::new("source", "one"),
            VmResourceTransferPolicy::OwnerOnly,
        )
        .expect("register source");
    let transfer = source.detach_owner_resources(owner);
    let mut destination = VmResourceTable::default();
    destination
        .register(
            &mut processes,
            owner,
            VmResourceDescriptor::new("destination", "one"),
            VmResourceTransferPolicy::OwnerOnly,
        )
        .expect("register collision");
    let failure = destination
        .import_resource_transfer(transfer)
        .expect_err("resource identity collision");
    assert!(failure.reason().contains("already contains"));
    source
        .import_resource_transfer(failure.into_transfer())
        .expect("restore source resources");
    assert_eq!(source.snapshots_for_owner(owner).len(), 1);
}

#[test]
fn resource_transfer_is_send_even_when_empty() {
    fn assert_send<T: Send>() {}
    assert_send::<VmResourceTransfer>();
    let (_, owner) = owner_process();
    assert_eq!(
        VmResourceTable::default()
            .detach_owner_resources(owner)
            .len(),
        0
    );
}
