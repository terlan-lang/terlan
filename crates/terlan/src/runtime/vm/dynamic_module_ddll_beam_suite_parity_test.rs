use super::{
    VmDynamicModuleDescriptor, VmDynamicModuleEvent, VmDynamicModuleLeaseCloseReason,
    VmDynamicModuleLoadOutcome, VmDynamicModulePendingAction, VmDynamicModuleRegistry,
    VmDynamicModuleReloadOutcome, VmDynamicModuleUnloadOutcome,
};
use crate::runtime::vm::process::{VmExitReason, VmProcessSource, VmProcessTable};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.DynamicModuleParity", name, 0)
}

fn descriptor(name: &str, artifact: &str) -> VmDynamicModuleDescriptor {
    VmDynamicModuleDescriptor::new(name, artifact)
}

#[test]
fn ddll_suite_load_references_validation_and_delayed_unload_contract() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let borrower = processes.spawn_root(source("borrower"));
    let stranger = processes.spawn_root(source("stranger"));
    let mut modules = VmDynamicModuleRegistry::default();

    assert_eq!(
        modules.load(&processes, owner, descriptor("echo", "echo-v1")),
        Ok(VmDynamicModuleLoadOutcome::Loaded)
    );
    assert_eq!(
        modules.load(&processes, owner, descriptor("echo", "echo-v1")),
        Ok(VmDynamicModuleLoadOutcome::Reused)
    );
    let lease = modules
        .open_lease(&processes, borrower, "echo")
        .expect("a live process may lease a loaded module");
    let before_wrong_owner = modules.snapshots();
    assert_eq!(
        modules.request_unload(&processes, stranger, "echo", false),
        Err(format!(
            "process {} does not own module echo",
            stranger.as_u64()
        ))
    );
    assert_eq!(modules.snapshots(), before_wrong_owner);

    assert_eq!(
        modules.request_unload(&processes, owner, "echo", false),
        Ok(VmDynamicModuleUnloadOutcome::ReferenceReleased)
    );
    assert_eq!(
        modules.request_unload(&processes, owner, "echo", false),
        Ok(VmDynamicModuleUnloadOutcome::Pending)
    );
    assert_eq!(
        modules.snapshots()[0].pending,
        Some(VmDynamicModulePendingAction::Unload)
    );
    assert_eq!(
        modules.load(&processes, stranger, descriptor("echo", "echo-v1")),
        Ok(VmDynamicModuleLoadOutcome::Reused)
    );
    assert_eq!(modules.snapshots()[0].pending, None);
    assert_eq!(
        modules.request_unload(&processes, stranger, "echo", false),
        Ok(VmDynamicModuleUnloadOutcome::Pending)
    );
    modules.close_lease(lease).expect("lease should close");
    assert!(modules.snapshots().is_empty());

    let event_count = modules.events().len();
    for invalid in [
        descriptor("", "artifact"),
        descriptor("bad", ""),
        descriptor("bad", "artifact").with_declared_name("wrong"),
        descriptor("bad", "artifact").with_init_success(false),
    ] {
        assert!(modules.load(&processes, owner, invalid).is_err());
    }
    assert!(modules.snapshots().is_empty());
    assert_eq!(modules.events().len(), event_count);
}

#[test]
fn ddll_suite_owner_exit_closes_owned_leases_and_preserves_other_references() {
    let mut processes = VmProcessTable::default();
    let first = processes.spawn_root(source("first"));
    let second = processes.spawn_root(source("second"));
    let mut modules = VmDynamicModuleRegistry::default();
    modules
        .load(&processes, first, descriptor("echo", "echo-v1"))
        .unwrap();
    modules
        .load(&processes, second, descriptor("echo", "echo-v1"))
        .unwrap();
    let first_lease = modules.open_lease(&processes, first, "echo").unwrap();
    let second_lease = modules.open_lease(&processes, second, "echo").unwrap();

    processes
        .exit_process(first, VmExitReason::Normal)
        .expect("first process should exit");
    modules.cleanup_owner(first);
    assert_eq!(modules.snapshots()[0].owner_references, vec![(second, 1)]);
    assert_eq!(modules.snapshots()[0].leases, vec![(second_lease, second)]);
    assert!(modules
        .events()
        .contains(&VmDynamicModuleEvent::LeaseClosed {
            name: "echo".to_string(),
            lease: first_lease,
            reason: VmDynamicModuleLeaseCloseReason::OwnerExited,
        }));

    assert_eq!(
        modules.request_unload(&processes, second, "echo", false),
        Ok(VmDynamicModuleUnloadOutcome::Pending)
    );
    modules.close_lease(second_lease).unwrap();
    assert!(modules.snapshots().is_empty());
}

