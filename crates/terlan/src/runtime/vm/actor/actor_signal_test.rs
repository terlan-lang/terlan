use super::super::super::process::{VmExitReason, VmProcessSource, VmProcessState};
use super::super::super::scheduler::{VmSchedulerDecision, VmSchedulerOutcome};
use super::super::super::ReplValue;
use super::super::{VmActorReceive, VmActorRuntime};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Main", name, 0)
}

#[test]
fn monitor_completion_wakes_blocked_observer_before_actor_execution() {
    let mut runtime = VmActorRuntime::default();
    let observer = runtime.spawn_root(source("observer"));
    let target = runtime.spawn_root(source("target"));
    let monitor_ref = runtime
        .monitor_actor(observer, target)
        .expect("monitor target");
    assert_eq!(
        runtime
            .receive_next_or_block(observer)
            .expect("block observer"),
        VmActorReceive::Blocked
    );

    runtime
        .exit_actor(target, VmExitReason::Killed)
        .expect("exit target");

    assert_eq!(
        runtime
            .processes()
            .snapshot(observer)
            .expect("observer")
            .state,
        VmProcessState::Runnable
    );
    let run = runtime
        .run_next(|process, _| {
            assert_eq!(process.pid, observer);
            VmSchedulerDecision::Block { reductions: 1 }
        })
        .expect("run awakened observer");
    assert_eq!(run.pid, Some(observer));
    assert_eq!(run.outcome, VmSchedulerOutcome::Blocked);

    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(observer)
        .expect("receive monitor completion")
    else {
        panic!("monitor completion must be available");
    };
    assert_eq!(
        message.payload,
        ReplValue::Tuple(vec![
            ReplValue::Atom("down".to_string()),
            ReplValue::Int(monitor_ref.as_u64() as i64),
            ReplValue::Int(target.as_u64() as i64),
            ReplValue::Atom("killed".to_string()),
        ])
    );
}

#[test]
fn trapped_exit_wakes_blocked_linked_actor() {
    let mut runtime = VmActorRuntime::default();
    let trapping = runtime.spawn_root(source("trapping"));
    let linked = runtime.spawn_root(source("linked"));
    runtime
        .set_actor_trap_exits(trapping, true)
        .expect("enable trapped exits");
    runtime.link_actors(trapping, linked).expect("link actors");
    assert_eq!(
        runtime
            .receive_next_or_block(trapping)
            .expect("block trapping actor"),
        VmActorReceive::Blocked
    );

    runtime
        .exit_actor(linked, VmExitReason::Error("failed".to_string()))
        .expect("exit linked actor");

    assert_eq!(
        runtime
            .processes()
            .snapshot(trapping)
            .expect("trapping")
            .state,
        VmProcessState::Runnable
    );
    let VmActorReceive::Message(message) = runtime
        .receive_next_or_block(trapping)
        .expect("receive trapped exit")
    else {
        panic!("trapped exit must be available");
    };
    assert_eq!(
        message.payload,
        ReplValue::Tuple(vec![
            ReplValue::Atom("exit".to_string()),
            ReplValue::Int(linked.as_u64() as i64),
            ReplValue::Tuple(vec![
                ReplValue::Atom("error".to_string()),
                ReplValue::String("failed".to_string()),
            ]),
        ])
    );
}

#[test]
fn cascaded_monitor_completions_preserve_fifo_and_single_scheduler_entry() {
    let mut runtime = VmActorRuntime::default();
    let observer = runtime.spawn_root(source("observer"));
    let first = runtime.spawn_root(source("first"));
    let second = runtime.spawn_root(source("second"));
    let first_ref = runtime
        .monitor_actor(observer, first)
        .expect("monitor first");
    let second_ref = runtime
        .monitor_actor(observer, second)
        .expect("monitor second");
    runtime.link_actors(first, second).expect("link targets");
    runtime
        .receive_next_or_block(observer)
        .expect("block observer");

    runtime
        .exit_actor(first, VmExitReason::Killed)
        .expect("cascade linked exits");

    let first_run = runtime
        .run_next(|process, _| {
            assert_eq!(process.pid, observer);
            VmSchedulerDecision::Block { reductions: 1 }
        })
        .expect("run observer once");
    assert_eq!(first_run.pid, Some(observer));
    assert_eq!(first_run.outcome, VmSchedulerOutcome::Blocked);
    let idle = runtime
        .run_next(|_, _| panic!("duplicate wake must not enqueue observer twice"))
        .expect("poll after deduplicated wake");
    assert_eq!(idle.pid, None);
    assert_eq!(idle.outcome, VmSchedulerOutcome::Idle);

    for (expected_ref, expected_target) in [(first_ref, first), (second_ref, second)] {
        let VmActorReceive::Message(message) = runtime
            .receive_next_or_block(observer)
            .expect("receive ordered completion")
        else {
            panic!("monitor completion must be available");
        };
        assert_eq!(
            message.payload,
            ReplValue::Tuple(vec![
                ReplValue::Atom("down".to_string()),
                ReplValue::Int(expected_ref.as_u64() as i64),
                ReplValue::Int(expected_target.as_u64() as i64),
                ReplValue::Atom("killed".to_string()),
            ])
        );
    }
}

#[test]
fn untrapped_abnormal_exit_cascades_without_requeueing_exited_actor() {
    let mut runtime = VmActorRuntime::default();
    let origin = runtime.spawn_root(source("origin"));
    let linked = runtime.spawn_root(source("linked"));
    runtime
        .monitor_actor(linked, origin)
        .expect("monitor linked origin");
    runtime.link_actors(origin, linked).expect("link actors");
    runtime
        .receive_next_or_block(linked)
        .expect("block linked actor");

    runtime
        .exit_actor(origin, VmExitReason::Killed)
        .expect("cascade exit");

    for pid in [origin, linked] {
        assert_eq!(
            runtime
                .processes()
                .snapshot(pid)
                .expect("exited process")
                .state,
            VmProcessState::Exited(VmExitReason::Killed)
        );
    }
    let idle = runtime
        .run_next(|_, _| panic!("exited actors must not execute"))
        .expect("poll exited actors");
    assert_eq!(idle.pid, None);
    assert_eq!(idle.outcome, VmSchedulerOutcome::Idle);
}
