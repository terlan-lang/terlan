use super::{
    VmResourceDescriptor, VmResourceEvent, VmResourceId, VmResourceTable, VmResourceTransferPolicy,
};
use crate::runtime::vm::process::{VmProcessId, VmProcessSource, VmProcessTable};

fn register(
    processes: &mut VmProcessTable,
    resources: &mut VmResourceTable,
    owner: VmProcessId,
    descriptor: VmResourceDescriptor,
    transfer_policy: VmResourceTransferPolicy,
) -> VmResourceId {
    let event = resources
        .register(processes, owner, descriptor, transfer_policy)
        .expect("resource registration should succeed");
    let VmResourceEvent::Registered { id, .. } = event else {
        panic!("resource registration must return its id");
    };
    id
}

#[test]
fn resource_table_owner_snapshots_are_ordered_isolated_and_live_only() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("app.Main", "owner", 0));
    let other = processes.spawn_root(VmProcessSource::new("app.Main", "other", 0));
    let missing = VmProcessId::from_raw_for_test(99);
    let mut resources = VmResourceTable::default();
    let first = register(
        &mut processes,
        &mut resources,
        owner,
        VmResourceDescriptor::new("socket", "first"),
        VmResourceTransferPolicy::OwnerOnly,
    );
    let other_id = register(
        &mut processes,
        &mut resources,
        other,
        VmResourceDescriptor::new("file.handle", "other"),
        VmResourceTransferPolicy::OwnerOnly,
    );
    let second = register(
        &mut processes,
        &mut resources,
        owner,
        VmResourceDescriptor::new("socket", "second"),
        VmResourceTransferPolicy::Transferable,
    );

    assert_eq!(
        resources
            .snapshots_for_owner(owner)
            .into_iter()
            .map(|snapshot| snapshot.id)
            .collect::<Vec<_>>(),
        vec![first, second]
    );
    assert_eq!(
        resources
            .snapshots_for_owner(other)
            .into_iter()
            .map(|snapshot| snapshot.id)
            .collect::<Vec<_>>(),
        vec![other_id]
    );
    assert!(resources.snapshots_for_owner(missing).is_empty());

    resources
        .release(&mut processes, owner, first)
        .expect("first resource should release");
    assert_eq!(
        resources
            .snapshots_for_owner(owner)
            .into_iter()
            .map(|snapshot| snapshot.id)
            .collect::<Vec<_>>(),
        vec![second]
    );
    resources.cleanup_owner_handles(&mut processes, owner);
    assert!(resources.snapshots_for_owner(owner).is_empty());
    assert_eq!(resources.snapshots_for_owner(other)[0].id, other_id);
}