#[test]
fn ddll_suite_reload_waits_for_leases_and_failed_replacement_is_atomic() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let borrower = processes.spawn_root(source("borrower"));
    let mut modules = VmDynamicModuleRegistry::default();
    modules
        .load(&processes, owner, descriptor("echo", "echo-v1"))
        .unwrap();
    let lease = modules.open_lease(&processes, borrower, "echo").unwrap();

    assert_eq!(
        modules.request_reload(&processes, owner, descriptor("echo", "echo-v2")),
        Ok(VmDynamicModuleReloadOutcome::Pending)
    );
    assert_eq!(
        modules.snapshots()[0].pending,
        Some(VmDynamicModulePendingAction::Reload(descriptor(
            "echo", "echo-v2"
        )))
    );
    let before_failed_reload = modules.snapshots();
    assert!(modules
        .request_reload(
            &processes,
            owner,
            descriptor("echo", "broken-v3").with_init_success(false)
        )
        .is_err());
    assert_eq!(modules.snapshots(), before_failed_reload);

    modules.close_lease(lease).unwrap();
    assert_eq!(modules.snapshots()[0].artifact_id, "echo-v2");
    assert_eq!(modules.snapshots()[0].pending, None);
    assert_eq!(
        modules.request_reload(&processes, owner, descriptor("echo", "echo-v3")),
        Ok(VmDynamicModuleReloadOutcome::Reloaded)
    );
    assert_eq!(modules.snapshots()[0].artifact_id, "echo-v3");
    assert_eq!(
        modules.request_reload(&processes, owner, descriptor("echo", "echo-v3")),
        Ok(VmDynamicModuleReloadOutcome::Unchanged)
    );
}

#[test]
fn ddll_suite_force_drain_permanent_module_and_event_order_contract() {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(source("owner"));
    let borrower = processes.spawn_root(source("borrower"));
    let mut modules = VmDynamicModuleRegistry::default();
    modules
        .load(&processes, owner, descriptor("echo", "echo-v1"))
        .unwrap();
    let first = modules.open_lease(&processes, owner, "echo").unwrap();
    let second = modules.open_lease(&processes, borrower, "echo").unwrap();
    assert_eq!(
        modules.request_unload(&processes, owner, "echo", true),
        Ok(VmDynamicModuleUnloadOutcome::Unloaded)
    );
    assert!(modules.snapshots().is_empty());
    assert_eq!(
        &modules.events()[modules.events().len() - 3..],
        &[
            VmDynamicModuleEvent::LeaseClosed {
                name: "echo".to_string(),
                lease: first,
                reason: VmDynamicModuleLeaseCloseReason::ForcedUnload,
            },
            VmDynamicModuleEvent::LeaseClosed {
                name: "echo".to_string(),
                lease: second,
                reason: VmDynamicModuleLeaseCloseReason::ForcedUnload,
            },
            VmDynamicModuleEvent::Unloaded {
                name: "echo".to_string(),
                artifact_id: "echo-v1".to_string(),
            },
        ]
    );

    modules
        .load(
            &processes,
            owner,
            descriptor("locked", "linked-in").with_permanent(true),
        )
        .unwrap();
    let before = modules.snapshots();
    assert_eq!(
        modules.request_unload(&processes, owner, "locked", true),
        Err("module locked is permanent".to_string())
    );
    assert_eq!(modules.snapshots(), before);
    processes.exit_process(owner, VmExitReason::Normal).unwrap();
    modules.cleanup_owner(owner);
    assert_eq!(modules.snapshots()[0].name, "locked");
    assert!(modules.snapshots()[0].permanent);
}
