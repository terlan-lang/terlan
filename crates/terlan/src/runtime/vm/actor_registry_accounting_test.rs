use super::*;

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("app.RegistryAccounting", function, 0)
}

fn process_reductions(runtime: &VmActorRuntime, pid: VmProcessId) -> u64 {
    runtime
        .processes
        .get(pid)
        .expect("accounted process")
        .reductions
}

#[test]
fn actor_runtime_charges_only_successful_registry_mutations_to_owner() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("owner"));
    let other = runtime.spawn_root(source("other"));
    let owner_before = process_reductions(&runtime, owner);
    let total_before = runtime.scheduler.metrics().total_reductions;

    runtime
        .register_name("worker", owner)
        .expect("register name");
    runtime
        .register_name("worker", owner)
        .expect("idempotent registration");
    let alias = runtime.create_alias(owner).expect("create alias");

    let total_before_observations = runtime.scheduler.metrics().total_reductions;
    assert_eq!(runtime.lookup_name("worker"), Some(owner));
    assert_eq!(runtime.registered_names(), ["worker".to_string()]);
    assert_eq!(runtime.resolve_alias(alias), Some(owner));
    assert_eq!(runtime.aliases_for_process(owner), [alias]);
    assert_eq!(runtime.alias_count(), 1);
    assert_eq!(
        runtime.scheduler.metrics().total_reductions,
        total_before_observations
    );

    assert_eq!(
        runtime.unregister_name("worker").expect("remove name"),
        owner
    );
    assert_eq!(runtime.remove_alias(alias).expect("remove alias"), owner);
    assert_eq!(process_reductions(&runtime, owner) - owner_before, 5);
    assert_eq!(
        runtime.scheduler.metrics().total_reductions - total_before,
        5
    );
    assert_eq!(process_reductions(&runtime, other), 0);
    assert_eq!(runtime.total_memory_reductions(), 0);

    runtime
        .register_name("other", other)
        .expect("register conflicting owner");
    let total_before_rejections = runtime.scheduler.metrics().total_reductions;
    let missing = VmProcessId::from_raw_for_test(404);
    assert_eq!(
        runtime
            .register_name("", owner)
            .expect_err("empty name must fail"),
        "actor name cannot be empty"
    );
    assert_eq!(
        runtime
            .register_name("missing-owner", missing)
            .expect_err("missing owner must fail"),
        "cannot register missing process 404"
    );
    assert_eq!(
        runtime
            .register_name("other", owner)
            .expect_err("conflicting owner must fail"),
        format!(
            "actor name `other` is already registered to process {}",
            other.as_u64()
        )
    );
    assert_eq!(
        runtime
            .unregister_name("missing")
            .expect_err("missing name must fail"),
        "actor name `missing` is not registered"
    );
    assert_eq!(
        runtime
            .create_alias(missing)
            .expect_err("missing alias owner must fail"),
        "cannot alias missing process 404"
    );
    assert_eq!(
        runtime
            .remove_alias(alias)
            .expect_err("stale alias must fail"),
        format!("process alias {} is not registered", alias.as_u64())
    );
    assert_eq!(
        runtime.scheduler.metrics().total_reductions,
        total_before_rejections
    );
    assert_eq!(process_reductions(&runtime, other), 1);

    runtime
        .exit_actor(owner, VmExitReason::Normal)
        .expect("exit owner");
    let total_before_exited = runtime.scheduler.metrics().total_reductions;
    assert_eq!(
        runtime
            .register_name("late", owner)
            .expect_err("exited owner name must fail"),
        format!("cannot register exited process {}", owner.as_u64())
    );
    assert_eq!(
        runtime
            .create_alias(owner)
            .expect_err("exited alias owner must fail"),
        format!("cannot alias exited process {}", owner.as_u64())
    );
    assert_eq!(
        runtime.scheduler.metrics().total_reductions,
        total_before_exited
    );
}
