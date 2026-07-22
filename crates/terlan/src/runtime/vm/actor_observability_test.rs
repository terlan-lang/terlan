use super::{
    VmActorReceive, VmActorRuntime, VmExitReason, VmProcessSource, VmRuntimeEnvironmentProfile,
};
use crate::runtime::vm::process::VmProcessState;
use crate::runtime::vm::scheduler::VmSchedulerDecision;
use crate::runtime::vm::ReplValue;

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Observability", name, 0)
}

fn profile(process_limit: usize) -> VmRuntimeEnvironmentProfile {
    VmRuntimeEnvironmentProfile::new(process_limit, 1).expect("valid observation profile")
}

#[test]
fn actor_observation_correlates_receive_wakeup_schedule_and_timer_boundaries() {
    let mut runtime = VmActorRuntime::default();
    let sender = runtime.spawn_root(source("sender"));
    let recipient = runtime
        .spawn_child(sender, source("recipient"))
        .expect("spawn recipient");
    runtime
        .register_name("recipient", recipient)
        .expect("register recipient");

    let scheduled = runtime
        .run_next(|_, _| VmSchedulerDecision::Yield { reductions: 4 })
        .expect("run sender slice");
    assert_eq!(scheduled.pid, Some(sender));

    runtime
        .send(sender, recipient, ReplValue::Int(7))
        .expect("send first payload");
    assert!(matches!(
        runtime
            .receive_next_or_block(recipient)
            .expect("receive first payload"),
        VmActorReceive::Message(message) if message.payload == ReplValue::Int(7)
    ));
    assert_eq!(
        runtime
            .receive_next_or_block(recipient)
            .expect("block empty recipient"),
        VmActorReceive::Blocked
    );
    let timer = runtime
        .send_after(sender, recipient, ReplValue::Int(9), 0, 5)
        .expect("schedule delayed payload");

    let blocked = runtime
        .observation_snapshot(profile(4))
        .expect("blocked observation");
    assert_eq!(blocked.environment.total_processes, 2);
    assert_eq!(blocked.environment.live_processes, 2);
    assert_eq!(blocked.environment.active_timers, 1);
    assert_eq!(blocked.environment.timers_started, 1);
    assert_eq!(blocked.scheduler.total_slices, 1);
    assert_eq!(blocked.scheduler.total_memory_reductions, 4);
    assert_eq!(blocked.scheduler.total_reductions, 14);
    assert_eq!(
        blocked.scheduler.total_reductions,
        4 + blocked.scheduler.total_memory_reductions + 2 + 1 + 1 + 1 + 1
    );
    assert_eq!(blocked.timer_metrics.started, 1);
    assert_eq!(blocked.timers.len(), 1);
    assert_eq!(blocked.timers[0].id, timer);
    assert_eq!(blocked.timers[0].owner, sender);
    assert_eq!(blocked.timers[0].deadline_tick, 5);
    let blocked_recipient = blocked
        .processes
        .iter()
        .find(|process| process.pid == recipient)
        .expect("recipient snapshot");
    assert_eq!(blocked_recipient.state, VmProcessState::Blocked);
    assert_eq!(blocked_recipient.mailbox_messages, 0);
    assert_eq!(blocked_recipient.registered_names, vec!["recipient"]);

    runtime
        .send(sender, recipient, ReplValue::Int(8))
        .expect("wake recipient");
    let woken = runtime
        .observation_snapshot(profile(4))
        .expect("woken observation");
    let woken_recipient = woken
        .processes
        .iter()
        .find(|process| process.pid == recipient)
        .expect("woken recipient snapshot");
    assert_eq!(woken_recipient.state, VmProcessState::Runnable);
    assert_eq!(woken_recipient.mailbox_messages, 1);
    assert_ne!(woken, blocked);

    let advanced = runtime.advance_actor_timers(5);
    assert_eq!(advanced.deliveries.len(), 1);
    runtime
        .exit_actor(sender, VmExitReason::Normal)
        .expect("exit timer owner");
    let completed = runtime
        .observation_snapshot(profile(4))
        .expect("completed observation");
    assert_eq!(completed.environment.live_processes, 1);
    assert_eq!(completed.environment.exited_processes, 1);
    assert_eq!(completed.environment.active_timers, 0);
    assert_eq!(completed.environment.timers_fired, 1);
    assert!(completed.timers.is_empty());
    assert_eq!(completed.timer_metrics.fired, 1);
    assert_eq!(completed.timer_metrics.ordering_trace, vec![timer.as_u64()]);
    assert!(completed.processes.iter().any(|process| {
        process.pid == sender && process.state == VmProcessState::Exited(VmExitReason::Normal)
    }));
    assert_eq!(
        completed,
        runtime
            .observation_snapshot(profile(4))
            .expect("stable repeated observation")
    );
}

#[test]
fn actor_observation_records_owner_exit_cleanup_without_cross_process_damage() {
    let mut runtime = VmActorRuntime::default();
    let owner = runtime.spawn_root(source("owner"));
    let survivor = runtime.spawn_root(source("survivor"));
    runtime
        .register_name("survivor", survivor)
        .expect("register survivor");
    assert_eq!(
        runtime
            .receive_next_or_block(survivor)
            .expect("block survivor"),
        VmActorReceive::Blocked
    );
    let timer = runtime
        .send_after(owner, survivor, ReplValue::Int(11), 10, 10)
        .expect("schedule owner timer");

    runtime
        .exit_actor(owner, VmExitReason::Killed)
        .expect("exit timer owner");
    let snapshot = runtime
        .observation_snapshot(profile(2))
        .expect("owner-exit observation");

    assert_eq!(snapshot.environment.live_processes, 1);
    assert_eq!(snapshot.environment.exited_processes, 1);
    assert_eq!(snapshot.environment.active_timers, 0);
    assert_eq!(snapshot.timer_metrics.started, 1);
    assert_eq!(snapshot.timer_metrics.owner_exited, 1);
    assert_eq!(snapshot.timer_metrics.ordering_trace, vec![timer.as_u64()]);
    assert!(snapshot.timers.is_empty());
    assert_eq!(runtime.delayed_send_count(), 0);
    let survivor_snapshot = snapshot
        .processes
        .iter()
        .find(|process| process.pid == survivor)
        .expect("survivor snapshot");
    assert_eq!(survivor_snapshot.state, VmProcessState::Blocked);
    assert_eq!(survivor_snapshot.mailbox_messages, 0);
    assert_eq!(survivor_snapshot.registered_names, vec!["survivor"]);
    assert!(snapshot.processes.iter().any(|process| {
        process.pid == owner && process.state == VmProcessState::Exited(VmExitReason::Killed)
    }));
}
