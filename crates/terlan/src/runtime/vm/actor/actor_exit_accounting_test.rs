use super::super::*;
use crate::runtime::vm::process::VmProcessState;

fn source(function: &str) -> VmProcessSource {
    VmProcessSource::new("app.ExitAccounting", function, 0)
}

fn process_reductions(runtime: &VmActorRuntime, pid: VmProcessId) -> u64 {
    runtime
        .processes
        .get(pid)
        .expect("accounted process")
        .reductions
}

#[test]
fn actor_runtime_charges_only_newly_initiated_exit_to_exiting_actor() {
    let mut runtime = VmActorRuntime::default();
    let actor = runtime.spawn_root(source("actor"));
    runtime
        .register_name("actor", actor)
        .expect("register actor");
    let alias = runtime.create_alias(actor).expect("create alias");
    runtime
        .processes
        .get_mut(actor)
        .expect("actor")
        .add_resource_handle("resource:1");
    let actor_before = process_reductions(&runtime, actor);
    let total_before = runtime.scheduler.metrics().total_reductions;

    assert_eq!(
        runtime
            .exit_actor(actor, VmExitReason::Normal)
            .expect("exit actor"),
        ["resource:1".to_string()]
    );
    assert_eq!(process_reductions(&runtime, actor) - actor_before, 1);
    assert_eq!(
        runtime.scheduler.metrics().total_reductions - total_before,
        1
    );
    assert_eq!(runtime.lookup_name("actor"), None);
    assert_eq!(runtime.resolve_alias(alias), None);
    assert!(matches!(
        runtime.processes.get(actor).expect("exited actor").state,
        VmProcessState::Exited(VmExitReason::Normal)
    ));
    assert_eq!(runtime.latest_fatal_diagnostic(), None);

    let total_before_repeated = runtime.scheduler.metrics().total_reductions;
    assert!(runtime
        .exit_actor(actor, VmExitReason::Killed)
        .expect("repeated exit remains idempotent")
        .is_empty());
    assert_eq!(
        runtime.scheduler.metrics().total_reductions,
        total_before_repeated
    );
    assert_eq!(
        runtime
            .exit_actor(VmProcessId::from_raw_for_test(404), VmExitReason::Normal)
            .expect_err("missing actor cannot exit"),
        "missing process 404"
    );
    assert_eq!(
        runtime.scheduler.metrics().total_reductions,
        total_before_repeated
    );

    let origin = runtime.spawn_root(source("origin"));
    let linked = runtime.spawn_root(source("linked"));
    runtime.link_actors(origin, linked).expect("link actors");
    let origin_before = process_reductions(&runtime, origin);
    let linked_before = process_reductions(&runtime, linked);
    let total_before_cascade = runtime.scheduler.metrics().total_reductions;
    runtime
        .exit_actor(origin, VmExitReason::Killed)
        .expect("cascade abnormal exit");
    assert_eq!(process_reductions(&runtime, origin) - origin_before, 1);
    assert_eq!(process_reductions(&runtime, linked) - linked_before, 0);
    assert_eq!(
        runtime.scheduler.metrics().total_reductions - total_before_cascade,
        1
    );
    assert!(matches!(
        runtime.processes.get(linked).expect("linked actor").state,
        VmProcessState::Exited(VmExitReason::Killed)
    ));
    let diagnostic = runtime
        .latest_fatal_diagnostic()
        .expect("abnormal exit captures a diagnostic");
    assert_eq!(diagnostic.cause_code, "actor.killed");
    assert!(diagnostic
        .processes
        .iter()
        .any(|process| process.pid == origin.as_u64()));
    assert_eq!(runtime.total_memory_reductions(), 0);
}
