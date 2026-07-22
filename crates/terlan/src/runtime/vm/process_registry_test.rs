use super::{VmExitReason, VmProcessId, VmProcessRegistryError, VmProcessSource, VmProcessTable};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Registry", name, 0)
}

#[test]
fn process_registry_lists_resolves_and_unregisters_names_deterministically() {
    let mut table = VmProcessTable::default();
    let worker = table.spawn_root(source("worker"));

    assert_eq!(table.registered_name_count(), 0);
    assert!(table.registered_names().is_empty());
    table
        .register_name("worker.secondary", worker)
        .expect("secondary name should register");
    table
        .register_name("worker.primary", worker)
        .expect("primary name should register");

    assert_eq!(table.registered_name_count(), 2);
    assert_eq!(
        table.registered_names(),
        ["worker.primary", "worker.secondary"]
    );
    assert_eq!(
        table.names_for_process(worker),
        ["worker.primary", "worker.secondary"]
    );
    assert_eq!(table.lookup_name("worker.primary"), Some(worker));
    assert_eq!(
        table
            .unregister_name("worker.primary")
            .expect("primary name should unregister"),
        worker
    );
    assert_eq!(table.lookup_name("worker.primary"), None);
    assert_eq!(table.registered_names(), ["worker.secondary"]);
}

#[test]
fn process_registry_bulk_removal_is_owner_scoped_and_ordered() {
    let mut table = VmProcessTable::default();
    let first = table.spawn_root(source("first"));
    let second = table.spawn_root(source("second"));
    table
        .register_name("first.z", first)
        .expect("first alias should register");
    table
        .register_name("second", second)
        .expect("second name should register");
    table
        .register_name("first.a", first)
        .expect("second alias should register");

    assert_eq!(
        table
            .unregister_process_names(first)
            .expect("first process names should unregister"),
        ["first.a", "first.z"]
    );
    assert_eq!(table.registered_names(), ["second"]);
    assert_eq!(table.lookup_name("second"), Some(second));
}

#[test]
fn process_registry_failures_are_typed_and_side_effect_free() {
    let mut table = VmProcessTable::default();
    let owner = table.spawn_root(source("owner"));
    let contender = table.spawn_root(source("contender"));
    let missing = VmProcessId::from_raw_for_test(404);
    table
        .register_name("service", owner)
        .expect("service name should register");

    assert_eq!(
        table
            .unregister_name("missing")
            .expect_err("missing name should fail"),
        VmProcessRegistryError::NameNotRegistered("missing".to_string())
    );
    assert_eq!(
        table
            .unregister_process_names(missing)
            .expect_err("missing process should fail"),
        VmProcessRegistryError::MissingProcess(missing)
    );
    assert_eq!(
        table
            .register_name("service", contender)
            .expect_err("name conflict should fail"),
        VmProcessRegistryError::Conflict {
            name: "service".to_string(),
            existing: owner,
        }
    );
    assert_eq!(table.registered_names(), ["service"]);
    assert_eq!(table.lookup_name("service"), Some(owner));
}

#[test]
fn process_registry_exit_removes_every_name_before_reuse() {
    let mut table = VmProcessTable::default();
    let exiting = table.spawn_root(source("exiting"));
    let replacement = table.spawn_root(source("replacement"));
    table
        .register_name("service", exiting)
        .expect("service name should register");
    table
        .register_name("service.health", exiting)
        .expect("health name should register");

    table
        .exit_process(exiting, VmExitReason::Normal)
        .expect("process should exit");

    assert!(table.registered_names().is_empty());
    table
        .register_name("service", replacement)
        .expect("released name should be reusable");
    assert_eq!(table.lookup_name("service"), Some(replacement));
}

#[test]
fn process_registry_reuses_name_after_high_churn_exit_cleanup() {
    const ITERATIONS: usize = 4_096;

    let mut table = VmProcessTable::default();
    let mut previous = None;

    for iteration in 0..ITERATIONS {
        let owner = table.spawn_root(source("churn_owner"));
        assert_ne!(Some(owner), previous, "process identity must not be reused");

        table
            .register_name("churn.worker", owner)
            .expect("fresh owner must acquire released name");
        assert_eq!(table.lookup_name("churn.worker"), Some(owner));

        table
            .exit_process(owner, VmExitReason::Killed)
            .expect("owner exit must complete");
        assert_eq!(table.lookup_name("churn.worker"), None);
        assert!(table.registered_names().is_empty());
        assert_eq!(
            table
                .register_name("churn.worker", owner)
                .expect_err("exited owner must not reacquire name"),
            VmProcessRegistryError::ExitedProcess(owner),
            "iteration {iteration}"
        );

        previous = Some(owner);
    }
}

#[test]
fn process_registry_enumerates_only_live_processes_in_allocation_order() {
    let mut table = VmProcessTable::default();
    let first = table.spawn_root(source("first"));
    let exited = table.spawn_root(source("exited"));
    let third = table.spawn_root(source("third"));
    table
        .exit_process(exited, VmExitReason::Normal)
        .expect("middle process should exit");

    assert_eq!(table.live_process_ids(), [first, third]);
}
