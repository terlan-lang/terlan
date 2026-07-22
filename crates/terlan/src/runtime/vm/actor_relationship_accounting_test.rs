use super::*;

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("app.RelationshipAccounting", function, 0)
}

fn process_reductions(runtime: &VmActorRuntime, pid: VmProcessId) -> u64 {
    runtime
        .processes()
        .get(pid)
        .expect("accounted process")
        .reductions
}

#[test]
fn actor_runtime_charges_only_successful_relationship_operations_to_initiator() {
    let mut runtime = VmActorRuntime::default();
    let initiator = runtime.spawn_root(source("initiator"));
    let peer = runtime.spawn_root(source("peer"));

    assert!(runtime.link_actors(initiator, peer).expect("create link"));
    assert!(!runtime
        .link_actors(initiator, peer)
        .expect("idempotent link"));
    assert!(runtime.unlink_actors(initiator, peer).expect("remove link"));
    assert!(!runtime
        .unlink_actors(initiator, peer)
        .expect("idempotent unlink"));

    let monitor_ref = runtime
        .monitor_actor(initiator, peer)
        .expect("create monitor");
    assert!(
        runtime
            .demonitor_actor(
                initiator,
                monitor_ref.clone(),
                VmActorDemonitorOptions::default(),
            )
            .expect("remove monitor")
            .removed
    );
    assert!(
        !runtime
            .demonitor_actor(
                initiator,
                monitor_ref.clone(),
                VmActorDemonitorOptions::default(),
            )
            .expect("idempotent demonitor")
            .removed
    );
    runtime
        .set_actor_trap_exits(initiator, true)
        .expect("enable trap exits");
    runtime
        .set_actor_trap_exits(initiator, true)
        .expect("idempotent trap-exit update");

    assert_eq!(process_reductions(&runtime, initiator), 9);
    assert_eq!(process_reductions(&runtime, peer), 0);
    assert_eq!(runtime.scheduler.metrics().total_reductions, 9);
    assert_eq!(runtime.total_memory_reductions(), 0);

    let total_before_rejections = runtime.scheduler.metrics().total_reductions;
    let missing = VmProcessId::from_raw_for_test(404);
    assert_eq!(
        runtime
            .link_actors(initiator, initiator)
            .expect_err("self link"),
        format!("cannot link process {} to itself", initiator.as_u64())
    );
    assert_eq!(
        runtime
            .unlink_actors(initiator, missing)
            .expect_err("missing peer"),
        "cannot unlink missing process 404"
    );
    assert_eq!(
        runtime
            .monitor_actor(initiator, missing)
            .expect_err("missing target"),
        "cannot monitor missing process 404"
    );
    assert_eq!(
        runtime
            .demonitor_actor(missing, monitor_ref, VmActorDemonitorOptions::default())
            .expect_err("missing observer"),
        "cannot demonitor from missing process 404"
    );
    assert_eq!(
        runtime
            .set_actor_trap_exits(missing, true)
            .expect_err("missing actor"),
        "cannot inspect trap exits for missing process 404"
    );
    assert_eq!(
        runtime.scheduler.metrics().total_reductions,
        total_before_rejections
    );
    assert_eq!(process_reductions(&runtime, initiator), 9);
}
