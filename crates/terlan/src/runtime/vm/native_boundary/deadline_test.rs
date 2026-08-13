use super::*;
use crate::runtime::vm::{
    process::{VmExitReason, VmProcessSource},
    scheduler::VmSchedulerConfig,
};

fn request_id(value: u64) -> RequestId {
    RequestId { value }
}

fn runtime() -> (VmProcessTable, VmScheduler, VmTimerTable, VmProcessId) {
    let mut processes = VmProcessTable::default();
    let owner = processes.spawn_root(VmProcessSource::new("app.Native", "call", 0));
    (
        processes,
        VmScheduler::new(VmSchedulerConfig::new(10, 100)),
        VmTimerTable::default(),
        owner,
    )
}

fn reply_code(reply: &NativeBoundaryReplyTerm) -> &str {
    match reply {
        NativeBoundaryReplyTerm::Error { code, .. } => code,
        NativeBoundaryReplyTerm::Ok(_) => panic!("expected NativeBoundary error reply"),
    }
}

#[test]
fn native_boundary_deadline_parks_actor_and_completion_wakes_it_once() {
    let (mut processes, mut scheduler, mut timers, owner) = runtime();
    let mut queue = VmNativeBoundaryDeadlineQueue::new(1);
    let scheduled = queue
        .start(
            &mut timers,
            &mut processes,
            &mut scheduler,
            VmNativeBoundaryDeadlineStart::new(owner, request_id(1), 10, 5),
        )
        .expect("park request");

    assert_eq!(scheduled.deadline_tick, 15);
    assert_eq!(
        processes.get(owner).expect("owner").state,
        VmProcessState::Blocked
    );
    assert_eq!(queue.pending_len(), 1);
    assert_eq!(queue.reserved_credits(), 1);
    assert_eq!(
        queue
            .complete(
                &mut timers,
                &mut processes,
                &mut scheduler,
                scheduled.timer_id,
            )
            .expect("complete request"),
        VmNativeBoundaryDeadlineCompletion::Completed {
            timer_id: scheduled.timer_id,
            request_id: request_id(1),
        }
    );
    assert_eq!(
        processes.get(owner).expect("owner").state,
        VmProcessState::Runnable
    );
    assert_eq!(scheduler.queued_len(), 1);
    assert_eq!(queue.pending_len(), 0);
    assert_eq!(queue.reserved_credits(), 0);
    assert!(timers.snapshots().is_empty());
}

#[test]
fn native_boundary_deadline_charges_only_successful_parks_to_scheduler() {
    let (mut processes, mut scheduler, mut timers, owner) = runtime();
    let rejected_owner = processes.spawn_root(VmProcessSource::new("app.Native", "rejected", 0));
    let mut queue = VmNativeBoundaryDeadlineQueue::new(1);

    queue
        .start(
            &mut timers,
            &mut processes,
            &mut scheduler,
            VmNativeBoundaryDeadlineStart::new(owner, request_id(1), 0, 10),
        )
        .expect("successful park");
    assert!(queue
        .start(
            &mut timers,
            &mut processes,
            &mut scheduler,
            VmNativeBoundaryDeadlineStart::new(rejected_owner, request_id(2), 0, 10),
        )
        .expect_err("backpressured park")
        .contains("native_boundary.backpressure_limit"));

    assert_eq!(processes.get(owner).expect("owner").reductions, 1);
    assert_eq!(
        processes
            .get(rejected_owner)
            .expect("rejected owner")
            .reductions,
        0
    );
    assert_eq!(scheduler.metrics().total_reductions, 1);
    assert_eq!(scheduler.metrics().processes[&owner.as_u64()].reductions, 1);
    assert_eq!(scheduler.total_memory_reductions(), 0);
}

