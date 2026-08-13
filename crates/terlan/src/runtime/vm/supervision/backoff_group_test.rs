use super::{
    VmSupervisionBackoffCompletion, VmSupervisionBackoffQueue, VmSupervisionBackoffStart,
    VmSupervisionRestartRequest,
};
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessId, VmProcessSource, VmProcessState, VmProcessTable},
    scheduler::VmScheduler,
    supervision::{
        VmChildSpec, VmRestartBackoffSchedule, VmRestartPolicy, VmSupervisionRestart,
        VmSupervisionSystem, VmSupervisorId,
    },
    timer::VmTimerTable,
};

fn source(name: &str) -> VmProcessSource {
    VmProcessSource::new("app.Worker", name, 0)
}

fn child_pid(
    supervision: &VmSupervisionSystem,
    supervisor: VmSupervisorId,
    child_id: &str,
) -> VmProcessId {
    supervision
        .snapshot(supervisor)
        .expect("snapshot")
        .children
        .into_iter()
        .find(|child| child.child_id == child_id)
        .expect("child snapshot")
        .pid
}

fn assert_exited(processes: &VmProcessTable, pid: VmProcessId) {
    assert!(matches!(
        processes.get(pid).map(|process| &process.state),
        Some(VmProcessState::Exited(VmExitReason::Killed))
    ));
}

#[test]
fn one_for_all_backoff_restarts_children_at_individual_vm_deadlines() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let mut timers = VmTimerTable::default();
    let mut scheduler = VmScheduler::default();
    let mut backoff = VmSupervisionBackoffQueue::default();
    let supervisor = supervision.create_supervisor_with_policy("root", VmRestartPolicy::OneForAll);
    let alpha = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("alpha", source("alpha"), 3)
                .with_restart_backoff(VmRestartBackoffSchedule::exponential(5, 20)),
        )
        .expect("alpha");
    let beta = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("beta", source("beta"), 3)
                .with_restart_backoff(VmRestartBackoffSchedule::exponential(10, 40)),
        )
        .expect("beta");

    let VmSupervisionBackoffStart::Deferred {
        restarted_immediately,
        scheduled,
    } = backoff
        .schedule_restart(
            &mut supervision,
            &mut timers,
            &mut processes,
            VmSupervisionRestartRequest::new(supervisor, "alpha", VmExitReason::Killed, 100),
        )
        .expect("schedule group")
    else {
        panic!("group should be deferred");
    };
    assert!(restarted_immediately.is_empty());
    assert_eq!(
        scheduled
            .iter()
            .map(|restart| (restart.child_id.as_str(), restart.deadline_tick))
            .collect::<Vec<_>>(),
        vec![("alpha", 105), ("beta", 110)]
    );
    assert_exited(&processes, alpha);
    assert_exited(&processes, beta);

    let alpha_event = timers
        .advance_clock(&mut processes, &mut scheduler, 105)
        .remove(0);
    let alpha_completion = backoff
        .handle_timer_event(&mut supervision, &mut processes, &alpha_event)
        .expect("handle alpha")
        .expect("alpha completion");
    assert!(matches!(
        alpha_completion,
        VmSupervisionBackoffCompletion::Restarted(VmSupervisionRestart::Restarted {
            old_pid,
            restart_delay_ms: 5,
            ..
        }) if old_pid == alpha
    ));
    assert_ne!(child_pid(&supervision, supervisor, "alpha"), alpha);
    assert_eq!(child_pid(&supervision, supervisor, "beta"), beta);

    let beta_event = timers
        .advance_clock(&mut processes, &mut scheduler, 110)
        .remove(0);
    let beta_completion = backoff
        .handle_timer_event(&mut supervision, &mut processes, &beta_event)
        .expect("handle beta")
        .expect("beta completion");
    assert!(matches!(
        beta_completion,
        VmSupervisionBackoffCompletion::Restarted(VmSupervisionRestart::Restarted {
            old_pid,
            restart_delay_ms: 10,
            ..
        }) if old_pid == beta
    ));
    assert_ne!(child_pid(&supervision, supervisor, "beta"), beta);
    assert_eq!(backoff.pending_len(), 0);
    assert_eq!(
        supervision
            .snapshot(supervisor)
            .expect("snapshot")
            .restart_history
            .len(),
        2
    );
}

