use super::super::super::memory::VmMemoryLimits;
use super::super::super::process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState};
use super::super::super::scheduler::{VmSchedulerClass, VmSchedulerDecision};
use super::super::super::ReplValue;
use super::super::{VmActorReceive, VmActorRuntime, VmActorSpawnOptions};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

fn process_reductions(runtime: &VmActorRuntime, pid: VmProcessId) -> u64 {
    runtime
        .processes()
        .get(pid)
        .expect("accounted process")
        .reductions
}

#[test]
fn actor_spawn_charges_only_successful_child_creation_to_parent() {
    let mut runtime = VmActorRuntime::default();
    let parent = runtime.spawn_root(source("parent"));

    let plain = runtime
        .spawn_child(parent, source("plain"))
        .expect("plain child");
    assert_eq!(process_reductions(&runtime, parent), 1);
    assert_eq!(process_reductions(&runtime, plain), 0);
    assert_eq!(runtime.scheduler.metrics().total_reductions, 1);

    let typed = runtime
        .spawn_child_with_options(
            parent,
            source("typed"),
            VmActorSpawnOptions::default()
                .with_scheduler_class(VmSchedulerClass::Priority)
                .linked()
                .monitored(),
        )
        .expect("typed child");
    assert_eq!(process_reductions(&runtime, parent), 2);
    assert_eq!(process_reductions(&runtime, typed.pid), 0);
    assert_eq!(runtime.scheduler.metrics().total_reductions, 2);

    let total_before_missing = runtime.scheduler.metrics().total_reductions;
    assert_eq!(
        runtime
            .spawn_child(VmProcessId::from_raw_for_test(404), source("missing"))
            .expect_err("missing parent"),
        "missing parent process 404"
    );
    assert_eq!(
        runtime.scheduler.metrics().total_reductions,
        total_before_missing
    );

    let exited = runtime.spawn_root(source("exited"));
    runtime
        .exit_actor(exited, VmExitReason::Normal)
        .expect("exit parent");
    let total_before_exited = runtime.scheduler.metrics().total_reductions;
    assert_eq!(
        runtime
            .spawn_child_with_options(
                exited,
                source("orphan"),
                VmActorSpawnOptions::default().linked().monitored(),
            )
            .expect_err("exited parent"),
        format!("cannot spawn child from exited process {}", exited.as_u64())
    );
    assert_eq!(
        runtime.scheduler.metrics().total_reductions,
        total_before_exited
    );
}

#[test]
fn actor_spawn_default_creates_and_schedules_plain_child() {
    let mut runtime = VmActorRuntime::default();
    let parent = runtime.spawn_root(source("parent"));

    let child = runtime
        .spawn_child(parent, source("child"))
        .expect("plain child spawn");

    let process = runtime.processes().get(child).expect("child process");
    assert_eq!(process.parent, Some(parent));
    assert_eq!(process.source, source("child"));
    assert_eq!(process.state, VmProcessState::Runnable);
    assert_eq!(
        runtime
            .failure_snapshot(parent)
            .expect("relationships")
            .links,
        []
    );
}

#[test]
fn actor_spawn_applies_priority_link_and_monitor_as_one_typed_plan() {
    let mut runtime = VmActorRuntime::with_runtime_identity(
        VmMemoryLimits::new(1024, 4096).expect("memory limits"),
        "node-a",
        9,
    )
    .expect("runtime identity");
    let parent = runtime.spawn_root(source("parent"));
    let spawned = runtime
        .spawn_child_with_options(
            parent,
            source("priority-child"),
            VmActorSpawnOptions::default()
                .with_scheduler_class(VmSchedulerClass::Priority)
                .linked()
                .monitored(),
        )
        .expect("typed child spawn");

    let monitor_ref = spawned.monitor_ref.expect("monitor reference");
    assert_eq!(monitor_ref.reference().node_id(), "node-a");
    assert_eq!(monitor_ref.reference().epoch(), 9);
    assert_eq!(monitor_ref.as_u64(), 1);
    let relationships = runtime.failure_snapshot(parent).expect("relationships");
    assert_eq!(relationships.links, [spawned.pid]);
    assert_eq!(relationships.monitoring.len(), 1);
    assert_eq!(relationships.monitoring[0].monitor_ref, monitor_ref);
    assert_eq!(relationships.monitoring[0].peer, spawned.pid);

    let run = runtime
        .run_next(|_, _| VmSchedulerDecision::Yield { reductions: 1 })
        .expect("priority child should run first");
    assert_eq!(run.pid, Some(spawned.pid));
}

#[test]
fn actor_spawn_monitor_delivers_child_completion_to_parent_mailbox() {
    let mut runtime = VmActorRuntime::default();
    let parent = runtime.spawn_root(source("parent"));
    let spawned = runtime
        .spawn_child_with_options(
            parent,
            source("child"),
            VmActorSpawnOptions::default().monitored(),
        )
        .expect("monitored child spawn");

    runtime
        .exit_actor(spawned.pid, VmExitReason::Normal)
        .expect("child completion");

    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(parent)
        .expect("parent receive")
    else {
        panic!("monitor completion must reach parent mailbox");
    };
    assert_eq!(
        message.payload,
        ReplValue::Tuple(vec![
            ReplValue::Atom("down".to_string()),
            ReplValue::Int(1),
            ReplValue::Int(spawned.pid.as_u64() as i64),
            ReplValue::Atom("normal".to_string()),
        ])
    );
    assert!(runtime
        .failure_snapshot(parent)
        .expect("relationships")
        .monitoring
        .is_empty());
}

#[test]
fn actor_spawn_rejects_missing_and_exited_parents_without_consuming_identity() {
    let mut runtime = VmActorRuntime::default();
    let exited = runtime.spawn_root(source("exited"));
    runtime
        .exit_actor(exited, VmExitReason::Normal)
        .expect("parent exit");
    let options = VmActorSpawnOptions::default().linked().monitored();

    assert_eq!(
        runtime
            .spawn_child_with_options(exited, source("orphan"), options)
            .expect_err("exited parent"),
        "cannot spawn child from exited process 1"
    );
    assert_eq!(
        runtime
            .spawn_child(exited, source("plain-orphan"))
            .expect_err("default spawn must preserve exited-parent error"),
        "cannot spawn child from exited process 1"
    );
    assert_eq!(
        runtime
            .spawn_child_with_options(
                VmProcessId::from_raw_for_test(99),
                source("missing"),
                options,
            )
            .expect_err("missing parent"),
        "cannot spawn child from missing process 99"
    );
    assert_eq!(
        runtime
            .failure_snapshot(VmProcessId::from_raw_for_test(99))
            .expect_err("missing relationship snapshot"),
        "cannot inspect missing process 99"
    );

    let live = runtime.spawn_root(source("live"));
    let spawned = runtime
        .spawn_child_with_options(live, source("child"), options)
        .expect("first valid spawn");
    assert_eq!(live.as_u64(), 2);
    assert_eq!(spawned.pid.as_u64(), 3);
    assert_eq!(spawned.monitor_ref.expect("monitor").as_u64(), 1);
}

#[test]
fn actor_runtime_identity_rejects_invalid_reference_namespaces() {
    let limits = VmMemoryLimits::new(1024, 4096).expect("memory limits");

    assert_eq!(
        VmActorRuntime::with_runtime_identity(limits.clone(), "  ", 1).expect_err("blank node"),
        "VM reference node id must not be empty"
    );
    assert_eq!(
        VmActorRuntime::with_runtime_identity(limits, "node-a", 0).expect_err("zero epoch"),
        "VM reference epoch must be non-zero"
    );
}