#[test]
fn native_boundary_deadline_timeout_wakes_actor_and_rejects_late_completion() {
    let (mut processes, mut scheduler, mut timers, owner) = runtime();
    let mut queue = VmNativeBoundaryDeadlineQueue::new(1);
    let scheduled = queue
        .start(
            &mut timers,
            &mut processes,
            &mut scheduler,
            VmNativeBoundaryDeadlineStart::new(owner, request_id(1), 0, 5),
        )
        .expect("park request");
    let events = timers.advance_clock(&mut processes, &mut scheduler, 5);
    let completion = queue
        .handle_timer_event(&mut processes, &mut scheduler, &events[0])
        .expect("handle timeout")
        .expect("owned event");
    let VmNativeBoundaryDeadlineCompletion::TimedOut {
        timer_id,
        request_id: timed_out_request,
        reply,
    } = completion
    else {
        panic!("expected timeout completion");
    };
    assert_eq!(timer_id, scheduled.timer_id);
    assert_eq!(timed_out_request, request_id(1));
    assert_eq!(reply_code(&reply), "native_boundary.timeout");
    assert_eq!(
        processes.get(owner).expect("owner").state,
        VmProcessState::Runnable
    );
    assert_eq!(queue.pending_len(), 0);
    assert_eq!(queue.reserved_credits(), 0);
    assert_eq!(
        queue
            .complete(
                &mut timers,
                &mut processes,
                &mut scheduler,
                scheduled.timer_id,
            )
            .expect_err("late completion"),
        format!(
            "missing pending NativeBoundary request for timer {}",
            scheduled.timer_id.as_u64()
        )
    );
}

#[test]
fn native_boundary_deadline_delivery_wins_completion_race_before_event_handling() {
    let (mut processes, mut scheduler, mut timers, owner) = runtime();
    let mut queue = VmNativeBoundaryDeadlineQueue::new(1);
    let scheduled = queue
        .start(
            &mut timers,
            &mut processes,
            &mut scheduler,
            VmNativeBoundaryDeadlineStart::new(owner, request_id(1), 0, 1),
        )
        .expect("park request");
    let events = timers.advance_clock(&mut processes, &mut scheduler, 1);

    assert_eq!(
        queue
            .complete(
                &mut timers,
                &mut processes,
                &mut scheduler,
                scheduled.timer_id,
            )
            .expect_err("deadline delivery wins"),
        format!(
            "NativeBoundary timer {} no longer owns completion: missing timer {}",
            scheduled.timer_id.as_u64(),
            scheduled.timer_id.as_u64()
        )
    );
    assert_eq!(queue.pending_len(), 1);
    assert!(matches!(
        queue
            .handle_timer_event(&mut processes, &mut scheduler, &events[0])
            .expect("handle delivered timeout"),
        Some(VmNativeBoundaryDeadlineCompletion::TimedOut { .. })
    ));
    assert_eq!(queue.pending_len(), 0);
}

#[test]
fn native_boundary_deadline_manual_cancel_releases_credit_and_wakes_actor() {
    let (mut processes, mut scheduler, mut timers, owner) = runtime();
    let mut queue = VmNativeBoundaryDeadlineQueue::new(1);
    let scheduled = queue
        .start(
            &mut timers,
            &mut processes,
            &mut scheduler,
            VmNativeBoundaryDeadlineStart::new(owner, request_id(1), 0, 10),
        )
        .expect("park request");
    let completion = queue
        .cancel(
            &mut timers,
            &mut processes,
            &mut scheduler,
            scheduled.timer_id,
        )
        .expect("cancel request");
    let VmNativeBoundaryDeadlineCompletion::Cancelled { reply, .. } = completion else {
        panic!("expected cancellation completion");
    };

    assert_eq!(reply_code(&reply), "native_boundary.cancelled");
    assert_eq!(
        processes.get(owner).expect("owner").state,
        VmProcessState::Runnable
    );
    assert_eq!(queue.pending_len(), 0);
    assert_eq!(queue.reserved_credits(), 0);
}

