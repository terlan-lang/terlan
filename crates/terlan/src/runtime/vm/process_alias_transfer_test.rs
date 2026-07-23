use super::transfer::VmProcessAliasTransfer;
use super::*;
use crate::runtime::vm::process::VmProcessSource;

fn owner_process() -> (VmProcessTable, VmProcessId) {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("app.Alias", "run", 0));
    (processes, owner)
}

#[test]
fn alias_transfer_preserves_exact_routes_and_capabilities() {
    let (processes, owner) = owner_process();
    let mut source = VmProcessAliasTable::default();
    let ordinary = source.create(&processes, owner).expect("ordinary alias");
    let priority_reply = source
        .create_with_options(
            &processes,
            owner,
            VmProcessAliasOptions::default().priority().reply(),
        )
        .expect("priority reply alias");
    let transfer = source.detach_owner_aliases(owner);
    assert_eq!(transfer.owner(), owner);
    assert_eq!(transfer.len(), 2);
    assert_eq!(source.len(), 0);

    let mut destination = VmProcessAliasTable::default();
    destination
        .import_alias_transfer(transfer)
        .expect("import aliases");
    assert_eq!(destination.resolve(ordinary), Some(owner));
    assert_eq!(
        destination.route(priority_reply),
        Some(VmProcessAliasRoute {
            owner,
            priority: true,
            reply: true,
        })
    );
}

#[test]
fn alias_collision_returns_complete_state_for_rollback() {
    let (processes, owner) = owner_process();
    let mut source = VmProcessAliasTable::default();
    source.create(&processes, owner).expect("source alias");
    let transfer = source.detach_owner_aliases(owner);
    let mut destination = VmProcessAliasTable::default();
    destination
        .create(&processes, owner)
        .expect("destination collision");
    let failure = destination
        .import_alias_transfer(transfer)
        .expect_err("alias collision");
    assert!(failure.reason().contains("already contains"));
    source
        .import_alias_transfer(failure.into_transfer())
        .expect("restore source aliases");
    assert_eq!(source.aliases_for_process(owner).len(), 1);
}

#[test]
fn alias_transfer_is_send_even_when_empty() {
    fn assert_send<T: Send>() {}
    assert_send::<VmProcessAliasTransfer>();
    let (_, owner) = owner_process();
    assert_eq!(
        VmProcessAliasTable::default()
            .detach_owner_aliases(owner)
            .len(),
        0
    );
}
