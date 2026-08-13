use super::super::{
    VmActorReceive, VmActorRuntime, VmExitReason, VmProcessSource, VmRuntimeEnvironmentProfile,
};
use crate::runtime::vm::process::VmProcessState;
use crate::runtime::vm::scheduler::VmSchedulerDecision;
use crate::runtime::vm::statistics::VmRuntimeStatisticsDelta;
use crate::runtime::vm::ReplValue;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("parity.Statistics", name, 0)
}

fn profile(schedulers: usize) -> VmRuntimeEnvironmentProfile {
    VmRuntimeEnvironmentProfile::new(32, schedulers).expect("statistics profile")
}

#[test]
fn statistics_suite_reductions_zero_diff_and_large_counter_contract() {
    let mut runtime = VmActorRuntime::default();
    let worker = runtime.spawn_root(source("worker"));
    let baseline = runtime
        .environment_snapshot(profile(1))
        .expect("baseline statistics");

    let run = runtime
        .run_next(|process, slice| {
            assert_eq!(process.pid, worker);
            assert_eq!(slice.pid, worker);
            VmSchedulerDecision::Yield { reductions: 7 }
        })
        .expect("run worker slice");
    assert_eq!(run.reductions_charged, 7);

    let (current, delta) = runtime
        .statistics_since(profile(1), &baseline)
        .expect("updated statistics");
    assert_eq!(
        delta,
        VmRuntimeStatisticsDelta {
            reductions: 7,
            scheduler_slices: 1,
            ..VmRuntimeStatisticsDelta::default()
        }
    );
    assert_eq!(
        current
            .statistics_delta_since(&current)
            .expect("consecutive immutable snapshots"),
        VmRuntimeStatisticsDelta::default()
    );

    let mut large_earlier = current.clone();
    large_earlier.total_reductions = u64::from(u32::MAX) - 5;
    let mut large_current = large_earlier.clone();
    large_current.total_reductions += 19;
    assert_eq!(
        large_current
            .statistics_delta_since(&large_earlier)
            .expect("large reduction counter")
            .reductions,
        19
    );

    let mut regressed = large_current.clone();
    regressed.total_reductions = large_earlier.total_reductions - 1;
    assert_eq!(
        regressed
            .statistics_delta_since(&large_earlier)
            .expect_err("counter regression must be rejected"),
        format!(
            "VM scheduler reduction count regressed from {} to {}",
            large_earlier.total_reductions, regressed.total_reductions
        )
    );
    let incompatible = runtime
        .environment_snapshot(profile(2))
        .expect("different profile snapshot");
    assert_eq!(
        incompatible
            .statistics_delta_since(&current)
            .expect_err("different profiles must not be compared"),
        "cannot compare VM statistics from different runtime profiles"
    );
}

#[test]
fn statistics_suite_run_queue_active_tasks_and_owned_counters_contract() {
    let mut runtime = VmActorRuntime::default();
    let receiver = runtime.spawn_root(source("receiver"));
    let sender = runtime.spawn_root(source("sender"));
    let disposable = runtime.spawn_root(source("disposable"));
    let initial = runtime
        .observation_snapshot(profile(1))
        .expect("initial statistics");
    assert_eq!(initial.environment.live_processes, 3);
    assert_eq!(initial.environment.run_queue, 3);

    let blocked = runtime
        .run_next(|process, _| {
            assert_eq!(process.pid, receiver);
            VmSchedulerDecision::Block { reductions: 3 }
        })
        .expect("block receiver");
    assert_eq!(blocked.pid, Some(receiver));
    let parked = runtime
        .observation_snapshot(profile(1))
        .expect("parked statistics");
    assert_eq!(parked.environment.live_processes, 3);
    assert_eq!(parked.environment.run_queue, 2);
    assert_eq!(
        parked
            .environment
            .statistics_delta_since(&initial.environment)
            .expect("scheduler work delta"),
        VmRuntimeStatisticsDelta {
            reductions: 3,
            scheduler_slices: 1,
            ..VmRuntimeStatisticsDelta::default()
        }
    );
    assert!(parked
        .processes
        .iter()
        .any(|process| { process.pid == receiver && process.state == VmProcessState::Blocked }));

    runtime
        .send(sender, receiver, ReplValue::Int(42))
        .expect("wake receiver");
    let timer = runtime
        .send_after(sender, receiver, ReplValue::Atom("late".to_string()), 10, 5)
        .expect("start timer");
    runtime
        .cancel_delayed_send(timer, 11)
        .expect("cancel timer");
    runtime
        .exit_actor(disposable, VmExitReason::Normal)
        .expect("exit queued actor");

    let completed = runtime
        .observation_snapshot(profile(1))
        .expect("completed statistics");
    assert_eq!(completed.environment.total_processes, 3);
    assert_eq!(completed.environment.live_processes, 2);
    assert_eq!(completed.environment.exited_processes, 1);
    assert_eq!(completed.environment.run_queue, 2);
    assert_eq!(completed.environment.mailbox_messages, 1);
    assert_eq!(completed.environment.active_timers, 0);
    let delta = completed
        .environment
        .statistics_delta_since(&parked.environment)
        .expect("owned counter delta");
    assert_eq!(delta.processes_created, 0);
    assert_eq!(delta.processes_exited, 1);
    assert_eq!(delta.timers_started, 1);
    assert_eq!(delta.timers_fired, 0);
    assert_eq!(delta.timers_cancelled, 1);
    assert!(delta.reductions > 0);
    assert!(delta.memory_reductions > 0);
    assert_eq!(delta.scheduler_slices, 0);
    assert_eq!(delta.scheduler_preemptions, 0);
    assert_eq!(completed.environment.run_queue, runtime.scheduled_len());
    assert!(completed.processes.iter().any(|process| {
        process.pid == receiver
            && process.state == VmProcessState::Runnable
            && process.mailbox_messages == 1
    }));
    assert!(matches!(
        runtime
            .receive_next_or_block(receiver)
            .expect("receive queued message"),
        VmActorReceive::Message(message) if message.payload == ReplValue::Int(42)
    ));
}