#[test]
fn native_boundary_deadline_owner_exit_cleans_worker_without_waking_dead_actor() {
    let (mut processes, mut scheduler, mut timers, owner) = runtime();
    let mut queue = VmNativeBoundaryDeadlineQueue::new(1);
    let scheduled = queue
        .start(
            &mut timers,
            &mut processes,
            &mut scheduler,
            VmNativeBoundaryDeadlineStart::new(owner, request_id(1), 0, 10),
        )
        .expect("park request");
    processes
        .exit_process(owner, VmExitReason::Killed)
        .expect("exit owner");
    assert_eq!(
        queue
            .cancel(
                &mut timers,
                &mut processes,
                &mut scheduler,
                scheduled.timer_id,
            )
            .expect_err("manual cancel cannot consume owner-exit timer"),
        format!(
            "NativeBoundary process {} is no longer parked",
            owner.as_u64()
        )
    );
    assert_eq!(queue.pending_len(), 1);
    assert_eq!(queue.reserved_credits(), 1);
    assert_eq!(timers.snapshots().len(), 1);
    let event = timers
        .cancel_owner_timers(owner)
        .into_iter()
        .next()
        .expect("owner exit timer");
    let completion = queue
        .handle_timer_event(&mut processes, &mut scheduler, &event)
        .expect("handle owner exit")
        .expect("owned event");
    let VmNativeBoundaryDeadlineCompletion::OwnerExited {
        timer_id, reply, ..
    } = completion
    else {
        panic!("expected owner-exit completion");
    };

    assert_eq!(timer_id, scheduled.timer_id);
    assert_eq!(reply_code(&reply), "native_boundary.cancelled");
    assert!(matches!(
        processes.get(owner).expect("owner").state,
        VmProcessState::Exited(_)
    ));
    assert_eq!(scheduler.queued_len(), 0);
    assert_eq!(queue.pending_len(), 0);
    assert_eq!(queue.reserved_credits(), 0);
}

#[test]
fn native_boundary_deadline_rejects_invalid_start_and_backpressure_atomically() {
    let (mut processes, mut scheduler, mut timers, owner) = runtime();
    let second = processes.spawn_root(VmProcessSource::new("app.Native", "second", 0));
    let mut queue = VmNativeBoundaryDeadlineQueue::new(1);
    assert_eq!(
        queue
            .start(
                &mut timers,
                &mut processes,
                &mut scheduler,
                VmNativeBoundaryDeadlineStart::new(owner, request_id(1), 0, 0),
            )
            .expect_err("zero timeout"),
        "NativeBoundary timeout must be positive"
    );
    assert_eq!(
        queue
            .start(
                &mut timers,
                &mut processes,
                &mut scheduler,
                VmNativeBoundaryDeadlineStart::new(owner, request_id(1), u64::MAX, 1),
            )
            .expect_err("overflow"),
        "NativeBoundary deadline overflow"
    );
    queue
        .start(
            &mut timers,
            &mut processes,
            &mut scheduler,
            VmNativeBoundaryDeadlineStart::new(owner, request_id(1), 0, 10),
        )
        .expect("first request");
    let error = queue
        .start(
            &mut timers,
            &mut processes,
            &mut scheduler,
            VmNativeBoundaryDeadlineStart::new(second, request_id(2), 0, 10),
        )
        .expect_err("worker backpressure");

    assert!(error.contains("native_boundary.backpressure_limit"));
    assert_eq!(
        processes.get(second).expect("second").state,
        VmProcessState::Runnable
    );
    assert_eq!(queue.pending_len(), 1);
    assert_eq!(queue.reserved_credits(), 1);
    assert_eq!(timers.snapshots().len(), 1);
}

#[test]
fn native_boundary_deadline_rejects_foreign_and_invalid_events_without_mutation() {
    let (mut processes, mut scheduler, mut timers, owner) = runtime();
    let foreign = processes.spawn_root(VmProcessSource::new("app.Native", "foreign", 0));
    let mut queue = VmNativeBoundaryDeadlineQueue::new(1);
    let scheduled = queue
        .start(
            &mut timers,
            &mut processes,
            &mut scheduler,
            VmNativeBoundaryDeadlineStart::new(owner, request_id(1), 0, 10),
        )
        .expect("park request");
    let foreign_event = VmTimerEvent::Fired {
        timer_id: scheduled.timer_id,
        owner: foreign,
        kind: VmTimerKind::OneShot,
    };
    assert!(queue
        .handle_timer_event(&mut processes, &mut scheduler, &foreign_event)
        .expect_err("foreign event")
        .contains("owner mismatch"));
    let interval_event = VmTimerEvent::Fired {
        timer_id: scheduled.timer_id,
        owner,
        kind: VmTimerKind::Interval,
    };
    assert!(queue
        .handle_timer_event(&mut processes, &mut scheduler, &interval_event)
        .expect_err("invalid kind")
        .contains("invalid deadline outcome"));
    assert_eq!(queue.pending_len(), 1);
    assert_eq!(queue.reserved_credits(), 1);
    assert_eq!(
        processes.get(owner).expect("owner").state,
        VmProcessState::Blocked
    );
}