#[test]
fn rest_for_one_backoff_preserves_earlier_children() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let mut timers = VmTimerTable::default();
    let mut scheduler = VmScheduler::default();
    let mut backoff = VmSupervisionBackoffQueue::default();
    let supervisor = supervision.create_supervisor_with_policy("root", VmRestartPolicy::RestForOne);
    let earlier = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("earlier", source("earlier"), 3)
                .with_restart_backoff(VmRestartBackoffSchedule::exponential(2, 8)),
        )
        .expect("earlier");
    let failed = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("failed", source("failed"), 3)
                .with_restart_backoff(VmRestartBackoffSchedule::exponential(4, 16)),
        )
        .expect("failed");
    let later = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("later", source("later"), 3)
                .with_restart_backoff(VmRestartBackoffSchedule::exponential(8, 32)),
        )
        .expect("later");

    let VmSupervisionBackoffStart::Deferred { scheduled, .. } = backoff
        .schedule_restart(
            &mut supervision,
            &mut timers,
            &mut processes,
            VmSupervisionRestartRequest::new(supervisor, "failed", VmExitReason::Killed, 20),
        )
        .expect("schedule rest-for-one")
    else {
        panic!("rest-for-one should defer selected children");
    };
    assert_eq!(scheduled.len(), 2);
    assert!(matches!(
        processes.get(earlier).map(|process| &process.state),
        Some(VmProcessState::Runnable)
    ));
    assert_exited(&processes, failed);
    assert_exited(&processes, later);

    for tick in [24, 28] {
        let event = timers
            .advance_clock(&mut processes, &mut scheduler, tick)
            .remove(0);
        assert!(matches!(
            backoff
                .handle_timer_event(&mut supervision, &mut processes, &event)
                .expect("handle timer"),
            Some(VmSupervisionBackoffCompletion::Restarted(_))
        ));
    }
    assert_eq!(child_pid(&supervision, supervisor, "earlier"), earlier);
    assert_ne!(child_pid(&supervision, supervisor, "failed"), failed);
    assert_ne!(child_pid(&supervision, supervisor, "later"), later);
}

#[test]
fn group_backoff_restarts_zero_delay_child_before_delayed_sibling() {
    let mut processes = VmProcessTable::default();
    let mut supervision = VmSupervisionSystem::default();
    let mut timers = VmTimerTable::default();
    let mut backoff = VmSupervisionBackoffQueue::default();
    let supervisor = supervision.create_supervisor_with_policy("root", VmRestartPolicy::OneForAll);
    let immediate = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("immediate", source("immediate"), 3),
        )
        .expect("immediate");
    let delayed = supervision
        .start_child(
            &mut processes,
            supervisor,
            VmChildSpec::new("delayed", source("delayed"), 3)
                .with_restart_backoff(VmRestartBackoffSchedule::exponential(7, 28)),
        )
        .expect("delayed");

    let VmSupervisionBackoffStart::Deferred {
        restarted_immediately,
        scheduled,
    } = backoff
        .schedule_restart(
            &mut supervision,
            &mut timers,
            &mut processes,
            VmSupervisionRestartRequest::new(supervisor, "immediate", VmExitReason::Killed, 10),
        )
        .expect("schedule mixed group")
    else {
        panic!("one delayed sibling should defer the group");
    };
    assert_eq!(restarted_immediately.len(), 1);
    assert_eq!(scheduled.len(), 1);
    assert_eq!(scheduled[0].child_id, "delayed");
    assert_ne!(child_pid(&supervision, supervisor, "immediate"), immediate);
    assert_exited(&processes, delayed);
}

#[test]
fn group_backoff_preflight_is_atomic_for_limit_and_deadline_overflow() {
    for now_tick in [0, u64::MAX] {
        let mut processes = VmProcessTable::default();
        let mut supervision = VmSupervisionSystem::default();
        let mut timers = VmTimerTable::default();
        let mut backoff = VmSupervisionBackoffQueue::default();
        let supervisor =
            supervision.create_supervisor_with_policy("root", VmRestartPolicy::OneForAll);
        let alpha = supervision
            .start_child(
                &mut processes,
                supervisor,
                VmChildSpec::new("alpha", source("alpha"), 3)
                    .with_restart_backoff(VmRestartBackoffSchedule::exponential(5, 20)),
            )
            .expect("alpha");
        let beta_limit = if now_tick == 0 { 0 } else { 3 };
        let beta = supervision
            .start_child(
                &mut processes,
                supervisor,
                VmChildSpec::new("beta", source("beta"), beta_limit)
                    .with_restart_backoff(VmRestartBackoffSchedule::exponential(10, 40)),
            )
            .expect("beta");

        if now_tick == 0 {
            assert!(matches!(
                backoff
                    .schedule_restart(
                        &mut supervision,
                        &mut timers,
                        &mut processes,
                        VmSupervisionRestartRequest::new(
                            supervisor,
                            "alpha",
                            VmExitReason::Killed,
                            now_tick,
                        ),
                    )
                    .expect("limit outcome"),
                VmSupervisionBackoffStart::Immediate(VmSupervisionRestart::LimitReached {
                    pid,
                    ..
                }) if pid == beta
            ));
        } else {
            let error = backoff
                .schedule_restart(
                    &mut supervision,
                    &mut timers,
                    &mut processes,
                    VmSupervisionRestartRequest::new(
                        supervisor,
                        "alpha",
                        VmExitReason::Killed,
                        now_tick,
                    ),
                )
                .expect_err("deadline overflow");
            assert!(error.contains("deadline overflow"));
        }
        assert!(timers.snapshots().is_empty());
        assert_eq!(backoff.pending_len(), 0);
        for pid in [alpha, beta] {
            assert!(matches!(
                processes.get(pid).map(|process| &process.state),
                Some(VmProcessState::Runnable)
            ));
        }
    }
}
