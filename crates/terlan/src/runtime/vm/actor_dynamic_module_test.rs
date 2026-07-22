use super::super::super::dynamic_module::{
    VmDynamicModuleDescriptor, VmDynamicModuleLoadOutcome, VmDynamicModulePendingAction,
    VmDynamicModuleUnloadOutcome,
};
use super::super::{VmActorRuntime, VmExitReason, VmProcessSource};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.DynamicModuleActorParity", name, 0)
}

#[test]
fn actor_dynamic_module_lifecycle_is_vm_owned_and_exit_driven() {
    let mut actors = VmActorRuntime::default();
    let owner = actors.spawn_root(source("owner"));
    let borrower = actors.spawn_root(source("borrower"));

    assert_eq!(
        actors.load_dynamic_module(owner, VmDynamicModuleDescriptor::new("echo", "echo-v1")),
        Ok(VmDynamicModuleLoadOutcome::Loaded)
    );
    let lease = actors
        .open_dynamic_module_lease(borrower, "echo")
        .expect("live actor should open module lease");
    assert!(lease.as_u64() > 0);
    assert_eq!(
        actors.unload_dynamic_module(owner, "echo", false),
        Ok(VmDynamicModuleUnloadOutcome::Pending)
    );
    assert_eq!(
        actors.dynamic_module_snapshots()[0].pending,
        Some(VmDynamicModulePendingAction::Unload)
    );

    actors
        .exit_actor(borrower, VmExitReason::Normal)
        .expect("borrower exit should drive lease cleanup");
    assert!(actors.dynamic_module_snapshots().is_empty());
}
